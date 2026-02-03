// src/transport/memory/transport.rs

//! In-memory transport implementation.
//!
//! This file contains the concrete implementation of the domain-level
//! `Transport` trait using in-process data structures only.
//!
//! The memory transport is the **reference implementation** of transport
//! semantics. Other transports are expected to approximate this behavior
//! as closely as their underlying systems allow and to document any
//! unavoidable deviations.

use std::sync::Arc;

#[allow(unused_imports)]
use crate::{
    // ---
    Address,
    Envelope,
    PublishOptions,
    Result,
    SubscribeOptions,
    Subscription,
    SubscriptionHandle,
    Transport,
    TransportPtr,
};

use super::SubscriptionManager;

/// In-memory transport.
///
/// This transport simulates a message broker entirely within the process.
/// It is intended for testing and for validating higher-level behavior
/// without introducing network, broker, or timing-related variability.
///
/// ## Semantics
///
/// - Subscriptions are registered immediately.
/// - Once `subscribe()` returns, subsequent matching publishes are deliverable.
/// - Message delivery is deterministic within a single process.
/// - Dropping a `SubscriptionHandle` implicitly unregisters the subscription.
///
/// ## Non-Goals
///
/// - Persistence or durability
/// - Network behavior or failure simulation
/// - Exact emulation of MQTT, AMQP, or other broker semantics
struct MemoryTransport {
    // ---
    subscriptions: SubscriptionManager,
    transport_id: String,
}

#[async_trait::async_trait]
impl Transport for MemoryTransport {
    // ---
    fn transport_id(&self) -> &str {
        self.transport_id.as_str()
    }

    /// Publish an envelope to all matching subscriptions.
    ///
    /// Matching semantics are intentionally simple: a subscription matches
    /// an address if their underlying string values are exactly equal.
    ///
    /// This behavior defines the reference matching semantics for the
    /// transport layer.
    async fn publish(&self, env: Envelope, _opts: PublishOptions) -> Result<()> {
        // ---
        self.subscriptions.fanout(&env, &self.transport_id).await;
        Ok(())
    }

    /// Register a subscription.
    ///
    /// Once this function returns successfully, any subsequent calls to
    /// `publish()` with matching addresses are deliverable to the returned
    /// inbox.
    async fn subscribe(
        &self,
        sub: Subscription,
        _opts: SubscribeOptions,
    ) -> Result<SubscriptionHandle> {
        // ---

        #[cfg(feature = "logging")]
        log::debug!("{}: subscribe to {:?}", self.transport_id(), sub);

        let rx = self.subscriptions.add(sub).await;

        Ok(SubscriptionHandle { inbox: rx })
    }

    /// Close the transport.
    ///
    /// For the in-memory transport, this clears all subscriptions.
    async fn close(&self) -> Result<()> {
        // ---

        #[cfg(feature = "logging")]
        log::debug!("{}: closing transport...", self.transport_id());

        self.subscriptions.clear().await;
        Ok(())
    }
}

/// Create a new in-memory transport.
///
/// This transport is always available and requires no external resources.
pub async fn create_transport(transport_id: &str) -> Result<TransportPtr> {
    // ---
    #[cfg(feature = "logging")]
    log::debug!("{transport_id}: create memory transport");

    let transport = MemoryTransport {
        // ---
        transport_id: transport_id.to_owned(),
        subscriptions: SubscriptionManager::new(),
    };

    Ok(Arc::new(transport))
}
