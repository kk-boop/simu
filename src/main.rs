use actix_web::body::BoxBody;
use actix_web::dev::ServiceResponse;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::middleware::{ErrorHandlerResponse, ErrorHandlers};
use actix_web::{web, App, HttpResponse, HttpServer};
use actix_web_httpauth::extractors::basic::Config;
use actix_web_httpauth::middleware::HttpAuthentication;
use handlebars::Handlebars;
use serde::Serialize;
use tracing::{error, info};

mod error;
mod file_service;
mod helper;

fn err_handler<B>(res: ServiceResponse<B>) -> actix_web::Result<ErrorHandlerResponse<BoxBody>> {
    let req = res.request();
    let hb = req.app_data::<web::Data<Handlebars>>().map(|h| h.get_ref());
    let fallback_error = || {
        HttpResponse::build(res.status())
            .content_type(ContentType::plaintext())
            .body(res.status().canonical_reason().unwrap_or("ERROR"))
    };

    #[derive(Serialize)]
    struct ErrData<'a> {
        status_code: u16,
        error: &'a str,
    }

    let resp = match hb {
        Some(hb) => {
            let data = ErrData {
                status_code: res.status().as_u16(),
                error: res.status().canonical_reason().unwrap_or("ERROR"),
            };
            let body = hb.render("error", &data);
            match body {
                Ok(body) => HttpResponse::build(res.status())
                    .content_type(ContentType::html())
                    .body(body),
                Err(err) => {
                    error!("Failed to apply error template! {}", err);
                    fallback_error()
                }
            }
        }
        None => fallback_error(),
    };
    Ok(ErrorHandlerResponse::Response(ServiceResponse::new(
        res.into_parts().0,
        resp.map_into_left_body(),
    )))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let mut handlebars = Handlebars::new();
    handlebars
        .register_templates_directory(".html", get_templates_dir())
        .unwrap();
    let handlebars_ref = web::Data::new(handlebars);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(Config::default().realm("Restricted area"))
            .app_data(handlebars_ref.clone())
            .wrap(
                ErrorHandlers::new()
                    .handler(StatusCode::NOT_FOUND, err_handler)
                    .handler(StatusCode::UNAUTHORIZED, err_handler)
                    .handler(StatusCode::FORBIDDEN, err_handler)
                    .handler(StatusCode::INTERNAL_SERVER_ERROR, err_handler),
            )
            .wrap(HttpAuthentication::basic(|req, _creds| async { Ok(req) }))
            .default_service(web::route().to(file_service::serve_files))
    });
    let (proto, addr) = get_bind_uri();
    info!("Binding to {} {}", proto, addr);
    let server = if proto == "unix" {
        server.bind_uds(addr)?
    } else if proto == "tcp" {
        server.bind(addr)?
    } else {
        panic!("Unknown protocol passed to SIMU_BIND!");
    };
    server.run().await
}

fn get_templates_dir() -> std::path::PathBuf {
    std::env::var("SIMU_TEMPLATES")
        .map(|s| std::path::PathBuf::from(&s))
        .unwrap_or_else(|_| std::path::PathBuf::from("./static/templates"))
}

fn get_bind_uri() -> (String, String) {
    let var = std::env::var("SIMU_BIND").unwrap_or_else(|_| "tcp:0.0.0.0:8080".to_string());
    let (l, r) = var
        .split_once(":")
        .expect("SIMU_BIND contents are malformed! Cannot start server");
    (l.to_owned().to_ascii_lowercase(), r.to_owned())
}
