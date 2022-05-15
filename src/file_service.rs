use actix_web::http::header::ContentType;
use actix_web::{web, HttpRequest, HttpResponse, Responder};
use actix_web_httpauth::extractors::basic::BasicAuth;
use handlebars::Handlebars;
use serde::Serialize;
use simu::{DirectoryEntry, ReturnCode};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info};

use crate::error::SimuError;

pub async fn serve_files(auth: BasicAuth, req: HttpRequest) -> impl Responder {
    info!("request to default; {}", req.path());
    if auth.password().is_none() {
        return HttpResponse::Unauthorized().finish();
    }
    let filepath = req
        .path()
        .strip_prefix('/')
        .expect("path must start with forward-slash");
    info!("considering path {}", filepath);

    let resp = if filepath.ends_with('/') {
        // todo proper path sep ref
        serve_dir(auth, &req, filepath).await
    } else if filepath.is_empty() {
        serve_dir(auth, &req, "./").await // temporary workaround
    } else {
        serve_file(auth, filepath).await
    };

    match resp {
        Err(err) => match err.code {
            ReturnCode::FileNotFound => HttpResponse::NotFound().finish(),
            ReturnCode::LoginFailed => HttpResponse::Unauthorized().finish(),
            ReturnCode::PermissionDenied => HttpResponse::Forbidden().finish(),
            ReturnCode::UnexpectedType => HttpResponse::Found()
                .append_header(("Location", format!("/{}/",filepath)))
                .finish(),
            _ => HttpResponse::InternalServerError().finish(),
        },
        Ok(res) => res,
    }
}

async fn serve_file(auth: BasicAuth, filepath: &str) -> Result<HttpResponse, SimuError> {
    let res = crate::helper::run_file(
        auth.user_id(),
        auth.password().expect("Password missing"),
        filepath,
    )
    .await?;

    let stream = ReceiverStream::new(res);
    Ok(HttpResponse::Ok().streaming::<_, crate::error::SimuError>(stream))
}

async fn serve_dir(
    auth: BasicAuth,
    req: &HttpRequest,
    dirpath: &str,
) -> Result<HttpResponse, SimuError> {
    let dir = crate::helper::run_dir(
        auth.user_id(),
        auth.password().expect("Password missing"),
        dirpath,
    )
    .await?;

    let hb = req.app_data::<web::Data<Handlebars>>().map(|h| h.get_ref());
    if hb.is_none() {
        error!("No Handlebars instance found! This is a bug!");
        return Err(SimuError::unknown());
    }

    #[derive(Serialize)]
    struct Dir<'a> {
        path: &'a str,
        entries: &'a [DirectoryEntry],
    }

    let body = hb.unwrap().render(
        "directory",
        &Dir {
            path: dirpath,
            entries: &dir.0,
        },
    );
    match body {
        Ok(body) => Ok(HttpResponse::Ok()
            .content_type(ContentType::html())
            .body(body)),
        Err(err) => {
            error!("Failed to apply error template! {}", err);
            Err(SimuError::unknown())
        }
    }
}
