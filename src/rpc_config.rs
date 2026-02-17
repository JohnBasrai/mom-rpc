//! Public, transport-agnostic RPC configuration.
//!
//! This type intentionally contains no transport-specific concepts
//! (e.g. MQTT client options). Transport layers are responsible for
//! interpreting this config into concrete connection settings.

use std::time::Duration;

/// Retry configuration with exponential backoff.
///
/// Configures automatic retry behavior for handling transient failures
/// in broker-based transports (MQTT, AMQP) where servers may not be
/// subscribed when clients publish requests.
///
/// # Example
///
/// ```
/// use mom_rpc::RetryConfig;
/// use std::time::Duration;
///
/// let retry_config = RetryConfig {
///     max_attempts: 5,
///     multiplier: 2.0,
///     initial_delay: Duration::from_millis(100),
///     max_delay: Duration::from_secs(10),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (0 = no retries, just the initial attempt).
    pub max_attempts: u32,

    /// Backoff multiplier applied to the delay after each retry.
    ///
    /// Example: 2.0 doubles the delay each time (exponential backoff).
    pub multiplier: f32,

    /// Initial delay before the first retry.
    pub initial_delay: Duration,

    /// Maximum delay between retry attempts (caps exponential growth).
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    /// Reasonable default retry configuration.
    ///
    /// - `max_attempts`: 3
    /// - `multiplier`: 2.0 (exponential backoff)
    /// - `initial_delay`: 100ms
    /// - `max_delay`: 5s
    fn default() -> Self {
        Self {
            max_attempts: 3,
            multiplier: 2.0,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
        }
    }
}

/// Transport configuration and connection parameters.
#[derive(Debug, Clone)]
pub struct RpcConfig {
    // ---
    /// Transport connection URI.
    ///
    /// For broker-based transports (MQTT, AMQP, Kafka), this specifies the
    /// broker address (e.g., "mqtt://localhost:1883", "amqp://localhost:5672/%2f").
    ///
    /// For brokerless transports (in-memory, DDS), this may be `None` or contain
    /// transport-specific configuration (e.g., DDS domain ID).
    pub transport_uri: Option<String>,

    /// Broker connection keep-alive interval in seconds (0 to disable).
    pub keep_alive_secs: Option<u16>,

    /// Unique identifier for this transport instance, used for logging.
    pub transport_id: String,

    /// Custom request queue name (AMQP only).
    ///
    /// If `None`, the queue name is derived from `transport_id`.
    pub request_queue_name: Option<String>,

    /// Custom response queue name (AMQP only).
    ///
    /// If `None`, the queue name is derived from `transport_id`.
    pub response_queue_name: Option<String>,

    /// Optional retry configuration for handling transient failures.
    ///
    /// When configured, RPC requests will automatically retry on
    /// retryable transport errors using exponential backoff.
    pub retry_config: Option<RetryConfig>,

    /// Timeout for waiting on response from server on each attempt.
    ///
    /// This is the timeout for receiving a response after publishing a request.
    /// If retry is enabled, this timeout applies to each retry attempt independently.
    ///
    /// Default: 30 seconds
    pub request_timeout: Duration,
}

impl RpcConfig {
    /// Create a new `RpcConfig` with the given broker URI.
    ///
    /// Keep-alive will use the transport default.
    pub fn with_broker(transport_uri: impl Into<String>, transport_id: impl Into<String>) -> Self {
        Self {
            transport_uri: Some(transport_uri.into()),
            keep_alive_secs: None,
            transport_id: transport_id.into(),
            request_queue_name: None,
            response_queue_name: None,
            retry_config: None,
            request_timeout: Duration::from_secs(30),
        }
    }

    /// Create a memory transport config (no broker).
    pub fn memory(transport_id: impl Into<String>) -> Self {
        Self {
            transport_uri: None,
            keep_alive_secs: None,
            transport_id: transport_id.into(),
            request_queue_name: None,
            response_queue_name: None,
            retry_config: None,
            request_timeout: Duration::from_secs(30),
        }
    }

    /// Set an explicit keep-alive interval.
    pub fn with_keep_alive_secs(mut self, secs: u16) -> Self {
        self.keep_alive_secs = Some(secs);
        self
    }

    /// Set a custom request queue name (AMQP only).
    pub fn with_request_queue_name(mut self, name: impl Into<String>) -> Self {
        self.request_queue_name = Some(name.into());
        self
    }

    /// Set a custom response queue name (AMQP only).
    pub fn with_response_queue_name(mut self, name: impl Into<String>) -> Self {
        self.response_queue_name = Some(name.into());
        self
    }

    /// Configure retry behavior with exponential backoff.
    ///
    /// Enables automatic retry of failed RPC requests on retryable
    /// transport errors. Particularly useful for broker-based transports
    /// (MQTT, AMQP) to handle startup race conditions.
    ///
    /// # Example
    ///
    /// ```
    /// use mom_rpc::{RpcConfig, RetryConfig};
    ///
    /// let config = RpcConfig::with_broker("mqtt://localhost:1883", "client")
    ///     .with_retry(RetryConfig::default());
    /// ```
    pub fn with_retry(mut self, config: RetryConfig) -> Self {
        self.retry_config = Some(config);
        self
    }

    /// Set the request timeout per attempt.
    ///
    /// This is how long to wait for a response on each request attempt.
    /// When combined with retry, the total request time is:
    ///   (request_timeout × max_attempts) + backoff delays
    ///
    /// Default: 30 seconds
    ///
    /// # Example
    ///
    /// ```
    /// use mom_rpc::RpcConfig;
    /// use std::time::Duration;
    ///
    /// let config = RpcConfig::with_broker("mqtt://localhost:1883", "client")
    ///     .with_request_timeout(Duration::from_secs(10));
    /// ```
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
}
