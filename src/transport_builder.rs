//! Transport builder for creating transport instances.
//!
//! Provides a fluent builder API for constructing transports with clear separation
//! between required and optional configuration.

use crate::{Result, RpcError, TransportPtr};

/// Builder for creating transport instances.
///
/// Provides both explicit queue configuration and mode-based sugar methods.
///
/// # Examples
///
/// ## Client mode with sugar method
/// ```no_run
/// use mom_rpc::TransportBuilder;
///
/// # async fn example() -> mom_rpc::Result<()> {
/// let transport = TransportBuilder::new()
///     .uri("mqtt://localhost:1883")
///     .node_id("sensor-client")
///     .client_mode()  // Auto-generates response_queue("responses/sensor-client")
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Server mode with explicit queue
/// ```no_run
/// use mom_rpc::TransportBuilder;
///
/// # async fn example() -> mom_rpc::Result<()> {
/// let transport = TransportBuilder::new()
///     .uri("mqtt://localhost:1883")
///     .node_id("server-42")
///     .request_queue("requests/server-42")  // Explicit queue name
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Full-duplex mode
/// ```no_run
/// use mom_rpc::TransportBuilder;
///
/// # async fn example() -> mom_rpc::Result<()> {
/// let transport = TransportBuilder::new()
///     .uri("mqtt://localhost:1883")
///     .node_id("edge-agent")
///     .full_duplex()  // Auto-generates both queues
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct TransportBuilder {
    uri: Option<String>,
    node_id: Option<String>,
    request_queue: Option<String>,
    response_queue: Option<String>,
    transport_type: Option<String>,
    keep_alive_secs: Option<u16>,

    // Track which sugar methods were called (conflicts detected at build())
    called_client_mode: bool,
    called_server_mode: bool,
    called_full_duplex: bool,
}

impl TransportBuilder {
    /// Create a new transport builder.
    pub fn new() -> Self {
        Self {
            uri: None,
            node_id: None,
            request_queue: None,
            response_queue: None,
            transport_type: None,
            keep_alive_secs: None,
            called_client_mode: false,
            called_server_mode: false,
            called_full_duplex: false,
        }
    }

    /// Set the transport URI (required).
    ///
    /// Examples:
    /// - `"mqtt://localhost:1883"`
    /// - `"amqp://localhost:5672/%2f"`
    /// - `"dds:45"` (DDS domain 45)
    pub fn uri(mut self, uri: impl Into<String>) -> Self {
        self.uri = Some(uri.into());
        self
    }

    /// Set the node ID (required).
    ///
    /// Used to generate default queue names when using mode sugar methods.
    pub fn node_id(mut self, id: impl Into<String>) -> Self {
        self.node_id = Some(id.into());
        self
    }

    /// Set explicit request queue name.
    ///
    /// Cannot be used together with mode sugar methods (`client_mode()`, etc).
    pub fn request_queue(mut self, queue: impl Into<String>) -> Self {
        self.request_queue = Some(queue.into());
        self
    }

    /// Set explicit response queue name.
    ///
    /// Cannot be used together with mode sugar methods (`client_mode()`, etc).
    pub fn response_queue(mut self, queue: impl Into<String>) -> Self {
        self.response_queue = Some(queue.into());
        self
    }

    /// Configure for client mode (sugar method).
    ///
    /// Auto-generates: `response_queue("responses/{node_id}")`
    ///
    /// Cannot be used together with explicit queue methods or other sugar methods.
    pub fn client_mode(mut self) -> Self {
        self.called_client_mode = true;
        self
    }

    /// Configure for server mode (sugar method).
    ///
    /// Auto-generates: `request_queue("requests/{node_id}")`
    ///
    /// Cannot be used together with explicit queue methods or other sugar methods.
    pub fn server_mode(mut self) -> Self {
        self.called_server_mode = true;
        self
    }

    /// Configure for full-duplex mode (sugar method).
    ///
    /// Auto-generates both:
    /// - `request_queue("requests/{node_id}")`
    /// - `response_queue("responses/{node_id}")`
    ///
    /// Cannot be used together with explicit queue methods or other sugar methods.
    pub fn full_duplex(mut self) -> Self {
        self.called_full_duplex = true;
        self
    }

