use std::fmt;

#[derive(Debug)]
#[allow(dead_code)]
pub struct MemPalaceError(pub String);

impl fmt::Display for MemPalaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MemPalaceError {}

impl From<anyhow::Error> for MemPalaceError {
    fn from(e: anyhow::Error) -> Self {
        MemPalaceError(e.to_string())
    }
}
