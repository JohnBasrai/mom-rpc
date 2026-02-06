use thiserror::Error;

/// Crate-wide result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors surfaced by the RPC layer.
///
/// These errors are intentionally transport-agnostic. Concrete transport
/// implementations are responsible for mapping their internal failures into
/// one of these variants.
#[derive(Debug, Error)]
pub enum Error {
    /// A request timed out while waiting for a response.
    ///
    /// Timeouts are currently introduced explicitly by higher-level logic
    /// (e.g., `tokio::time::timeout`) rather than by transports themselves.
    #[error("request timed out")]
    Timeout,

    /// The underlying transport connection was lost.
    #[error("connection lost")]
    ConnectionLost,

    /// Serialization or deserialization failure.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// A transport-level failure that does not map to a more specific variant.
    #[error("transport error")]
    Transport,

    /// No handler was registered for the requested method.
    ///
    /// The contained `String` is the method name that was requested.
    #[error("handler not found for method: {0}")]
    HandlerNotFound(String),

    /// A response was received but could not be interpreted as a valid RPC
    /// response.
    #[error("invalid response")]
    InvalidResponse,

    /// A request was received but it did not have a method in it.
    #[error("invalid request")]
    InvalidRequest,

    /// A request was missing a response address or `reply-to` field.
    #[error("missing response topic")]
    MissingResponseTopic,
}