    /// Set explicit transport type.
    ///
    /// Valid values: `"memory"`, `"rumqttc"`, `"lapin"`, `"dust-dds"`
    ///
    /// If not specified, uses feature-flag driven selection (current behavior).
    pub fn transport_type(mut self, flag: impl Into<String>) -> Self {
        self.transport_type = Some(flag.into());
        self
    }

    /// Set broker keep-alive interval in seconds.
    ///
    /// If not specified, uses transport default.
    pub fn keep_alive_secs(mut self, secs: u16) -> Self {
        self.keep_alive_secs = Some(secs);
        self
    }

    /// Build the transport (consumes self).
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Required fields missing (`uri`, `node_id`)
    /// - No mode specified (no queues set)
    /// - Multiple sugar methods called
    /// - Both sugar methods and explicit queues used
    /// - Transport creation fails
    pub async fn build(mut self) -> Result<TransportPtr> {
        // Validate required fields
        let uri = self
            .uri
            .ok_or_else(|| RpcError::MissingConfig("uri".into()))?;
        let node_id = self
            .node_id
            .as_ref()
            .ok_or_else(|| RpcError::MissingConfig("node_id".into()))?;

        // Check how many sugar methods were called
        let sugar_count = [
            self.called_client_mode,
            self.called_server_mode,
            self.called_full_duplex,
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        // Detect multiple sugar methods
        if sugar_count > 1 {
            return Err(RpcError::ConfigConflict(
                "Cannot call multiple mode sugar methods (client_mode, server_mode, full_duplex)"
                    .into(),
            ));
        }

        // Check for conflicts between sugar and explicit
        let has_explicit_queues = self.request_queue.is_some() || self.response_queue.is_some();
        if sugar_count > 0 && has_explicit_queues {
            return Err(RpcError::ConfigConflict(
                "Cannot use both mode sugar methods and explicit queue configuration".into(),
            ));
        }

        // If sugar mode was used, generate queue names based on which flag is set
        if self.called_client_mode {
            self.response_queue = Some(format!("responses/{}", node_id));
        } else if self.called_server_mode {
            self.request_queue = Some(format!("requests/{}", node_id));
        } else if self.called_full_duplex {
            self.request_queue = Some(format!("requests/{}", node_id));
            self.response_queue = Some(format!("responses/{}", node_id));
        }

        // Validate at least one queue is specified
        if self.request_queue.is_none() && self.response_queue.is_none() {
            return Err(RpcError::MissingConfig(
                "at least one queue (request or response) or a mode method".into(),
            ));
        }

        // Infer mode from which queues are set
        let mode = match (&self.request_queue, &self.response_queue) {
            (Some(_), Some(_)) => crate::TransportMode::FullDuplex,
            (Some(_), None) => crate::TransportMode::Server,
            (None, Some(_)) => crate::TransportMode::Client,
            (None, None) => unreachable!("validated above"),
        };

        let config = crate::TransportConfig {
            uri,
            node_id: self.node_id.unwrap(),
            mode,
            request_queue: self.request_queue,
            response_queue: self.response_queue,
            transport_type: self.transport_type.clone(),
            keep_alive_secs: self.keep_alive_secs,
        };

        // Dispatch to the appropriate transport factory.
        //
        // When transport_type is explicit, use it directly.
        // When None, try each factory in priority order (dust_dds → rumqttc → lapin → memory).
        // Disabled transports return Err immediately via the Null Object stubs, so the
        // first Ok() wins. Memory is the unconditional fallback.
        match self.transport_type.as_deref() {
            Some("rumqttc") => crate::create_rumqttc_transport(config).await,
            Some("lapin") => crate::create_lapin_transport(config).await,
            Some("dust-dds") => crate::create_dust_dds_transport(config).await,
            Some("memory") => crate::create_memory_transport(config).await,
            Some(other) => Err(RpcError::Transport(format!(
                "unrecognized transport_type: {other}, valid values: memory, rumqttc, lapin, dust-dds"
            ))),
            None => {
                if let Ok(t) = crate::create_dust_dds_transport(config.clone()).await {
                    return Ok(t);
                }
                if let Ok(t) = crate::create_rumqttc_transport(config.clone()).await {
                    return Ok(t);
                }
                if let Ok(t) = crate::create_lapin_transport(config.clone()).await {
                    return Ok(t);
                }
                crate::create_memory_transport(config).await
            }
        }
    }
}

impl Default for TransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}
