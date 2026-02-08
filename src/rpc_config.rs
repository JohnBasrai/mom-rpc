//! Public, transport-agnostic RPC configuration.
//!
//! This type intentionally contains no transport-specific concepts
//! (e.g. MQTT client options). Transport layers are responsible for
//! interpreting this config into concrete connection settings.

/// Transport configuration and connection parameters.
#[derive(Debug, Clone)]
pub struct RpcConfig {
    // ---
    /// Transport connection URI.
    ///
    /// For broker-based transports (MQTT, AMQP, Kafka), this specifies the
    /// broker address (e.g., "mqtt://localhost:1883", "amqp://localhost:5672/%2f").
    ///
    /// For brokerless transports (in-memory, DDS), this may be `None` or contain
    /// transport-specific configuration (e.g., DDS domain ID).
    pub transport_uri: Option<String>,

    /// Broker connection keep-alive interval in seconds (0 to disable).
    pub keep_alive_secs: Option<u16>,

    /// Unique identifier for this transport instance, used for logging.
    pub transport_id: String,

    /// Custom request queue name (AMQP only).
    ///
    /// If `None`, the queue name is derived from `transport_id`.
    pub request_queue_name: Option<String>,

    /// Custom response queue name (AMQP only).
    ///
    /// If `None`, the queue name is derived from `transport_id`.
    pub response_queue_name: Option<String>,
}

impl RpcConfig {
    /// Create a new `RpcConfig` with the given broker URI.
    ///
    /// Keep-alive will use the transport default.
    pub fn with_broker(transport_uri: impl Into<String>, transport_id: impl Into<String>) -> Self {
        Self {
            transport_uri: Some(transport_uri.into()),
            keep_alive_secs: None,
            transport_id: transport_id.into(),
            request_queue_name: None,
            response_queue_name: None,
        }
    }

    /// Create a memory transport config (no broker).
    pub fn memory(transport_id: impl Into<String>) -> Self {
        Self {
            transport_uri: None,
            keep_alive_secs: None,
            transport_id: transport_id.into(),
            request_queue_name: None,
            response_queue_name: None,
        }
    }

    /// Set an explicit keep-alive interval.
    pub fn with_keep_alive_secs(mut self, secs: u16) -> Self {
        self.keep_alive_secs = Some(secs);
        self
    }

    /// Set a custom request queue name (AMQP only).
    pub fn with_request_queue_name(mut self, name: impl Into<String>) -> Self {
        self.request_queue_name = Some(name.into());
        self
    }

    /// Set a custom response queue name (AMQP only).
    pub fn with_response_queue_name(mut self, name: impl Into<String>) -> Self {
        self.response_queue_name = Some(name.into());
        self
    }
}
