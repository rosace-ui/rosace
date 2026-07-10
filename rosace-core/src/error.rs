/// Errors that can arise within the ROSACE framework.
#[derive(Debug)]
pub enum RosaceError {
    /// A required resource could not be located.
    NotFound { resource: &'static str },
    /// The system is in an unexpected or inconsistent state.
    InvalidState(String),
    /// A layout computation failed or produced an invalid result.
    LayoutError(String),
    /// An unexpected internal error occurred.
    Internal(String),
}

impl RosaceError {
    /// Creates a `NotFound` error for the given resource name.
    pub fn not_found(resource: &'static str) -> Self {
        RosaceError::NotFound { resource }
    }

    /// Creates an `Internal` error with an arbitrary message.
    pub fn internal(msg: impl Into<String>) -> Self {
        RosaceError::Internal(msg.into())
    }
}

impl std::fmt::Display for RosaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RosaceError::NotFound { resource } => write!(f, "resource not found: {resource}"),
            RosaceError::InvalidState(msg) => write!(f, "invalid state: {msg}"),
            RosaceError::LayoutError(msg) => write!(f, "layout error: {msg}"),
            RosaceError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for RosaceError {}

/// Convenience alias for `Result<T, RosaceError>`.
pub type RosaceResult<T> = Result<T, RosaceError>;
