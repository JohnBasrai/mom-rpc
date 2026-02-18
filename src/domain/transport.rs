// src/domain/transport.rs

//! Transport domain abstractions.
//!
//! This module defines internal domain abstractions used by the client and
//! server layers and is not intended to be a user-facing API
//!
//! It defines the domain-level transport interface used by the client and
//! server layers to exchange messages. It intentionally avoids any reference to
//! concrete protocols, brokers, or client libraries.
//!
//! The transport layer is responsible only for delivering opaque envelopes
//! to subscribed consumers. Higher-level semantics such as RPC correlation,
//! retries, or timeouts are handled elsewhere.
//!
//! Concrete implementations of this interface live under `src/transport/`.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::mpsc;

/// Operational mode of a transport.
///
/// Determines which queues the transport subscribes to and which
/// RPC operations are valid on the resulting broker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    // ---
    /// Subscribes to response queue only. Supports `request_to()`.
    Client,

    /// Subscribes to request queue only. Supports `register_rpc_handle()`, `spawn()`, `run()`.
    Server,

    /// Subscribes to both queues. Supports all operations.
    FullDuplex,
}

/// Shared base state for all transport implementations.
///
/// Embeds common fields so that default `Transport` trait implementations
/// can delegate to this struct rather than repeating logic in each transport.
///
/// # Usage
///
/// Each concrete transport embeds this as a field named `base`:
///
/// ```ignore
/// struct MqttTransport {
///     base: TransportBase,
///     // ... mqtt specific fields
/// }
///
/// impl Transport for MqttTransport {
///     fn base(&self) -> &TransportBase { &self.base }
/// }
/// ```
pub struct TransportBase {
    /// Unique identifier for this transport instance (the node_id).
    pub transport_id: String,
    /// Operational mode of this transport.
    pub mode: TransportMode,
    /// Request queue name (set for Server and FullDuplex modes).
    pub request_queue: Option<String>,
    /// Response queue name (set for Client and FullDuplex modes).
    pub response_queue: Option<String>,
}

impl TransportBase {
    /// Create a new TransportBase.
    pub fn new(
        transport_id: impl Into<String>,
        mode: TransportMode,
        request_queue: Option<String>,
        response_queue: Option<String>,
    ) -> Self {
        Self {
            transport_id: transport_id.into(),
            mode,
            request_queue,
            response_queue,
        }
    }
}

impl From<&TransportConfig> for TransportBase {
    /// Construct a `TransportBase` from a `TransportConfig` reference.
    ///
    /// Clones only the fields needed by `TransportBase`, leaving `config`
    /// available for transport-specific use (URI, keep-alive, etc.).
    fn from(config: &TransportConfig) -> Self {
        // ---
        Self {
            transport_id: config.node_id.clone(),
            mode: config.mode,
            request_queue: config.request_queue.clone(),
            response_queue: config.response_queue.clone(),
        }
    }
}

/// Configuration for creating a transport instance.
///
/// Passed to transport factory functions (`create_*_transport()`).
#[derive(Clone, Debug)]
pub struct TransportConfig {
    /// Broker URI (e.g. `"mqtt://localhost:1883"`, `"amqp://localhost:5672/%2f"`)
    pub uri: String,
    /// Node ID for this transport instance.
    pub node_id: String,
    /// Operational mode.
    pub mode: TransportMode,
    /// Request queue name (required for Server and FullDuplex modes).
    pub request_queue: Option<String>,
    /// Response queue name (required for Client and FullDuplex modes).
    pub response_queue: Option<String>,
    /// Optional transport type override (e.g. `"rumqttc"`, `"lapin"`, `"dust-dds"`).
    /// If `None`, uses feature-flag driven selection.
    pub transport_type: Option<String>,
    /// Broker keep-alive interval in seconds.
    pub keep_alive_secs: Option<u16>,
}

/// A transport address.
///
/// An `Address` represents a destination to which messages may be published.
/// Its interpretation is transport-specific (e.g. MQTT topic, AMQP routing
/// key, queue name), but it is treated as an opaque identifier at the domain
/// level.
///
/// Addresses are immutable, cheap to clone, and safe to share across threads.
///
/// The domain layer makes no assumptions about address syntax, hierarchy,
/// or wildcard behavior. See [`Envelope`] for an example of creating an
/// Address.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Address(pub Arc<str>);

