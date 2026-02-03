//! Public, transport-agnostic RPC configuration.
//!
//! This type intentionally contains no transport-specific concepts
//! (e.g. MQTT client options). Transport layers are responsible for
//! interpreting this config into concrete connection settings.

#[derive(Debug, Clone)]
pub struct RpcConfig {
    /// Broker address, e.g. `mqtt://localhost:1883`
    pub broker_addr: String,

    /// Keep-alive interval in seconds.
    ///
    /// If `None`, a sensible transport default is used.
    pub keep_alive_secs: Option<u16>,

    /// The transport_id used by the transport end point.
    pub transport_id: String,
}

impl RpcConfig {
    /// Create a new `RpcConfig` with the given broker address.
    ///
    /// Keep-alive will use the transport default.
    pub fn with_broker(broker_addr: impl Into<String>, transport_id: impl Into<String>) -> Self {
        Self {
            broker_addr: broker_addr.into(),
            keep_alive_secs: None,
            transport_id: transport_id.into(),
        }
    }

    /// Create a memory transport config (no broker).
    pub fn memory(transport_id: impl Into<String>) -> Self {
        Self {
            broker_addr: String::new(),
            keep_alive_secs: None,
            transport_id: transport_id.into(),
        }
    }

    /// Set an explicit keep-alive interval.
    pub fn with_keep_alive_secs(mut self, secs: u16) -> Self {
        self.keep_alive_secs = Some(secs);
        self
    }
}
