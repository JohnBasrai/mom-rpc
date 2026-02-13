//! In-memory transport implementation.
//!
//! This module provides a pure in-process implementation of the domain-level
//! `Transport` trait. It is intended primarily for testing, local execution,
//! and as a reference for transport semantics.
//!
//! ## Reference Semantics
//!
//! The in-memory transport defines the **reference behavior** for the transport
//! layer. All other transport implementations are expected to approximate this
//! behavior as closely as their underlying systems allow and to document any
//! unavoidable deviations.
//!
//! In particular, the in-memory transport establishes the following expectations:
//!
//! - Once `subscribe()` returns successfully, messages published *after* that
//!   point and matching the subscription are deliverable.
//! - Message delivery is deterministic within a single process.
//! - No messages are dropped due to timing, scheduling, or background IO.
//!
//! ## Non-Goals
//!
//! This transport does not attempt to emulate the failure modes, persistence,
//! or delivery guarantees of any specific broker. It exists to provide a clear,
//! deterministic baseline against which higher-level behavior can be validated.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};

#[allow(unused_imports)]
use crate::{
    // ---
    log_debug,
    log_error,
    log_info,
    log_warn,
    Address,
    Envelope,
    Result,
    RpcConfig,
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
/// - Subscriptions remain active until the transport is closed.
///
/// ## Non-Goals
///
/// - Persistence or durability
/// - Network behavior or failure simulation
/// - Exact emulation of MQTT, AMQP, or other broker semantics
struct MemoryTransport {
    // ---
    subscriptions: RwLock<HashMap<Subscription, Vec<mpsc::Sender<Envelope>>>>,
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
    async fn publish(&self, env: Envelope) -> Result<()> {
        // ---
        let subs = self.subscriptions.read().await;

        for (sub, senders) in subs.iter() {
            if sub.0 == env.address.0 {
                log_debug!("{}: publish to {sub:?}", self.transport_id());

                for sender in senders {
                    // Ignore send failures; a closed channel indicates
                    // a dropped SubscriptionHandle.
                    match sender.send(env.clone()).await {
                        Ok(_) => {}
                        Err(_err) => {
                            log_info!("publish error {_err:?}");
                        }
                    }
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
    async fn subscribe(&self, sub: Subscription) -> Result<SubscriptionHandle> {
        // ---

        log_debug!("{}: subscribe to {sub:?}", self.transport_id());

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

        log_debug!("{}: closing transport...", self.transport_id());

        let mut subs = self.subscriptions.write().await;
        subs.clear();
        Ok(())
    }
}

/// Create a new in-memory transport.
///
/// This transport is always available and requires no external resources.
pub async fn create_transport(config: &RpcConfig) -> Result<TransportPtr> {
    // ---
    let transport_id = config.transport_id.clone();
    log_debug!("{transport_id}: create memory transport");

    let transport = MemoryTransport {
        // ---
        transport_id,
        subscriptions: RwLock::new(HashMap::new()),
    };

    Ok(Arc::new(transport))
}
