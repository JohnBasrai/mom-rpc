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

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

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
    subscriptions: RwLock<HashMap<Subscription, Vec<mpsc::Sender<Envelope>>>>,
}

#[async_trait::async_trait]
impl Transport for MemoryTransport {
    // ---

    /// Publish an envelope to all matching subscriptions.
    ///
    /// Matching semantics are intentionally simple: a subscription matches
    /// an address if their underlying string values are exactly equal.
    ///
    /// This behavior defines the reference matching semantics for the
    /// transport layer.
    async fn publish(&self, env: Envelope, _opts: PublishOptions) -> Result<()> {
        // ---
        let subs = self.subscriptions.read().await;

        for (sub, senders) in subs.iter() {
            if sub.0 == env.address.0 {
                for sender in senders {
                    // Ignore send failures; a closed channel indicates
                    // a dropped SubscriptionHandle.
                    let _ = sender.send(env.clone()).await;
                }
            }
        }

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

        let (tx, rx) = mpsc::channel(16);

        let mut subs = self.subscriptions.write().await;
        subs.entry(sub).or_insert_with(Vec::new).push(tx);

        Ok(SubscriptionHandle { inbox: rx })
    }

    /// Close the transport.
    ///
    /// For the in-memory transport, this clears all subscriptions.
    async fn close(&self) -> Result<()> {
        // ---

        let mut subs = self.subscriptions.write().await;
        subs.clear();
        Ok(())
    }
}

/// Create a new in-memory transport.
///
/// This transport is always available and requires no external resources.
pub async fn create_transport() -> Result<TransportPtr> {
    // ---

    let transport = MemoryTransport {
        // ---
        subscriptions: RwLock::new(HashMap::new()),
    };

    Ok(Arc::new(transport))
}
