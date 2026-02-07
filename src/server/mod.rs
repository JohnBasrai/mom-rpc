// src/server/mod.rs
//! RPC server for handling requests and sending responses.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use bytes::Bytes;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::SubscribeOptions;

use super::{
    // ---
    Address,
    Envelope,
    PublishOptions,
    Result,
    RpcError,
    Subscription,
    TransportPtr,
};

mod handler;

use handler::{wrap_handler, BoxedHandler};

/// Acquire a mutex guard, intentionally ignoring poisoning.
///
/// Mutex poisoning indicates that another task panicked while holding the lock.
/// In this server, the protected state is a best-effort handler registry
/// (method → handler). There are no cross-field invariants whose violation
/// could cause memory unsafety or systemic corruption.
///
/// The worst possible outcome is a missing handler dispatch, which is
/// acceptable for an RPC server.
fn lock_ignore_poison<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // ---
    match m.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

// Key=method, value=handler
type HandlerMap = HashMap<String, BoxedHandler>;

/// Running RPC server instance.
///
/// Cheap to clone (internally `Arc`-backed).
#[derive(Clone)]
pub struct RpcServer {
    // ---
    inner: Arc<Inner>,
}

struct Inner {
    // ---
    transport: TransportPtr,
    node_id: String,
    handlers: Mutex<HandlerMap>,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
}

impl RpcServer {
    // ---
    pub fn with_transport(transport: TransportPtr, node_id: impl Into<String>) -> Self {
        // ---
        Self {
            inner: Arc::new(Inner {
                transport,
                node_id: node_id.into(),
                handlers: Mutex::new(HashMap::new()),
                shutdown_tx: Mutex::new(None),
            }),
        }
    }

    /// Run server, blocking current task until shutdown() is called.
    ///
    /// This method subscribes to incoming requests and processes them in a loop
    /// until `shutdown()` is called from another task or the transport closes.
    ///
    /// **Important:** This method blocks the current task. Use this in your main
    /// application loop. For tests or background usage, use `spawn()` instead.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mom_rpc::{RpcServer, RpcConfig, create_memory_transport};
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = RpcConfig::memory("server");
    /// let transport = create_memory_transport(&config).await?;
    /// let server = RpcServer::with_transport(transport.clone(), "my-service");
    ///
    /// // Setup signal handling
    /// let server_clone = server.clone();
    /// tokio::spawn(async move {
    ///     tokio::signal::ctrl_c().await.expect("failed to listen for Ctrl+C");
    ///     server_clone.shutdown().await.expect("shutdown failed");
    /// });
    ///
    /// // Blocks until shutdown() is called
    /// server.run().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run(&self) -> Result<()> {
        // ---
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        // Store shutdown sender for shutdown() to use
        {
            let mut tx = lock_ignore_poison(&self.inner.shutdown_tx);
            *tx = Some(shutdown_tx);
        }

