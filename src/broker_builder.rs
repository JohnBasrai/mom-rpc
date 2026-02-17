//! RPC broker builder.
//!
//! Provides a fluent builder API for configuring RPC broker instances
//! with retry and timeout settings.

use crate::{Result, RpcBroker, TransportPtr};
use std::time::Duration;

/// Builder for creating RPC broker instances.
///
/// Accepts a transport and allows configuration of retry behavior,
/// request timeouts, and an optional node ID override.
///
/// # Examples
///
/// ## Client with retry
/// ```no_run
/// use mom_rpc::{TransportBuilder, RpcBrokerBuilder};
/// use std::time::Duration;
///
/// # async fn example() -> mom_rpc::Result<()> {
/// let transport = TransportBuilder::new()
///     .uri("mqtt://localhost:1883")
///     .node_id("client")
///     .client_mode()
///     .build()
///     .await?;
///
/// let client = RpcBrokerBuilder::new(transport)
///     .retry_max_attempts(20)
///     .retry_multiplier(2.0)
///     .retry_initial_delay(Duration::from_millis(200))
///     .retry_max_delay(Duration::from_secs(10))
///     .request_timeout(Duration::from_millis(200))
///     .build()?;
/// # Ok(())
/// # }
/// ```
///
/// ## Server without retry
/// ```no_run
/// use mom_rpc::{TransportBuilder, RpcBrokerBuilder};
///
/// # async fn example() -> mom_rpc::Result<()> {
/// let transport = TransportBuilder::new()
///     .uri("mqtt://localhost:1883")
///     .node_id("server")
///     .server_mode()
///     .build()
///     .await?;
///
/// let server = RpcBrokerBuilder::new(transport).build()?;
/// # Ok(())
/// # }
/// ```
pub struct RpcBrokerBuilder {
    // ---
    transport: TransportPtr,
    node_id: Option<String>,

    // Retry configuration (all optional)
    retry_max_attempts: Option<u32>,
    retry_multiplier: Option<f32>,
    retry_initial_delay: Option<Duration>,
    retry_max_delay: Option<Duration>,

    // Request timeout (optional, default: 30s)
    request_timeout: Option<Duration>,
}

impl RpcBrokerBuilder {
    /// Create a new broker builder.
    ///
    /// Mode is inferred automatically from the transport's queue configuration.
    /// The broker's `node_id` defaults to `transport.transport_id()`; override
    /// with [`.node_id()`](Self::node_id) when sharing a transport between brokers.
    pub fn new(transport: TransportPtr) -> Self {
        // ---
        Self {
            transport,
            node_id: None,
            retry_max_attempts: None,
            retry_multiplier: None,
            retry_initial_delay: None,
            retry_max_delay: None,
            request_timeout: None,
        }
    }

    /// Override the broker's logical node ID.
    ///
    /// By default the broker uses `transport.transport_id()` as its node ID.
    /// This setter allows a different ID when multiple brokers share the same
    /// transport instance (common in memory-transport tests).
    pub fn node_id(mut self, id: impl Into<String>) -> Self {
        self.node_id = Some(id.into());
        self
    }

    /// Set maximum retry attempts.
    ///
    /// Default: no retries (single attempt).
    pub fn retry_max_attempts(mut self, attempts: u32) -> Self {
        self.retry_max_attempts = Some(attempts);
        self
    }

    /// Set retry backoff multiplier.
    ///
    /// Default: 2.0 (exponential backoff).
    pub fn retry_multiplier(mut self, multiplier: f32) -> Self {
        self.retry_multiplier = Some(multiplier);
        self
    }

    /// Set initial delay before first retry.
    ///
    /// Default: 100ms.
    pub fn retry_initial_delay(mut self, delay: Duration) -> Self {
        self.retry_initial_delay = Some(delay);
        self
    }

    /// Set maximum delay between retry attempts.
    ///
    /// Default: 5s.
    pub fn retry_max_delay(mut self, delay: Duration) -> Self {
        self.retry_max_delay = Some(delay);
        self
    }

    /// Set request timeout per attempt.
    ///
    /// Default: 30s.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Build the RPC broker (consumes self).
    pub fn build(self) -> Result<RpcBroker> {
        // ---
        use crate::TransportMode;

        let mode = match self.transport.mode() {
            TransportMode::Client => crate::BrokerMode::Client,
            TransportMode::Server => crate::BrokerMode::Server,
            TransportMode::FullDuplex => crate::BrokerMode::FullDuplex,
        };

        // node_id: explicit override, else fall back to transport_id
        let node_id = self
            .node_id
            .unwrap_or_else(|| self.transport.transport_id().to_string());

        // Build retry config only if at least one retry parameter was set
        let retry_config = if self.retry_max_attempts.is_some()
            || self.retry_multiplier.is_some()
            || self.retry_initial_delay.is_some()
            || self.retry_max_delay.is_some()
        {
            Some(crate::RetryConfig {
                max_attempts: self.retry_max_attempts.unwrap_or(3),
                multiplier: self.retry_multiplier.unwrap_or(2.0),
                initial_delay: self
                    .retry_initial_delay
                    .unwrap_or(Duration::from_millis(100)),
                max_delay: self.retry_max_delay.unwrap_or(Duration::from_secs(5)),
            })
        } else {
            None
        };

        let request_timeout = self.request_timeout.unwrap_or(Duration::from_secs(30));

        RpcBroker::new(self.transport, node_id, mode, retry_config, request_timeout)
    }
}
