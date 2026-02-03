// src/server/mod.rs
//! RPC server for handling requests and sending responses.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use bytes::Bytes;
use tokio::task::JoinHandle;

use crate::SubscribeOptions;

use super::{
    // ---
    Address,
    Envelope,
    Error,
    PublishOptions,
    Result,
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
}

impl RpcServer {
    // ---
    pub fn new(transport: TransportPtr, node_id: String) -> Self {
        // ---
        Self {
            inner: Arc::new(Inner {
                transport,
                node_id,
                handlers: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub async fn run(&self) -> Result<JoinHandle<Result<()>>> {
        // ---
        let sub = Subscription::from(format!("requests/{}", self.node_id()));

        let mut handle = self
            .inner
            .transport
            .subscribe(sub, SubscribeOptions { durable: false })
            .await?;

        let server = self.clone();

        let join = tokio::spawn(async move {
            loop {
                match handle.inbox.recv().await {
                    Some(env) => {
                        if let Err(_err) = server.handle_envelope(env).await {
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
            Ok(())
        });

        Ok(join)
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

        let handler = handler.ok_or_else(|| Error::HandlerNotFound(method.to_string()))?;

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
        let method = env.method.as_deref().ok_or(Error::InvalidRequest)?;

        let reply_to = env.reply_to.ok_or(Error::MissingResponseTopic)?;

        let correlation_id = env.correlation_id.ok_or(Error::InvalidRequest)?;

        let response_payload = self.dispatch_request(method, env.payload).await?;

        self.publish_response(reply_to.0.to_string(), response_payload, correlation_id)
            .await
    }
}
