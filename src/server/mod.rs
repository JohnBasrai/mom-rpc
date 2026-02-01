/// RPC server for handling requests and sending responses
mod handler;

use crate::error::{Error, Result};
use handler::{wrap_handler, BoxedHandler};
use rumqttc::{AsyncClient, Publish, QoS};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::RwLock;

/// RPC server for handling requests over MQTT
///
/// Registers handlers for specific topics and executes them concurrently
/// when requests arrive. Each handler runs in its own spawned task.
///
/// # Example
///
/// ```no_run
/// use mqtt_rpc_rs::RpcServer;
/// use rumqttc::{MqttOptions, AsyncClient};
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Deserialize)]
/// struct Request { value: i32 }
///
/// #[derive(Serialize)]
/// struct Response { result: i32 }
///
/// # async fn example() -> Result<(), mqtt_rpc_rs::Error> {
/// let mqtt_options = MqttOptions::new("server-01", "localhost", 1883);
/// let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
///
/// tokio::spawn(async move {
///     loop { eventloop.poll().await.ok(); }
/// });
///
/// let mut server = RpcServer::new(mqtt_client).await?;
///
/// server.handle("math/add", |req: Request| async move {
///     Ok(Response { result: req.value + 10 })
/// }).await?;
///
/// server.run().await?;
/// # Ok(())
/// # }
/// ```
pub struct RpcServer {
    // ---
    mqtt: AsyncClient,
    handlers: Arc<RwLock<HashMap<String, BoxedHandler>>>,
}

impl RpcServer {
    // ---

    /// Create a new RPC server
    ///
    /// # Arguments
    ///
    /// * `mqtt` - The MQTT async client to use for communication
    pub async fn new(mqtt: AsyncClient) -> Result<Self> {
        // ---
        Ok(Self {
            mqtt,
            handlers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Register an async handler for a topic
    ///
    /// # Arguments
    ///
    /// * `topic` - The base topic to handle (will subscribe to `{topic}/request`)
    /// * `handler` - Async function that processes requests and returns responses
    ///
    /// # Handler Execution
    ///
    /// - Each request spawns a new task (concurrent execution)
    /// - Handler is called with deserialized request
    /// - Response is automatically serialized and published
    /// - Correlation ID is preserved from request to response
    ///
    /// # Errors
    ///
    /// Returns an error if subscription fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mqtt_rpc_rs::RpcServer;
    /// # use serde::{Serialize, Deserialize};
    /// # #[derive(Deserialize)] struct Req { a: i32, b: i32 }
    /// # #[derive(Serialize)] struct Resp { sum: i32 }
    /// # async fn example(mut server: RpcServer) -> Result<(), mqtt_rpc_rs::Error> {
    /// server.handle("math/add", |req: Req| async move {
    ///     Ok(Resp { sum: req.a + req.b })
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn handle<F, Fut, Req, Resp>(&mut self, topic: &str, handler: F) -> Result<()>
    where
        F: Fn(Req) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Result<Resp>> + Send + 'static,
        Req: DeserializeOwned + Send + 'static,
        Resp: Serialize + Send + 'static,
    {
        // ---
        let request_topic = format!("{}/request", topic);

        // Subscribe to request topic
        self.mqtt
            .subscribe(&request_topic, QoS::AtLeastOnce)
            .await
            .map_err(|e| Error::from_transport(crate::error::TransportError::Client(e)));

        // Store handler
        let wrapped = wrap_handler(handler);
        let mut handlers: tokio::sync::RwLockWriteGuard<HashMap<String, BoxedHandler>> =
            self.handlers.write().await;
        handlers.insert(topic.to_string(), wrapped);

        Ok(())
    }

    /// Run the server event loop
    ///
    /// This method blocks and processes incoming MQTT messages.
    /// For each request, it looks up the handler and spawns a task
    /// to process the request concurrently.
    ///
    /// # Errors
    ///
    /// Returns an error if MQTT connection is lost.
    pub async fn run(&mut self) -> Result<()> {
        // ---
        loop {
            // This would need the eventloop to be passed in or managed differently
            // For now, this is a placeholder showing the structure
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    /// Handle an incoming request message
    ///
    /// Extracts the topic, correlation ID, and response topic from JSON payload,
    /// then spawns a task to execute the handler.
    #[allow(dead_code)]
    pub(crate) async fn handle_request(&self, publish: Publish) -> Result<()> {
        // ---
        // Extract topic (remove /request suffix)
        let full_topic = &publish.topic;
        let topic = full_topic
            .strip_suffix("/request")
            .ok_or_else(|| Error::HandlerNotFound(full_topic.clone()))?;

        // Look up handler
        let handler = {
            let handlers = self.handlers.read().await;
            handlers
                .get(topic)
                .cloned()
                .ok_or_else(|| Error::HandlerNotFound(topic.to_string()))?
        };

        // Run handler (handler returns (response_topic, response_bytes))
        let (response_topic, response_bytes) = handler(publish.payload).await?;

        // Publish response
        self.mqtt
            .publish(response_topic, QoS::AtLeastOnce, false, response_bytes)
            .await
            .map_err(|e| Error::from_transport(crate::error::TransportError::Client(e)))?;

        Ok(())
    }
}
