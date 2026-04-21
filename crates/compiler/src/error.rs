use std::fmt;

#[derive(Debug)]
pub enum CompileError {
    Unsupported(String),
    WatParse(String),
    InvalidInput(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::Unsupported(m) => write!(f, "unsupported: {m}"),
            CompileError::WatParse(m) => write!(f, "wat parse failed: {m}"),
            CompileError::InvalidInput(m) => write!(f, "invalid input: {m}"),
        }
    }
}

impl std::error::Error for CompileError {}
