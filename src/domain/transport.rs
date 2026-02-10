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
/// or wildcard behavior.
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
/// The subscription remains active until the transport is closed.
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
/// - [`crate::create_memory_transport`] - In-memory transport (always available)
/// - [`crate::create_transport`] - Creates a transport based on the enabled features.
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
    /// Returns the transport_id of the transport.
    fn transport_id(&self) -> &str;

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
/// - Safe to share between `RpcClient` and `RpcServer`
///
/// Used to erase concrete transport types behind a stable domain interface.
pub type TransportPtr = Arc<dyn Transport>;