        self.run_with_shutdown(shutdown_rx).await
    }

    /// Spawn server in background, returning a JoinHandle.
    ///
    /// This is an alternative to `run()` for when you need the server running
    /// in the background while doing other work in the main task.
    ///
    /// **Use cases:**
    /// - Tests that need both server and client in the same task
    /// - Applications that need to do other work while server runs
    /// - When you want manual control over the server task
    ///
    /// **Important:** Use either `run()` OR `spawn()`, not both.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mom_rpc::{RpcServer, RpcConfig, create_memory_transport};
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = RpcConfig::memory("server");
    /// let transport = create_memory_transport(&config).await?;
    /// let server = RpcServer::with_transport(transport.clone(), "my-service");
    ///
    /// // Spawn server in background
    /// let handle = server.spawn();
    ///
    /// // ... do other work (e.g., create client, send requests) ...
    ///
    /// // Clean shutdown
    /// server.shutdown().await?;
    /// handle.await??;
    /// # Ok(())
    /// # }
    /// ```
    pub fn spawn(&self) -> JoinHandle<Result<()>> {
        // ---
        let server = self.clone();
        tokio::spawn(async move { server.run().await })
    }

    /// Trigger graceful shutdown of the server.
    ///
    /// Sends a shutdown signal to `run()`, causing it to exit its loop.
    /// This does not wait for the server to actually stop - use the JoinHandle
    /// from `spawn()` or rely on `run()` returning if you need synchronization.
    ///
    /// **Important:** When using `spawn()`, always call `shutdown()` BEFORE
    /// closing the transport. Closing the transport first will leave the server
    /// task hanging.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mom_rpc::{RpcServer, RpcConfig, create_memory_transport};
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = RpcConfig::memory("server");
    /// let transport = create_memory_transport(&config).await?;
    /// let server = RpcServer::with_transport(transport.clone(), "my-service");
    ///
    /// let handle = server.spawn();
    ///
    /// // ... do work ...
    ///
    /// // CORRECT shutdown order:
    /// server.shutdown().await?;   // 1. Signal server to stop
    /// transport.close().await?;   // 2. Close transport resources
    /// handle.await??;             // 3. Wait for task completion
    /// # Ok(())
    /// # }
    /// ```
    pub async fn shutdown(&self) -> Result<()> {
        // ---
        let tx = {
            let mut tx_opt = lock_ignore_poison(&self.inner.shutdown_tx);
            tx_opt.take()
        };

        if let Some(tx) = tx {
            let _ = tx.send(());
        }

        Ok(())
    }

    async fn run_with_shutdown(&self, mut shutdown_rx: oneshot::Receiver<()>) -> Result<()> {
        // ---
        let sub = Subscription::from(format!("requests/{}", self.node_id()));

        let mut handle = self
            .inner
            .transport
            .subscribe(sub, SubscribeOptions { durable: false })
            .await?;

        loop {
            tokio::select! {
                msg = handle.inbox.recv() => {
                    match msg {
                        Some(env) => {
                            if let Err(_err) = self.handle_envelope(env).await {
                                #[cfg(feature = "logging")]
                                log::warn!("server request handling error: {_err}");
                            }
                        }
                        None => {
                            #[cfg(feature = "logging")]
                            log::debug!("transport closed or subscription dropped");
                            break;
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    #[cfg(feature = "logging")]
                    log::debug!("shutdown signal received");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Register a typed async handler for a method.
    ///
    /// The handler receives the decoded request payload type and returns a response
    /// payload type. The server wraps these into the crate’s request/response
    /// envelope format (including correlation).
    pub fn register<F, Fut, Req, Resp>(&self, method: &str, handler: F)
    where
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<Resp>> + Send + 'static,
        Req: serde::de::DeserializeOwned + Send + 'static,
        Resp: serde::Serialize + Send + 'static,
    {
        // ---
        let mut handlers = lock_ignore_poison(&self.inner.handlers);
        handlers.insert(method.to_string(), wrap_handler(handler));
    }

    pub(crate) fn node_id(&self) -> &str {
        &self.inner.node_id
    }

    async fn dispatch_request(&self, method: &str, bytes: Bytes) -> Result<Bytes> {
        // ---
        let handler = {
            let handlers = lock_ignore_poison(&self.inner.handlers);
            handlers.get(method).cloned()
        };

        let handler = handler.ok_or_else(|| RpcError::HandlerNotFound(method.to_string()))?;

        // Return response payload or an error
        handler(bytes).await
    }

    async fn publish_response(
        &self,
        response_topic: String,
        response_bytes: Bytes,
        correlation_id: Arc<str>,
    ) -> Result<()> {
        // ---
        let env = Envelope::response(
            Address::from(response_topic),
            response_bytes,
            correlation_id,
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
            .await
    }

    #[cfg(false)]
    fn subscription(&self) -> Subscription {
        // ---
        // We want to receive requests for any method under:
        //   requests/<node_id>/<method>
        //
        // The in-memory transport defines reference semantics. Other transports
        // should approximate this behavior as closely as possible.
        Subscription::from(format!("requests/{}", self.node_id()))
    }

    async fn handle_envelope(&self, env: Envelope) -> Result<()> {
        // ---
        let method = env.method.as_deref().ok_or(RpcError::InvalidRequest)?;

        let reply_to = env.reply_to.ok_or(RpcError::MissingResponseTopic)?;

        let correlation_id = env.correlation_id.ok_or(RpcError::InvalidRequest)?;

        let response_payload = self.dispatch_request(method, env.payload).await?;

        self.publish_response(reply_to.0.to_string(), response_payload, correlation_id)
            .await
    }
}
