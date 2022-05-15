use std::io::{ErrorKind, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use bytes::Bytes;
use lazy_static::lazy_static;
use simu::{Directory, ReturnCode};
use tokio::sync::mpsc;
use tokio::task;
use tracing::{debug, error, warn};

use crate::error::SimuError;

const BUFFER_SIZE: usize = 65536;

lazy_static! {
    static ref SUID_LOC: Box<Path> = {
        // Hacky, find executable's path, and assume suid helper is next to it
        // Makes using debug and release modes side-by-side more easy
        let mut path = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        path.push("simu_suid_helper");
        debug!("suid helper: {:?}", path);
        path.into_boxed_path()
    };
}

pub async fn run_file(
    usern: &str,
    passw: &str,
    path: &str,
) -> Result<mpsc::Receiver<Result<Bytes, SimuError>>, SimuError> {
    let mut input = build_partial_input(usern, passw, path);

    input.extend(b"FIL");
    input.push(0);
    run_helper(input).await
}

pub async fn run_dir(usern: &str, passw: &str, path: &str) -> Result<Directory, SimuError> {
    let mut input = build_partial_input(usern, passw, path);

    input.extend(b"DIR");
    input.push(0);

    let mut recv = run_helper(input).await?;
    let mut buf = Vec::with_capacity(BUFFER_SIZE);
    while let Some(bytes) = recv.recv().await {
        let bytes = bytes?;
        buf.extend_from_slice(&bytes[..]);
    }
    let dir = bincode::deserialize(&buf[..]);
    if dir.is_err() {
        error!("Error while deserializing helper output!");
        Err(SimuError::unknown())
    } else {
        Ok(dir.unwrap())
    }
}

async fn run_helper(input: Vec<u8>) -> Result<mpsc::Receiver<Result<Bytes, SimuError>>, SimuError> {
    // Data channel
    let (tx, rx) = mpsc::channel::<Result<Bytes, SimuError>>(16);
    // Error/success channel
    let (etx, mut erx) = mpsc::channel::<Result<(), ReturnCode>>(1);
    // Small buffer also forces bad callpath blocking issues to arise
    let _res = task::spawn_blocking(move || {
        let command = Command::new(&**SUID_LOC)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        if let Err(ref e) = command {
            if !tx.is_closed() {
                error!("Failed to open helper command! {}", e);
                etx.blocking_send(Err(ReturnCode::Unknown)).unwrap();
            }
        }
        let mut command = command.unwrap();
        let mut stdin = command.stdin.take().unwrap();
        let mut stdout = command.stdout.take().unwrap();
        let mut stderr = command.stderr.take().unwrap();
        let mut did_data_arrive = false;

        let res = stdin.write(&input);
        if let Err(e) = res {
            error!("Failed to write info to helper! {}", e);
            etx.blocking_send(Err(ReturnCode::Unknown)).unwrap();
        }
        loop {
            let mut buf = [0u8; BUFFER_SIZE];
            let res = stdout.read(&mut buf);
            match res {
                Err(e) => {
                    if e.kind() == ErrorKind::Interrupted {
                        continue;
                    }
                    error!("Failed to read stdout from the helper! {}", e);
                    tx.blocking_send(Ok(Bytes::new())).unwrap();
                }
                Ok(sz) => {
                    if sz == 0 {
                        if !tx.is_closed() {
                            tx.blocking_send(Ok(Bytes::new())).unwrap(); // file ended
                        }
                        break;
                    }
                    // If first stdout read was 0, it was closed without content,
                    // so this check needs to be after EOF check
                    if !did_data_arrive {
                        did_data_arrive = true;
                        etx.blocking_send(Ok(())).unwrap(); // Signal incoming data
                    }
                    if tx.is_closed() {
                        continue; // We do no have an audience anymore, however we cant really kill the reader either, as that will be under inaccessible permissions
                    }
                    let res = tx.blocking_send(Ok(buf[0..sz].to_vec().into()));
                    if let Err(_e) = res {
                        // Send failed, reader disconnected most likely.
                        //let res = command.kill();
                        //error!("Reader closed; kill outcome: {:?}", res);
                        warn!("HTTP client disconnected unexpectedly!");
                        std::mem::drop(stdout); // this forces the helper to crash on stdout close
                        break;
                    }
                }
            }
        }
        let mut outp = Vec::new();
        loop {
            let mut buf = [0u8; BUFFER_SIZE / 2];
            let res = stderr.read(&mut buf);
            match res {
                Err(e) => {
                    if e.kind() == ErrorKind::Interrupted {
                        continue;
                    }
                    error!("Failed to read stderr from the helper! {}", e);
                    return;
                }
                Ok(sz) => {
                    if sz == 0 {
                        break;
                    }
                    for byte in buf.iter().take(sz).copied() {
                        outp.push(byte);
                    }
                }
            }
        }
        if !outp.is_empty() {
            let errmsg = String::from_utf8(outp);
            match errmsg {
                Ok(str_) => warn!("Additional stderr output from helper: '{}'", str_),
                Err(_) => error!("Failed to parse stderr output from helper!"),
            }
        }
        let res = command.wait();
        if did_data_arrive {
            // We already sent data and communicated preliminary success,
            // so the errors are hard to make an use of here.
            return;
        }
        match res {
            Err(_) => {
                error!("Process start failed!");
                etx.blocking_send(Err(ReturnCode::Unknown)).unwrap(); // Process start failed
            }
            Ok(status) => {
                if status.code() != Some(0) {
                    etx.blocking_send(Err(ReturnCode::from(status))).unwrap();
                }
            }
        }
    });
    match erx.recv().await {
        Some(v) => match v {
            Ok(_) => Ok(rx),
            Err(rc) => Err(SimuError::new(rc)),
        },
        None => Err(SimuError::unknown()),
    }
}

fn build_partial_input(usern: &str, passw: &str, path: &str) -> Vec<u8> {
    let mut input: Vec<u8> = usern.as_bytes().to_vec();
    input.push(0);

    input.extend(passw.as_bytes().iter().copied());
    input.push(0);

    input.extend(path.as_bytes().iter().copied());
    input.push(0);
    input
}
