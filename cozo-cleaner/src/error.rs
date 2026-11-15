use std::fmt;

pub type Result<T> = std::result::Result<T, CleanError>;

/// Everything that can go wrong while configuring or running a pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanError {
    /// Invalid pipeline configuration (bad TOML, impossible parameters, bad regex…).
    Config(String),
    /// A step refers to a column that does not exist at that point of the pipeline.
    UnknownColumn { step: String, column: String },
    /// Input cannot be read or output cannot be written.
    Io(String),
    /// Input data is malformed.
    Parse(String),
}

impl fmt::Display for CleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CleanError::Config(m) => write!(f, "invalid pipeline: {m}"),
            CleanError::UnknownColumn { step, column } => {
                write!(f, "step `{step}`: unknown column `{column}`")
            }
            CleanError::Io(m) => write!(f, "i/o error: {m}"),
            CleanError::Parse(m) => write!(f, "cannot parse input: {m}"),
        }
    }
}

impl std::error::Error for CleanError {}
