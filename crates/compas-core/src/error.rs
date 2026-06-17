use thiserror::Error;

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, CompasError>;

/// Errors that cross subsystem boundaries. Real-time audio code never returns
/// these from the callback (it cannot allocate); they surface on the control
/// thread during load/seek/analyze operations.
#[derive(Debug, Error)]
pub enum CompasError {
    #[error("audio device error: {0}")]
    Device(String),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("operation not supported for this source's capabilities: {0}")]
    Capability(String),

    #[error("{0}")]
    Other(String),
}
