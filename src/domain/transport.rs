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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
#[derive(Clone, Debug)]
pub struct Envelope {
    // ---
    /// Delivery address used by the transport.
    ///
    /// This field is transport-specific and is used only for routing and
    /// delivery. It must not encode RPC method semantics.
    pub address: Address,

    /// Optional RPC method name.
    ///
    /// This field is present for RPC request envelopes and is used by the
    /// server to select the appropriate handler. Response envelopes do not
    /// include a method. This field MUST be `Some` for RPC request envelopes.
    /// It is always `None` for response envelopes.
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

    /// Optional content type metadata.
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

/// Options controlling message publication.
///
/// These options express delivery intent at the domain level. Concrete
/// transports may ignore, approximate, or map these options onto their
/// own mechanisms.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PublishOptions {
    /// Whether the message should be treated as durable.
    pub durable: bool,

    /// Optional time-to-live for the message, in milliseconds.
    pub ttl_ms: Option<u64>,
}

/// Options controlling subscription behavior.
///
/// These options express intent only. Not all transports are required to
/// support all options.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct SubscribeOptions {
    /// Whether the subscription should be treated as durable.
    pub durable: bool,
}

/// Handle returned from a successful subscription.
///
/// Dropping the handle indicates that the subscription is no longer needed.
/// Transport implementations may use this as a signal to unregister the
/// subscription.
pub struct SubscriptionHandle {
    /// Inbox for receiving delivered envelopes.
    pub inbox: mpsc::Receiver<Envelope>,
}

/// Transport abstraction.
///
/// A `Transport` provides best-effort delivery of message envelopes between
/// producers and subscribers, with stronger semantics provided by higher
/// layers. It defines the minimal contract required by higher-level layers
/// without committing to any specific protocol or broker. Next higer-level
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
#[async_trait::async_trait]
#[allow(dead_code)]
pub trait Transport: Send + Sync {
    // ---
    /// Publish an envelope to the given address.
    async fn publish(&self, env: Envelope, opts: PublishOptions) -> Result<()>;

    /// Register a subscription and return a handle for receiving messages.
    async fn subscribe(
        &self,
        sub: Subscription,
        opts: SubscribeOptions,
    ) -> Result<SubscriptionHandle>;

    /// Close the transport and release any associated resources.
    async fn close(&self) -> Result<()>;
}

/// Shared transport pointer.
///
/// Used to erase concrete transport types behind a stable domain interface.
pub type TransportPtr = Arc<dyn Transport>;
