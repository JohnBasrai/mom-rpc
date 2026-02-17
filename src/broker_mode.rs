//! Broker mode enumeration.
//!
//! Defines the operational mode of an [`RpcBroker`](crate::RpcBroker) based on
//! which queues it subscribes to.

/// Operational mode of an RPC broker.
///
/// The mode determines which operations are valid:
/// - [`Client`](BrokerMode::Client): Can call `request_to()`, cannot `register()` handlers
/// - [`Server`](BrokerMode::Server): Can `register()` handlers, cannot call `request_to()`
/// - [`FullDuplex`](BrokerMode::FullDuplex): Can both `register()` and `request_to()`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerMode {
    /// Client mode - subscribes to response queue, publishes to request queues.
    ///
    /// Allows: `request_to()`  
    /// Forbids: `register()`, `spawn()`, `run()`
    Client,

    /// Server mode - subscribes to request queue, publishes to response queues.
    ///
    /// Allows: `register()`, `spawn()`, `run()`  
    /// Forbids: `request_to()`
    Server,

    /// Full-duplex mode - subscribes to both request and response queues.
    ///
    /// Allows: All operations (`request_to()`, `register()`, `spawn()`, `run()`)
    FullDuplex,
}
