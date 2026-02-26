use std::fmt;

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