impl<T> From<T> for Address
where
    T: Into<Arc<str>>,
{
    fn from(value: T) -> Self {
        // ---
        Address(value.into())
    }
}

/// A subscription identifier.
///
/// A `Subscription` represents a request to receive messages addressed to
/// some set of destinations. How a subscription matches an address is
/// defined by the transport implementation.
///
/// The domain layer intentionally does not prescribe wildcard syntax or
/// matching rules. Implementations are expected to document their own
/// matching behavior.
///
/// The in-memory transport provides the reference semantics for matching.
/// See [`SubscriptionHandle`] for an example of reading messages from a subscription.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Subscription(pub Arc<str>);

impl From<Address> for Subscription {
    fn from(address: Address) -> Self {
        // ---
        Subscription(address.0)
    }
}

impl<T> From<T> for Subscription
where
    T: Into<Arc<str>>,
{
    fn from(value: T) -> Self {
        // ---
        Subscription(value.into())
    }
}

/// An opaque message envelope.
///
/// An `Envelope` is the unit of transport between producers and consumers.
/// It carries a payload along with optional metadata used by higher-level
/// layers (such as RPC correlation or reply routing).
///
/// The transport layer does not interpret the payload or metadata fields;
/// it is responsible only for delivery.
///
/// # Examples
///
/// ## Creating a request envelope
///
/// ```
/// # use mom_rpc::{Envelope, Address};
/// # use bytes::Bytes;
/// # use std::sync::Arc;
/// let envelope = Envelope::request(
///     Address::from("requests/sensor-service"),
///     "read_temperature".into(),
///     Bytes::from(b"payload".to_vec()),
///     Arc::from("correlation-123"),
///     Address::from("responses/client-1"),
///     Arc::from("application/json"),
/// );
/// ```
///
/// ## Creating a response envelope
///
/// ```
/// # use mom_rpc::{Envelope, Address};
/// # use bytes::Bytes;
/// # use std::sync::Arc;
/// let envelope = Envelope::response(
///     Address::from("responses/client-1"),
///     Bytes::from(b"result".to_vec()),
///     Arc::from("correlation-123"),
///     Arc::from("application/json"),
/// );
/// ```
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Envelope {
    // ---
    /// Delivery address used by the transport.
    ///
    /// This field is transport-specific and is used only for routing and
    /// delivery. It must not encode RPC method semantics.
    pub address: Address,

    /// Method name for routing to handlers.
    ///
    /// This field MUST be `Some` for RPC request envelopes where method dispatch is
    /// required. For broadcast or pub/sub patterns that don't use method routing,
    /// this field MAY be None. It is always `None` for response envelopes.
    pub method: Option<Arc<str>>,

    /// Opaque payload bytes.
    ///
    /// The interpretation of this payload is defined by higher-level
    /// protocol logic (e.g., JSON-RPC-style request/response).
    pub payload: Bytes,

    /// Correlation identifier used to associate requests with responses.
    pub correlation_id: Option<Arc<str>>,

    /// Optional response address.
    ///
    /// When present on a request envelope, responses must be sent to this
    /// address by the server.
    pub reply_to: Option<Address>,

    /// Optional content type metadata (e.g., "application/json").
    ///
    /// This field is informational and not enforced by the RPC layer.
    /// Transports and handlers may use this for serialization decisions.
    pub content_type: Option<Arc<str>>,
}

impl Envelope {
    // ---
    /// Create a request envelope.
    ///
    /// # Arguments
    ///
    /// * `address` - Destination address (e.g., "requests/service-name")
    /// * `method` - RPC method name
    /// * `payload` - Serialized request data
    /// * `correlation_id` - Unique identifier for matching responses
    /// * `reply_to` - Address where the response should be sent
    /// * `content_type` - Payload format (typically "application/json")
    pub fn request(
        address: Address,
        method: Arc<str>,
        payload: Bytes,
        correlation_id: Arc<str>,
        reply_to: Address,
        content_type: Arc<str>,
    ) -> Self {
        // ---
        Self {
            address,
            method: Some(method),
            payload,
            correlation_id: Some(correlation_id),
            reply_to: Some(reply_to),
            content_type: Some(content_type),
        }
    }

