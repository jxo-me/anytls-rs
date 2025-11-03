use thiserror::Error;

/// AnyTLS protocol errors
#[derive(Error, Debug)]
pub enum AnyTlsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Stream not found: {0}")]
    StreamNotFound(u32),

    #[error("Session closed")]
    SessionClosed,

    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Padding scheme error: {0}")]
    PaddingScheme(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, AnyTlsError>;

