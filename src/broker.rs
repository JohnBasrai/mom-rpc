//! Unified RPC broker.
//!
//! Provides a single type that can act as client, server, or both (full-duplex)
//! based on the configured mode.

use crate::{
    // ---
    Address,
    BrokerMode,
    Envelope,
    Result,
    RetryConfig,
    RpcError,
    Subscription,
    TransportPtr,
};
use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Map of pending client requests awaiting responses.
///
/// Key: correlation ID, Value: oneshot sender to deliver the response payload.
type PendingRequests = Arc<Mutex<HashMap<String, oneshot::Sender<Bytes>>>>;

/// Registry of RPC method handlers.
///
/// Key: method name, Value: type-erased handler function.
type HandlerRegistry = Arc<Mutex<HashMap<String, Arc<dyn HandlerFn>>>>;

/// Unified RPC broker supporting client, server, and full-duplex modes.
///
/// The broker's operational mode determines which methods are valid:
/// - **Client mode**: Can call `request_to()`, cannot `register_rpc_handler()` or `spawn()`
/// - **Server mode**: Can `register_rpc_handler()` and `spawn()`, cannot call `request_to()`
/// - **Full-duplex mode**: Can use all methods
pub struct RpcBroker {
    inner: Arc<Inner>,
}

struct Inner {
    transport: TransportPtr,
    node_id: String,
    mode: BrokerMode,
    retry_config: Option<RetryConfig>,
    request_total_timeout: Duration,

    // Client-side state (for Client and FullDuplex modes)
    pending: PendingRequests,
    _rx_task: Option<JoinHandle<()>>,

    // Server-side state (for Server and FullDuplex modes)
    handlers: HandlerRegistry,
    _server_rx_task: Option<JoinHandle<()>>,

    // Shutdown signaling - shared across clones
    shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    shutdown_rx: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
}

// Handler trait for type-erased async functions
trait HandlerFn: Send + Sync {
    fn call(&self, payload: Bytes) -> BoxFuture<'static, Result<Bytes>>;
}

type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// Implementation of HandlerFn for actual handler closures
struct Handler<F, Fut, TReq, TResp>
where
    F: Fn(TReq) -> Fut + Send + Sync,
    Fut: Future<Output = Result<TResp>> + Send,
    TReq: DeserializeOwned,
    TResp: Serialize,
{
    func: F,
    _phantom: std::marker::PhantomData<fn(TReq, TResp, Fut)>,
}

impl<F, Fut, TReq, TResp> HandlerFn for Handler<F, Fut, TReq, TResp>
where
    F: Fn(TReq) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<TResp>> + Send + 'static,
    TReq: DeserializeOwned + Send + 'static,
    TResp: Serialize + Send + 'static,
{
    fn call(&self, payload: Bytes) -> BoxFuture<'static, Result<Bytes>> {
        // Deserialize request
        let req: TReq = match serde_json::from_slice(&payload) {
            Ok(r) => r,
            Err(e) => return Box::pin(async move { Err(e.into()) }),
        };

        // Call handler
        let fut = (self.func)(req);

        Box::pin(async move {
            let resp = fut.await?;
            let bytes = serde_json::to_vec(&resp)?;
            Ok(Bytes::from(bytes))
        })
    }
}

/// Acquire mutex guard, ignoring poisoning
fn lock_ignore_poison<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

