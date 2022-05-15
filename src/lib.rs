use std::fs::DirEntry;
use std::process::ExitStatus;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct DirectoryEntry {
    name: String,
    is_dir: bool,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Directory(pub Vec<DirectoryEntry>);

impl From<DirEntry> for DirectoryEntry {
    fn from(dir: DirEntry) -> Self {
        let mut name = dir.file_name().to_string_lossy().to_owned().to_string();
        let is_dir = dir.file_type().unwrap().is_dir();
        if is_dir {
            name += "/";
        }
        Self { name, is_dir }
    }
}

#[derive(Debug)]
pub enum ReturnCode {
    Success = 0,

    // Errors within the system
    FileNotFound = 1,
    LoginFailed = 2,
    UnexpectedType = 3,
    PermissionDenied = 4,

    // Errors from outside
    SignalTerm = 99,
    Panic = 101,
    Unknown = 1000,
}

impl From<ExitStatus> for ReturnCode {
    fn from(code: ExitStatus) -> Self {
        match code.code().unwrap_or(1000) {
            1 => Self::FileNotFound,
            2 => Self::LoginFailed,
            3 => Self::UnexpectedType,
            4 => Self::PermissionDenied,
            101 => Self::Panic,
            99 => Self::SignalTerm,
            0 => Self::Success,
            _ => Self::Unknown,
        }
    }
}
