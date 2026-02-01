use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("request timed out")]
    Timeout,

    #[error("connection lost")]
    ConnectionLost,

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("transport error")]
    Transport,

    #[error("handler not found for method: {0}")]
    HandlerNotFound(String),

    #[error("invalid response")]
    InvalidResponse,

    #[error("missing response topic")]
    MissingResponseTopic,
}

// crate-internal only: transport-specific errors
#[derive(Debug)]
pub(crate) enum TransportError {
    #[allow(dead_code)]
    Client(rumqttc::ClientError),
    #[allow(dead_code)]
    Protocol(rumqttc::Error),
}

impl Error {
    pub(crate) fn from_transport(e: TransportError) -> Self {
        match e {
            // NOTE: rumqttc::ClientError does not expose a dedicated Timeout variant in 0.25.x.
            // Our API-level Timeout comes from explicit tokio::time::timeout usage.
            TransportError::Client(_) => Error::ConnectionLost,
            TransportError::Protocol(_err) => {
                #[cfg(feature = "logging")]
                log::error!("transport error: {_err}");
                Error::Transport
            }
        }
    }
}

impl From<TransportError> for Error {
    fn from(e: TransportError) -> Self {
        Error::from_transport(e)
    }
}