impl RpcBroker {
    /// Create a new RPC broker (internal use by RpcBrokerBuilder).
    pub(crate) fn new(
        transport: TransportPtr,
        node_id: String,
        mode: BrokerMode,
        retry_config: Option<RetryConfig>,
        request_total_timeout: Duration,
    ) -> Result<Self> {
        // Initialize based on mode
        let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));
        let handlers: HandlerRegistry = Arc::new(Mutex::new(HashMap::new()));

        let (rx_task, server_rx_task) = match mode {
            BrokerMode::Client => {
                // Client mode: start response receiver task
                let rx_task = Some(Self::start_client_task(
                    transport.clone(),
                    node_id.clone(),
                    pending.clone(),
                ));
                (rx_task, None)
            }
            BrokerMode::Server => {
                // Server mode: start request receiver task
                let server_rx_task = Some(Self::start_server_task(
                    transport.clone(),
                    node_id.clone(),
                    handlers.clone(),
                ));
                (None, server_rx_task)
            }
            BrokerMode::FullDuplex => {
                // Full-duplex: start both receiver tasks
                let rx_task = Some(Self::start_client_task(
                    transport.clone(),
                    node_id.clone(),
                    pending.clone(),
                ));
                let server_rx_task = Some(Self::start_server_task(
                    transport.clone(),
                    node_id.clone(),
                    handlers.clone(),
                ));
                (rx_task, server_rx_task)
            }
        };

        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        Ok(Self {
            inner: Arc::new(Inner {
                transport,
                node_id,
                mode,
                retry_config,
                request_total_timeout,
                pending,
                _rx_task: rx_task,
                handlers,
                _server_rx_task: server_rx_task,
                shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
                shutdown_rx: Arc::new(Mutex::new(Some(shutdown_rx))),
            }),
        })
    }

    // Start background task to receive responses (client-side)
    fn start_client_task(
        transport: TransportPtr,
        node_id: String,
        pending: Arc<Mutex<HashMap<String, oneshot::Sender<Bytes>>>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let subscription = Subscription::from(format!("responses/{node_id}"));
            let mut handle = match transport.subscribe(subscription).await {
                Ok(h) => h,
                Err(e) => {
                    crate::log_error!("failed to subscribe to responses: {e}");
                    return;
                }
            };

            crate::log_debug!("client task started for responses/{node_id}");

            while let Some(envelope) = handle.inbox.recv().await {
                let correlation_id = match envelope.correlation_id {
                    Some(ref id) => id.as_ref(),
                    None => {
                        crate::log_warn!("response missing correlation_id");
                        continue;
                    }
                };

                let tx = {
                    let mut pending = lock_ignore_poison(&pending);
                    pending.remove(correlation_id)
                };

                if let Some(tx) = tx {
                    let _ = tx.send(envelope.payload);
                } else {
                    crate::log_debug!("no pending request for correlation_id: {correlation_id}");
                }
            }

            crate::log_debug!("client task stopped for responses/{node_id}");
        })
    }

    // Start background task to receive requests (server-side)
    fn start_server_task(
        transport: TransportPtr,
        node_id: String,
        handlers: Arc<Mutex<HashMap<String, Arc<dyn HandlerFn>>>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let subscription = Subscription::from(format!("requests/{node_id}"));
            let mut handle = match transport.subscribe(subscription).await {
                Ok(h) => h,
                Err(e) => {
                    crate::log_error!("failed to subscribe to requests: {e}");
                    return;
                }
            };

            crate::log_debug!("server task started for requests/{node_id}");

            while let Some(envelope) = handle.inbox.recv().await {
                let method = match envelope.method {
                    Some(ref m) => m.as_ref(),
                    None => {
                        crate::log_warn!("request missing method");
                        continue;
                    }
                };

                let reply_to = match envelope.reply_to {
                    Some(ref addr) => addr.clone(),
                    None => {
                        crate::log_warn!("request missing reply_to");
                        continue;
                    }
                };

                let correlation_id = match envelope.correlation_id {
                    Some(ref id) => id.clone(),
                    None => {
                        crate::log_warn!("request missing correlation_id");
                        continue;
                    }
                };

                // Look up handler
                let handler = {
                    let handlers = lock_ignore_poison(&handlers);
                    handlers.get(method).cloned()
                };

                let handler = match handler {
                    Some(h) => h,
                    None => {
                        crate::log_warn!("no handler for method: {method}");
                        // Could send error response here
                        continue;
                    }
                };

                // Call handler and send response
                let transport_clone = transport.clone();
                tokio::spawn(async move {
                    let result = handler.call(envelope.payload).await;

                    let response_payload = match result {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            crate::log_error!("handler error: {e}");
                            return;
                        }
                    };

                    let response_env = Envelope::response(
                        reply_to,
                        response_payload,
                        correlation_id,
                        Arc::from("application/json"),
                    );

                    if let Err(e) = transport_clone.publish(response_env).await {
                        crate::log_error!("failed to publish response: {e}");
                    }
                });
            }

            crate::log_debug!("server task stopped for requests/{node_id}");
        })
    }

    /// Send an RPC request (Client or FullDuplex mode only).  Uses the broker's
    /// configured `request_total_timeout`.
    ///
    /// # Errors
    ///
    /// Returns [`RpcError::Timeout`] if the request exceeds `request_total_timeout`.
    ///
    /// Returns [`RpcError::InvalidMode`] if the broker is in server mode.
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
        // Use configured timeout
        self.request_to_with_timeout(
            target_node_id,
            method,
            req,
            self.inner.request_total_timeout,
        )
        .await
    }

    /// Send an RPC request with custom timeout (Client or FullDuplex mode only).
    ///
    /// Overrides the broker's configured `request_total_timeout` for this single
    /// request.  Useful for operations that need longer/shorter timeouts than the
    /// default.
    ///
    /// # Errors
    ///
    /// Returns [`RpcError::Timeout`] if the request exceeds `request_total_timeout`.
    ///
    /// Returns [`RpcError::InvalidMode`] if the broker is in server mode.
    pub async fn request_to_with_timeout<TReq, TResp>(
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
        // Validate mode
        match self.inner.mode {
            BrokerMode::Client | BrokerMode::FullDuplex => {
                // Allowed - proceed
            }
            BrokerMode::Server => {
                return Err(RpcError::InvalidMode(
                    "request_to() not allowed in Server mode".into(),
                ));
            }
        }

        let target_node_id = target_node_id.to_string();
        let method = method.to_string();

        // Serialize request once outside retry loop
        let value: serde_json::Value = serde_json::to_value(req)?;
        let bytes = serde_json::to_vec(&value)?;
        let req_bytes = Bytes::from(bytes);

        // Use retry helper directly with optional retry config
        let response_bytes = crate::retry_with_backoff(self.inner.retry_config.as_ref(), || {
            let target_node_id = target_node_id.clone();
            let method = method.clone();
            let req_bytes = req_bytes.clone();
            let inner = self.inner.clone();
            let custom_timeout = timeout;

            async move {
                Self::request_to_inner_with_timeout(
                    inner,
                    &target_node_id,
                    &method,
                    req_bytes,
                    custom_timeout,
                )
                .await
            }
        })
        .await?;

        // Deserialize response after successful retry
        let resp: TResp = serde_json::from_slice(&response_bytes)?;
        Ok(resp)
    }

    /// Inner request implementation without retry logic.
    async fn request_to_inner_with_timeout(
        inner: Arc<Inner>,
        target_node_id: &str,
        method: &str,
        req_bytes: Bytes,
        timeout: Duration,
    ) -> Result<Bytes> {
        use crate::CorrelationId;
        use tokio::time;

        let correlation_id = CorrelationId::generate();
        let correlation_id_str = correlation_id.to_string();

        let (tx, rx) = oneshot::channel();

        {
            let mut pending = lock_ignore_poison(&inner.pending);
            pending.insert(correlation_id_str.clone(), tx);
        }

        let request_addr = Address::from(format!("requests/{target_node_id}"));

        let env = Envelope::request(
            request_addr,
            method.into(),
            req_bytes,
            Arc::from(correlation_id.to_string()),
            Address::from(format!("responses/{}", inner.node_id)),
            Arc::from("application/json"),
        );

        inner.transport.publish(env).await?;

        let has_retry = inner.retry_config.is_some();

        let response = time::timeout(timeout, rx)
            .await
            .map_err(|_| {
                if has_retry {
                    // With retry: return retryable error to trigger retry
                    RpcError::TransportRetryable(
                        "request timeout waiting for response, will retry".into(),
                    )
                } else {
                    // Without retry: return terminal timeout error
                    RpcError::Timeout
                }
            })?
            .map_err(|err| {
                let msg = format!(
                    "response channel closed (server dropped or transport shutdown:{err:?})"
                );
                RpcError::Transport(msg)
            })?;

        Ok(response)
    }

    /// Register an RPC handler (Server or FullDuplex mode only).
    ///
    /// # Errors
    ///
    /// Returns `RpcError::InvalidMode` if called in Client mode.
    pub fn register_rpc_handler<TReq, TResp, F, Fut>(&self, method: &str, handler: F) -> Result<()>
    where
        TReq: DeserializeOwned + Send + 'static,
        TResp: Serialize + Send + 'static,
        F: Fn(TReq) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<TResp>> + Send + 'static,
    {
        // Validate mode
        match self.inner.mode {
            BrokerMode::Server | BrokerMode::FullDuplex => {
                // Allowed - proceed
            }
            BrokerMode::Client => {
                return Err(RpcError::InvalidMode(
                    "register_rpc_handler() not allowed in Client mode".into(),
                ));
            }
        }

        let handler_obj = Handler {
            func: handler,
            _phantom: std::marker::PhantomData,
        };

        let mut handlers = lock_ignore_poison(&self.inner.handlers);
        handlers.insert(method.to_string(), Arc::new(handler_obj));

        Ok(())
    }

    /// Spawn the broker's receive loop (Server or FullDuplex mode only).
    ///
    /// # Errors
    ///
    /// Returns `RpcError::InvalidMode` if called in Client mode.
    pub fn spawn(self) -> Result<JoinHandle<()>> {
        // Validate mode
        match self.inner.mode {
            BrokerMode::Server | BrokerMode::FullDuplex => {
                // Allowed - proceed
            }
            BrokerMode::Client => {
                return Err(RpcError::InvalidMode(
                    "spawn() not allowed in Client mode".into(),
                ));
            }
        }

        // Server task already running in background
        // Just return a handle that completes when broker is dropped
        Ok(tokio::spawn(async move {
            // Keep broker alive
            std::future::pending::<()>().await;
        }))
    }

    /// Run the broker's receive loop (Server or FullDuplex mode only).
    ///
    /// # Errors
    ///
    /// Returns `RpcError::InvalidMode` if called in Client mode.
    pub async fn run(self) -> Result<()> {
        // Validate mode
        match self.inner.mode {
            BrokerMode::Server | BrokerMode::FullDuplex => {
                // Allowed - proceed
            }
            BrokerMode::Client => {
                return Err(RpcError::InvalidMode(
                    "run() not allowed in Client mode".into(),
                ));
            }
        }

        // Wait for shutdown signal
        let shutdown_rx = lock_ignore_poison(&self.inner.shutdown_rx).take();
        if let Some(rx) = shutdown_rx {
            let _ = rx.await;
        }
        Ok(())
    }

    /// Shutdown the broker.
    pub async fn shutdown(&self) {
        // Send shutdown signal
        {
            let mut tx_guard = lock_ignore_poison(&self.inner.shutdown_tx);
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(());
            }
        }

        // Close transport
        let _ = self.inner.transport.close().await;
    }
}

impl Clone for RpcBroker {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