    /// Create a response envelope.
    ///
    /// # Arguments
    ///
    /// * `address` - Destination address (from the request's reply_to field)
    /// * `payload` - Serialized response data
    /// * `correlation_id` - Correlation ID from the original request
    /// * `content_type` - Payload format (typically "application/json")
    pub fn response(
        address: Address,
        payload: Bytes,
        correlation_id: Arc<str>,
        content_type: Arc<str>,
    ) -> Self {
        // ---
        Self {
            method: None,
            address,
            payload,
            correlation_id: Some(correlation_id),
            reply_to: None,
            content_type: Some(content_type),
        }
    }
}

/// Handle returned from a successful subscription.
///
/// The subscription remains active until either:
/// - The handle is dropped (receiver channel closes)
/// - The transport is closed
///
/// Dropping the handle automatically unsubscribes from the topic.
///
/// # Example
///
/// ```no_run
/// # use mom_rpc::{TransportBuilder, Subscription};
/// # async fn example() -> mom_rpc::Result<()> {
/// let transport = TransportBuilder::new()
///     .uri("memory://")
///     .node_id("app")
///     .full_duplex()
///     .build()
///     .await?;
///
/// let subscription = Subscription::from("notifications");
/// let mut handle = transport.subscribe(subscription).await?;
///
/// // Read messages from the subscription
/// while let Some(envelope) = handle.inbox.recv().await {
///     println!("Received: {:?}", envelope);
/// }
/// # Ok(())
/// # }
/// ```
pub struct SubscriptionHandle {
    // ---
    /// Receiver channel for delivered envelopes matching this subscription.
    pub inbox: mpsc::Receiver<Envelope>,
}

/// Transport abstraction.
///
/// A `Transport` provides best-effort delivery of message envelopes between
/// producers and subscribers, with stronger semantics provided by higher
/// layers. It defines the minimal contract required by higher-level layers
/// without committing to any specific protocol or broker. Next higher-level
/// provides correlation, retries, timeouts.
///
/// Implementations must ensure that:
/// - Once `subscribe()` returns successfully, messages published *after* that
///   point and matching the subscription are deliverable.
/// - `publish()` is non-blocking with respect to subscribers.
/// - No domain-level assumptions are made about ordering, durability, or
///   retries beyond what is explicitly documented.
///
/// The in-memory transport serves as the reference implementation of these
/// semantics.
///
/// # Available Implementations
///
/// - `create_memory_transport` - In-memory transport (always available)
///
/// # Notes
///
/// This trait uses `async_trait`; the expanded documentation may show explicit
/// lifetimes and a boxed `Future`. This is an implementation detail — consumers
/// should treat methods as normal `async fn`s.
#[async_trait::async_trait]
#[allow(dead_code)]
pub trait Transport: Send + Sync {
    // ---
    /// Returns a reference to the shared base state.
    ///
    /// Required method - each concrete transport must implement this
    /// by returning `&self.base`.
    fn base(&self) -> &TransportBase;

    /// Returns the transport_id of the transport.
    ///
    /// Default implementation delegates to `base()`.
    fn transport_id(&self) -> &str {
        &self.base().transport_id
    }

    /// Returns the operational mode of the transport.
    ///
    /// Default implementation delegates to `base()`.
    fn mode(&self) -> TransportMode {
        self.base().mode
    }

    /// Publish an envelope to the given address.
    ///
    /// RPC semantics always use non-durable delivery with no TTL.
    /// Transports implement their own default QoS settings for RPC patterns.
    async fn publish(&self, env: Envelope) -> Result<()>;

    /// Register a subscription and return a handle for receiving messages.
    async fn subscribe(&self, sub: Subscription) -> Result<SubscriptionHandle>;

    /// Close the transport and release any associated resources.
    async fn close(&self) -> Result<()>;
}

/// Shared transport pointer.
///
/// This is an `Arc<dyn Transport>`, which means:
/// - `.clone()` is cheap (only increments a reference count)
/// - Multiple clones share the same underlying connection
/// - Used to erase concrete transport types behind a stable domain interface.
pub type TransportPtr = Arc<dyn Transport>;
