// src/client/mod.rs
//! RPC client implementation.
//!
//! This module contains the core [`RpcClient`] type which sends RPC requests
//! to server nodes and receives responses over a transport.
//!
//! # Architecture
//!
//! The client subscribes to `responses/{node_id}` on the configured transport
//! and runs a background receive loop to match incoming responses with pending
//! requests using correlation IDs.
//!
//! Each request generates a unique correlation ID and registers a oneshot
//! channel in the pending map. When a response arrives, the receive loop
//! looks up the channel and sends the payload back to the waiting request.
//!
//! # Concurrency
//!
//! Multiple requests can be in-flight simultaneously. The pending map is
//! protected by a mutex but lock contention is minimal since operations
//! are just HashMap insert/remove.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time;

use crate::{
    // ---
    log_debug,
    Address,
    Envelope,
    Result,
    RpcError,
    Subscription,
    TransportPtr,
};

use crate::CorrelationId;

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

        let mut handle = transport.subscribe(response_sub).await?;

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
                                    crate::log_warn!("client response handling error: {_err}");
                                }
                            } else {
                                // Inner was dropped, exit loop
                                break;
                            }
                        }
                        None => {
                            // Transport closed or subscription dropped.
                            log_debug!("transport closed or subscription dropped");
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
    pub async fn new(config: &crate::RpcConfig, node_id: &str) -> Result<Self> {
        // ---
        let transport = crate::create_transport(config).await?;
        Self::with_transport(transport, node_id).await
    }

    /// Send an RPC request to a target service node.
    ///
    /// - `target_node_id`: service identity (e.g., `"sensor-service"`)
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

        self.inner.transport.publish(env).await?;

        let response = rx.await.map_err(|err| {
            let msg =
                format!("response channel closed (server dropped or transport shutdown:{err:?})");
            RpcError::Transport(msg)
        })?;
        let resp: TResp = serde_json::from_slice(&response)?;
        Ok(resp)
    }

    /// Send an RPC request with a timeout.
    ///
    /// This is a convenience wrapper around [`request_to`](Self::request_to) that
    /// applies a timeout to the entire request/response cycle. If the timeout
    /// expires before a response is received, returns [`RpcError::Timeout`].
    ///
    /// # Arguments
    ///
    /// - `target_node_id`: service identity (e.g., `"sensor-service"`)
    /// - `method`: RPC method name
    /// - `req`: request payload
    /// - `timeout`: maximum time to wait for response
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mom_rpc::{RpcClient, create_transport, RpcConfig};
    /// # use serde::{Deserialize, Serialize};
    /// # use std::time::Duration;
    /// #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    /// pub enum TemperatureUnit { Celsius, Fahrenheit }
    /// #[derive(Debug, Clone, Serialize, Deserialize)]
    /// pub struct ReadTemperature { pub unit: TemperatureUnit }
    /// #[derive(Debug, Clone, Serialize, Deserialize)]
    /// pub struct SensorReading {
    ///     pub value: f32,
    ///     pub unit: String,
    ///     pub timestamp_ms: u64,
    /// }
    /// # async fn example() -> mom_rpc::Result<()> {
    /// let config = RpcConfig::memory("client");
    /// let transport = create_transport(&config).await?;
    /// let client = RpcClient::with_transport(transport, "client").await?;
    ///
    /// let resp: SensorReading = client
    ///     .request_with_timeout(
    ///         "env-sensor-42",
    ///         "read_temperature",
    ///         ReadTemperature { unit: TemperatureUnit::Celsius },
    ///         Duration::from_secs(5),
    ///     )
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn request_with_timeout<TReq, TResp>(
        &self,
        target_node_id: &str,
        method: &str,
        req: TReq,
        timeout: Duration,
    ) -> Result<TResp>
    where
        TReq: Serialize,
        TResp: DeserializeOwned,
    {
        time::timeout(timeout, self.request_to(target_node_id, method, req))
            .await
            .map_err(|_| RpcError::Timeout)?
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
            if tx.send(payload).is_err() {
                log_debug!(
                    "response arrived after request abandoned (correlation_id: {correlation_id})"
                );
            }
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
