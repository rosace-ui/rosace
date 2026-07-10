/// The five states of an async atom operation.
///
/// Used to model the lifecycle of an asynchronous data fetch or mutation.
/// The hook integration (binding to an [`crate::Atom`] and a [`Future`]) is wired
/// in a later phase; this enum is the data-only definition.
#[derive(Debug, Clone)]
pub enum AsyncState<T: Clone> {
    /// No operation has been started.
    Idle,
    /// An operation is in flight; no data is available yet.
    Loading,
    /// The operation completed successfully.
    Success(T),
    /// The operation failed with an error.
    Error(AsyncError),
    /// A previous success is visible while a refresh is in flight.
    Refreshing(T),
}

/// An error produced by an async atom operation.
#[derive(Debug, Clone)]
pub struct AsyncError {
    /// Human-readable description of what went wrong.
    pub message: String,
}

impl AsyncError {
    /// Creates a new [`AsyncError`] from any type that converts into a [`String`].
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
