// std::error::Error is a trait, not a concrete type you can instantiate with new().
// For that you define your own error type (often an enum) and implement the std::error::Error trait for it.

#[derive(Debug)]
pub enum RqError { // In the future, define more different error variants as needed
    Config(String), // Configuration-related errors with a message
    Io(std::io::Error), // Wraps std::io::Error for IO-related issues
}

impl std::error::Error for RqError {} // this is the key: our own error type implements the std::error::Error trait

impl std::fmt::Display for RqError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RqError::Config(msg) => write!(f, "Configuration error: {}", msg),
            RqError::Io(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl From<std::io::Error> for RqError {
    fn from(err: std::io::Error) -> Self {
        RqError::Io(err)
    }
}