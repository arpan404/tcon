use std::fmt;
use crate::span::Span;

#[derive(Debug)]
pub struct TconError{
    msg: String,
}

impl TconError{ 
    /// Creates an error from a message.
    pub fn msg<M: Into<String>>(msg: M) -> Self {
        TconError {
            msg: msg.into(),
        }
    }
}
impl fmt::Display for TconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result{
        // Prefix make it easy to spot the TconError in logs.
        write!(f, "TconError: {}", self.msg)
    }
}

impl std::error::Error for TconError {}

impl From<std::io::Error> for TconError{
    fn from(e: std::io::Error) -> Self{
        /// Wrap std I/O errors in TconError for consistent error handling.
        TconError::msg(format!("I/O error: {}", e))
    }
}


/// A lowering/semantic error with a precise span.
#[derive(Debug, Clone)]
pub struct LowerError{
    pub message: String,
    pub span: Span,
}

impl LowerError{
    pub fn new(message: impl Into<String>, span: Span) -> Self{
        Self{
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for LowerError{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result{
        // fallback formatting (diagonistics use LineIndex fro line/col)
        write!(f, "{} at {}", self.message, self.span)
    }
}

impl std::error::Error for LowerError{}

pub type LowerResult<T> = Result<T, LowerError>;
