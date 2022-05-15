use std::fmt::Display;

use simu::ReturnCode;

#[derive(Debug)]
pub struct SimuError {
    pub code: ReturnCode,
}

impl SimuError {
    pub fn new(code: ReturnCode) -> Self {
        Self { code }
    }
    pub fn unknown() -> Self {
        Self {
            code: ReturnCode::Unknown,
        }
    }
}

impl Display for SimuError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        fmt.write_str("SimuError()")?;
        Ok(())
    }
}

impl std::error::Error for SimuError {}
