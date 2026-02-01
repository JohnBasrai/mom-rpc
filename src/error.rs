use thiserror::Error;

/// Errors that can occur during RPC operations
#[derive(Error, Debug)]
pub enum Error {
    /// Request timed out waiting for response
    #[error("request timed out")]
    Timeout,

    /// MQTT connection lost
    #[error("MQTT connection lost")]
    ConnectionLost,

    /// JSON serialization or deserialization failed
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// MQTT client error
    #[error("MQTT error: {0}")]
    Mqtt(#[from] rumqttc::Error),

    /// No handler registered for the requested topic
    #[error("no handler found for topic: {0}")]
    HandlerNotFound(String),

    /// Invalid response format
    #[error("invalid response format")]
    InvalidResponse,

    /// Response topic not provided in request
    #[error("response topic not provided in request")]
    MissingResponseTopic,
}

/// Result type alias for RPC operations
pub type Result<T> = std::result::Result<T, Error>;
