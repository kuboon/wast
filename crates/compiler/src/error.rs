use std::fmt;

#[derive(Debug)]
pub enum CompileError {
    Unsupported(String),
    WatParse(String),
    InvalidInput(String),
}

impl CompileError {
    /// Full message, including any multi-line detail (e.g. truncated
    /// generated WAT/WIT) that `Display` omits for conciseness.
    pub fn detail(&self) -> &str {
        match self {
            CompileError::Unsupported(m)
            | CompileError::WatParse(m)
            | CompileError::InvalidInput(m) => m,
        }
    }
}

impl fmt::Display for CompileError {
    /// Concise one-line rendering: the variant tag plus the first line of
    /// the message. Long generated-text context (WAT/WIT dumps) lives on
    /// subsequent lines of the message and is available via `Debug` or
    /// [`CompileError::detail`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (tag, m) = match self {
            CompileError::Unsupported(m) => ("unsupported", m),
            CompileError::WatParse(m) => ("wat parse failed", m),
            CompileError::InvalidInput(m) => ("invalid input", m),
        };
        write!(f, "{tag}: {}", m.lines().next().unwrap_or(""))
    }
}

impl std::error::Error for CompileError {}
