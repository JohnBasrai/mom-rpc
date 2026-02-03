//! Transport execution runner.
//!
//! This module provides the glue between a [`Transport`] implementation and
//! higher-level protocol roles (such as RPC servers).
//!
//! The runner is responsible for:
//! - subscribing to a transport using a consumer-provided [`Subscription`]
//! - driving a receive loop over the transport inbox
//! - dispatching received [`Envelope`]s to user-defined logic
//! - consolidating logging and error handling for inbound messages
//!
//! This module is intentionally small and opinionated. It does **not**:
//! - define RPC semantics
//! - impose retry, timeout, or durability policies
//! - attempt to enforce lifecycle correctness at the type level
//!
//! Users are expected to explicitly call [`run`] when they are ready to begin
//! processing incoming messages.
//!
//! ## Design notes
//!
//! ### Explicit execution
//!
//! The runner does not attempt to ensure that it has been started via the type
//! system. Forgetting to call [`run`] will simply result in no messages being
//! processed. This is considered an acceptable tradeoff in favor of simplicity
//! and composability.
//!
//! ### Reference semantics
//!
//! The in-memory transport defines the reference behavior for subscription
//! matching and message delivery. Broker-backed transports are expected to
//! approximate this behavior as closely as their underlying system allows.
//!
//! ### Receive loop
//!
//! Internally, [`run`] spawns a long-running asynchronous task that waits for
//! incoming envelopes from the transport. When the transport closes or the
//! subscription is dropped, the loop exits naturally.
//!
//! The loop yields execution while waiting for messages and does not spin or
//! burn CPU.
//!
//! ### Error handling
//!
//! Errors returned by [`TransportConsumer::handle_envelope`] are considered
//! best-effort failures and are logged (when logging is enabled), but do not
//! terminate the receive loop. This mirrors the behavior of message-oriented
//! middleware, where individual message failures should not bring down the
//! consumer.
//!
//! ## Relationship to higher-level APIs
//!
//! Higher-level constructs such as `RpcServer` implement [`TransportConsumer`]
//! and are driven by this runner. The runner itself is transport-agnostic and
//! protocol-agnostic.
//!
//! This separation allows the same transport infrastructure to be reused for
//! different protocols or roles in the future, including potential full-duplex
//! or ORB-style designs.

use crate::Result;
use tokio::task::JoinHandle;

use crate::{
    // ---
    Envelope,
    Subscription,
    TransportPtr,
};

/// A consumer of transport-delivered envelopes.
///
/// Implementors define:
/// - which subscription they are interested in
/// - how incoming envelopes should be handled
///
/// This trait intentionally makes no assumptions about protocol semantics.
/// It is equally suitable for RPC servers, test harnesses, or other
/// message-driven roles.
///
/// Implementations should assume:
/// - envelopes may arrive out of order
/// - envelopes may be duplicated (depending on transport)
/// - delivery is best-effort unless explicitly guaranteed by the transport
#[async_trait::async_trait]
pub trait TransportConsumer: Send + Sync {
    /// Return the subscription used to receive incoming envelopes.
    ///
    /// The interpretation of the subscription string is transport-specific.
    /// However, transports are expected to approximate the reference semantics
    /// defined by the in-memory transport.
    fn subscription(&self) -> Subscription;

    /// Handle a single incoming envelope.
    ///
    /// Returning an error indicates that the envelope could not be processed,
    /// but does not terminate the receive loop. Errors are logged (when enabled)
    /// and processing continues with subsequent messages.
    async fn handle_envelope(&self, env: Envelope) -> Result<()>;
}

/// Start a transport receive loop for a given consumer.
///
/// This function:
/// - subscribes to the transport using the consumer-provided subscription
/// - spawns an asynchronous task that receives envelopes
/// - dispatches envelopes to the consumer
///
/// The returned [`JoinHandle`] represents the lifetime of the receive loop.
/// Dropping the handle does not stop the loop; the loop terminates naturally
/// when the transport closes or the subscription is dropped.
///
/// ## Lifecycle
///
/// This function does not block. It is the caller’s responsibility to ensure
/// that the returned task handle is retained for as long as message processing
/// is desired.
///
/// ## Logging
///
/// When the `logging` feature is enabled:
/// - errors returned by the consumer are logged at `warn` level
/// - transport shutdown is logged at `debug` level
pub async fn run<T>(transport: TransportPtr, consumer: T) -> Result<JoinHandle<Result<()>>>
where
    T: TransportConsumer + 'static,
{
    // ---
    let sub = consumer.subscription();

    let mut handle = transport
        .subscribe(sub, crate::domain::SubscribeOptions { durable: false })
        .await?;

    let join = tokio::spawn(async move {
        // ---
        loop {
            match handle.inbox.recv().await {
                Some(env) => {
                    if let Err(_err) = consumer.handle_envelope(env).await {
                        #[cfg(feature = "logging")]
                        log::warn!("transport consumer error: {_err}");
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
