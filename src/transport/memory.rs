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
use std::sync::{Arc, OnceLock};

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
    Subscription,
    SubscriptionHandle,
    Transport,
    TransportBase,
    TransportConfig,
    TransportPtr,
};

/// Shared message bus for the in-memory transport.
///
/// Simulates a message broker within a single process. All `MemoryTransport`
/// instances that share a `MemoryHub` can publish and receive each other's
/// messages, exactly as nodes connected to a real broker would.
///
/// # ⚠️  Testing Only - Subject to Change
///
/// **This type is exposed only for `mom-rpc`'s own integration tests.**  
/// It may change or be removed in future versions without a deprecation cycle.  
/// **Production code should use [`TransportBuilder`](crate::TransportBuilder)** instead.
///
/// # Usage in Integration Tests
///
/// For integration tests that need isolation between parallel test cases,
/// construct a hub explicitly and pass it to [`create_memory_transport_with_hub`]:
///
/// ```
/// # use mom_rpc::{MemoryHub, TransportConfig, TransportMode};
/// # async fn example() -> mom_rpc::Result<()> {
/// let hub = MemoryHub::new();
///
/// let server_config = TransportConfig {
///     uri: String::new(), node_id: "server".into(), mode: TransportMode::Server,
///     request_queue: Some("requests/server".into()), response_queue: None,
///     transport_type: None, keep_alive_secs: None,
/// };
/// let client_config = TransportConfig {
///     uri: String::new(), node_id: "client".into(), mode: TransportMode::Client,
///     request_queue: None, response_queue: Some("responses/client".into()),
///     transport_type: None, keep_alive_secs: None,
/// };
///
/// let server_transport = mom_rpc::create_memory_transport_with_hub(server_config, hub.clone()).await?;
/// let client_transport = mom_rpc::create_memory_transport_with_hub(client_config, hub.clone()).await?;
/// # Ok(())
/// # }
/// ```
pub struct MemoryHub {
    // ---
    subscriptions: RwLock<HashMap<Subscription, Vec<mpsc::Sender<Envelope>>>>,
}

impl MemoryHub {
    /// Create a new, empty hub.
    pub fn new() -> Arc<Self> {
        // ---
        Arc::new(Self {
            subscriptions: RwLock::new(HashMap::new()),
        })
    }

    async fn publish(&self, _transport_id: &str, env: Envelope) -> Result<()> {
        // ---
        let subs = self.subscriptions.read().await;

        for (sub, senders) in subs.iter() {
            if sub.0 == env.address.0 {
                log_debug!("{_transport_id}: publish to {sub:?}");

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

    async fn subscribe(
        &self,
        _transport_id: &str,
        sub: Subscription,
    ) -> Result<SubscriptionHandle> {
        // ---
        log_debug!("{_transport_id}: subscribe to {sub:?}");

        let (tx, rx) = mpsc::channel(16);

        let mut subs = self.subscriptions.write().await;
        subs.entry(sub).or_insert_with(Vec::new).push(tx);

        Ok(SubscriptionHandle { inbox: rx })
    }

    async fn close(&self, _transport_id: &str) -> Result<()> {
        // ---
        log_debug!("{_transport_id}: closing transport...");

        let mut subs = self.subscriptions.write().await;
        subs.clear();
        Ok(())
    }
}

impl Default for MemoryHub {
    fn default() -> Self {
        // ---
        Self {
            subscriptions: RwLock::new(HashMap::new()),
        }
    }
}

/// Process-global hub used by [`create_memory_transport`].
static GLOBAL_HUB: OnceLock<Arc<MemoryHub>> = OnceLock::new();

fn global_hub() -> Arc<MemoryHub> {
    GLOBAL_HUB.get_or_init(MemoryHub::new).clone()
}

/// In-memory transport.
///
/// Routes messages through a shared [`MemoryHub`], simulating a message broker
/// within the process. Multiple transport instances sharing the same hub can
/// publish and receive each other's messages.
struct MemoryTransport {
    // ---
    base: TransportBase,
    hub: Arc<MemoryHub>,
}

#[async_trait::async_trait]
impl Transport for MemoryTransport {
    // ---
    fn base(&self) -> &TransportBase {
        &self.base
    }

    /// Publish an envelope to all matching subscriptions on the shared hub.
    ///
    /// Matching semantics are intentionally simple: a subscription matches
    /// an address if their underlying string values are exactly equal.
    ///
    /// This behavior defines the reference matching semantics for the
    /// transport layer.
    async fn publish(&self, env: Envelope) -> Result<()> {
        self.hub.publish(self.transport_id(), env).await
    }

    /// Register a subscription on the shared hub.
    ///
    /// Once this function returns successfully, any subsequent calls to
    /// `publish()` with matching addresses are deliverable to the returned
    /// inbox.
    async fn subscribe(&self, sub: Subscription) -> Result<SubscriptionHandle> {
        self.hub.subscribe(self.transport_id(), sub).await
    }

    /// Close the transport.
    ///
    /// Clears all subscriptions from the shared hub. Note that if other
    /// transports share the same hub, their subscriptions are also cleared.
    /// Use per-test hubs via [`create_memory_transport_with_hub`] to avoid this.
    async fn close(&self) -> Result<()> {
        self.hub.close(self.transport_id()).await
    }
}

/// Create a new in-memory transport using the process-global hub.
///
/// All transports created with this function share a single message bus,
/// matching the semantics of nodes connected to a real broker. Suitable for
/// production use and simple single-test scenarios.
///
/// For isolated parallel testing, use [`create_memory_transport_with_hub`].
///
/// # Errors
///
/// Currently infallible — always returns `Ok`.
pub async fn create_memory_transport(config: TransportConfig) -> Result<TransportPtr> {
    // ---
    create_memory_transport_with_hub(config, global_hub()).await
}

/// Create a new in-memory transport using the provided hub.
///
/// # ⚠️  Testing Only - Subject to Change
///
/// **This function is exposed only for `mom-rpc`'s own integration tests.**  
/// It may change or be removed in future versions without a deprecation cycle.  
/// **Production code should use [`TransportBuilder`](crate::TransportBuilder)** instead.
///
/// # Purpose
///
/// Allows multiple transports to share an explicitly constructed [`MemoryHub`],
/// providing isolation between test cases running in parallel.
///
/// # Errors
///
/// Currently infallible — always returns `Ok`.
pub async fn create_memory_transport_with_hub(
    config: TransportConfig,
    hub: Arc<MemoryHub>,
) -> Result<TransportPtr> {
    // ---
    log_debug!("{}: create memory transport", config.node_id);

    let transport = MemoryTransport {
        base: TransportBase::from(&config),
        hub,
    };

    Ok(Arc::new(transport))
}
