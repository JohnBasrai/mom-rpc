// src/client/mod.rs

//! RPC client implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::{
    // ---
    Address,
    Envelope,
    PublishOptions,
    Result,
    RpcError,
    SubscribeOptions,
    Subscription,
    TransportPtr,
};

use super::CorrelationId;

/// Acquire a mutex guard, intentionally ignoring poisoning.
///
/// Mutex poisoning indicates that another task panicked while holding the lock.
/// The protected state here is a best-effort pending-response map
/// (correlation_id → oneshot sender).
///
/// Ignoring poisoning is acceptable because:
/// - There are no invariants spanning multiple fields.
/// - The worst outcome is a dropped or unmatched response.
/// - Connection-level failures are handled by the transport receive loop.
///
/// This avoids propagating non-`Send` poison errors across async boundaries.
fn lock_ignore_poison<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // ---
    match m.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Running RPC client instance.
///
/// Cheap to clone (internally `Arc`-backed).
#[derive(Clone)]
pub struct RpcClient {
    inner: Arc<Inner>,
}

type PendingMap = HashMap<String, oneshot::Sender<Bytes>>;

struct Inner {
    // ---
    transport: TransportPtr,
    node_id: String,
    pending: Mutex<PendingMap>,

    /// Best-effort receive loop handle.
    ///
    /// We keep it so the task isn't immediately dropped, and so it can be
    /// extended later (shutdown, join-on-close, etc.).
    _rx_task: JoinHandle<()>,
}

impl RpcClient {
    // ---
    /// Create a client with an explicitly provided transport.
    ///
    /// This is the constructor you want for tests and for advanced users.
    pub async fn with_transport(
        transport: TransportPtr,
        node_id: impl Into<String>,
    ) -> Result<Self> {
        // ---
        let node_id = node_id.into();

        // Subscribe to responses for this node.
        //
        // NOTE: memory transport is exact-match, so do NOT use MQTT-style wildcards here.
        // Other transports may choose to interpret subscription strings differently,
        // but they should approximate memory semantics where possible.
        let response_sub: Subscription = Subscription::from(format!("responses/{node_id}"));

        let mut handle = transport
            .subscribe(response_sub, SubscribeOptions { durable: false })
            .await?;

        // We need a partially built client to call handle_response().
        let pending: Mutex<PendingMap> = Mutex::new(PendingMap::new());

        // We'll create the Arc<Inner> after we spawn the receive loop.
        // The loop needs a clone of RpcClient (cheap).
        let transport_for_inner = transport.clone();
        let node_id_for_inner = node_id.clone();

        // Temporary inner without task; we’ll replace _rx_task after spawn.
        // (We build the Arc first so the rx task can call back into client.)
        let inner = Arc::new_cyclic(|weak| {
            // ---
            let weak = weak.clone();

            // Spawn receive loop.
            let rx_task = tokio::spawn(async move {
                // ---
                loop {
                    match handle.inbox.recv().await {
                        Some(env) => {
                            if let Some(inner) = weak.upgrade() {
                                let client = RpcClient { inner };
                                if let Err(_err) = client.handle_envelope(env) {
                                    #[cfg(feature = "logging")]
                                    log::warn!("client response handling error: {_err}");
                                }
                            } else {
                                // Inner was dropped, exit loop
                                break;
                            }
                        }
                        None => {
                            // Transport closed or subscription dropped.
                            #[cfg(feature = "logging")]
                            log::debug!("transport closed or subscription dropped");
                            break;
                        }
                    }
                }
            });

            Inner {
                // ---
                transport: transport_for_inner,
                node_id: node_id_for_inner,
                pending,
                _rx_task: rx_task,
            }
        });

        Ok(Self { inner })
    }

    /// Convenience constructor that selects the crate-default transport.
    ///
    /// This calls `crate::create_transport()` (feature-driven) and then
    /// constructs the client using `with_transport()`.
    pub async fn new(config: &super::RpcConfig, node_id: &str) -> Result<Self> {
        // ---
        let transport = crate::create_transport(config).await?;
        Self::with_transport(transport, node_id).await
    }

    /// Send an RPC request to a target service node.
    ///
    /// - `target_node_id`: service identity (e.g., `"math-service"`)
    /// - `method`: RPC method name
    /// - `req`: request payload
    pub async fn request_to<TReq, TResp>(
        &self,
        target_node_id: &str,
        method: &str,
        req: TReq,
    ) -> Result<TResp>
    where
        TReq: Serialize,
        TResp: DeserializeOwned,
    {
        // ---
        let correlation_id = CorrelationId::generate();
        let correlation_id_str = correlation_id.to_string();

        let (tx, rx) = oneshot::channel();

        {
            let mut pending = lock_ignore_poison(&self.inner.pending);
            pending.insert(correlation_id_str.clone(), tx);
        }

        let request_addr = Address::from(format!("requests/{target_node_id}"));

        let value: Value = serde_json::to_value(req)?;
        let bytes = serde_json::to_vec(&value)?;

        let env = Envelope::request(
            request_addr,
            method.into(),
            Bytes::from(bytes),
            Arc::from(correlation_id.to_string()),
            Address::from(format!("responses/{}", self.inner.node_id)),
            Arc::<str>::from("application/json"),
        );

        self.inner
            .transport
            .publish(
                env,
                PublishOptions {
                    durable: false,
                    ttl_ms: None,
                },
            )
            .await?;

        let response = rx.await.map_err(|_err| {
            #[cfg(feature = "logging")]
            log::warn!("response channel closed (server dropped or transport shutdown:{_err:?})");
            RpcError::Transport
        })?;
        let resp: TResp = serde_json::from_slice(&response)?;
        Ok(resp)
    }

    /// Internal hook used by the transport receive loop to dispatch responses.
    pub(crate) fn handle_response(&self, correlation_id: Arc<str>, payload: Bytes) -> Result<()> {
        // ---
        let key: &str = &correlation_id;

        let tx = {
            let mut pending = lock_ignore_poison(&self.inner.pending);
            pending.remove(key)
        };

        if let Some(tx) = tx {
            // If the receiver is gone, that's fine; request_to will time out / drop.
            let _ = tx.send(payload);
        }

        Ok(())
    }

    pub(crate) fn node_id(&self) -> &str {
        &self.inner.node_id
    }

    fn handle_envelope(&self, env: Envelope) -> Result<()> {
        // ---
        // We only care about responses addressed to our response address.
        // (Subscription should already constrain this for memory transport.)
        let expected_addr = format!("responses/{}", self.node_id());

        if *env.address.0 != expected_addr {
            return Ok(());
        }

        let correlation_id = env.correlation_id.ok_or(RpcError::InvalidResponse)?;

        self.handle_response(correlation_id, env.payload)
    }
}
