/// RPC client for sending requests and awaiting responses
mod pending;

use crate::error::{Error, Result};
use crate::protocol::{CorrelationId, RpcRequest, RpcResponse};
use bytes::Bytes;
use pending::PendingRequests;
use rumqttc::{AsyncClient, Publish, QoS};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// RPC client for sending requests over MQTT
///
/// Creates a client that can send requests to MQTT topics and await responses.
/// Multiple requests can be in-flight concurrently, each identified by a unique
/// correlation ID.
///
/// # Example
///
/// ```no_run
/// use mqtt_rpc_rs::RpcClient;
/// use rumqttc::{MqttOptions, AsyncClient};
/// use std::time::Duration;
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Serialize)]
/// struct Request { value: i32 }
///
/// #[derive(Deserialize)]
/// struct Response { result: i32 }
///
/// # async fn example() -> Result<(), mqtt_rpc_rs::Error> {
/// let mqtt_options = MqttOptions::new("client-01", "localhost", 1883);
/// let (mqtt_client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
///
/// tokio::spawn(async move {
///     loop { eventloop.poll().await.ok(); }
/// });
///
/// let rpc_client = RpcClient::new(mqtt_client, "client-01").await?;
///
/// let response: Response = rpc_client
///     .request("math/add", &Request { value: 5 })
///     .timeout(Duration::from_secs(5))
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct RpcClient {
    // ---
    mqtt: AsyncClient,
    client_id: String,
    pending: Arc<Mutex<PendingRequests>>,
}

impl RpcClient {
    // ---

    /// Create a new RPC client
    ///
    /// # Arguments
    ///
    /// * `mqtt` - The MQTT async client to use for communication
    /// * `client_id` - Unique identifier for this client (used for response topic)
    ///
    /// # Behavior
    ///
    /// - Subscribes to response topic: `responses/{client_id}`
    /// - Spawns background task to handle incoming responses
    ///
    /// # Errors
    ///
    /// Returns an error if subscription fails.
    pub async fn new(mqtt: AsyncClient, client_id: impl Into<String>) -> Result<Self> {
        // ---
        let client_id = client_id.into();
        let response_topic = format!("responses/{}", client_id);

        // Subscribe to response topic
        mqtt.subscribe(&response_topic, QoS::AtLeastOnce)
            .await
            .map_err(|e| rumqttc::ClientError::from(e));

        let pending = Arc::new(Mutex::new(PendingRequests::new()));

        Ok(Self {
            mqtt,
            client_id,
            pending,
        })
    }

    /// Send an RPC request and return a future that resolves when the response arrives
    ///
    /// # Arguments
    ///
    /// * `topic` - The base topic for the request (will publish to `{topic}/request`)
    /// * `payload` - The request payload to serialize
    ///
    /// # Returns
    ///
    /// A `Result` containing the deserialized response or an error.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Serialization fails
    /// - MQTT publish fails
    /// - Response doesn't arrive (use `request_timeout` for timeout handling)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mqtt_rpc_rs::RpcClient;
    /// # use serde::{Serialize, Deserialize};
    /// # #[derive(Serialize)] struct Req { value: i32 }
    /// # #[derive(Deserialize)] struct Resp { result: i32 }
    /// # async fn example(client: RpcClient) -> Result<(), mqtt_rpc_rs::Error> {
    /// let response: Resp = client
    ///     .request("math/add", &Req { value: 5 })
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn request<Req, Resp>(&self, topic: &str, payload: &Req) -> Result<Resp>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        // ---
        self.request_timeout(topic, payload, None).await
    }

    /// Send an RPC request with a timeout
    ///
    /// # Arguments
    ///
    /// * `topic` - The base topic for the request (will publish to `{topic}/request`)
    /// * `payload` - The request payload to serialize
    /// * `timeout` - Optional timeout duration
    ///
    /// # Returns
    ///
    /// A `Result` containing the deserialized response or an error.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Serialization fails
    /// - MQTT publish fails
    /// - Request times out
    /// - Response deserialization fails
    pub async fn request_timeout<Req, Resp>(
        &self,
        topic: &str,
        payload: &Req,
        timeout: Option<Duration>,
    ) -> Result<Resp>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        // ---
        let correlation_id = CorrelationId::generate();

        // Register pending request
        let rx = {
            let mut pending = self.pending.lock().await;
            pending.register(correlation_id.clone())
        };

        // Send request
        self.send_request(topic, payload, correlation_id.clone())
            .await?;

        // Wait for response with optional timeout
        let response_bytes: Bytes = if let Some(duration) = timeout {
            match tokio::time::timeout(duration, rx).await {
                Ok(Ok(bytes)) => bytes,
                Ok(Err(_)) => {
                    // Receiver dropped (shouldn't happen)
                    let mut pending = self.pending.lock().await;
                    pending.remove(&correlation_id);
                    return Err(Error::ConnectionLost);
                }
                Err(_) => {
                    // Timeout
                    let mut pending = self.pending.lock().await;
                    pending.remove(&correlation_id);
                    return Err(Error::Timeout);
                }
            }
        } else {
            match rx.await {
                Ok(bytes) => bytes,
                Err(_) => return Err(Error::ConnectionLost),
            }
        };

        // Deserialize response
        let response: RpcResponse<Resp> = serde_json::from_slice(&response_bytes)?;
        Ok(response.payload)
    }

    /// Handle an incoming MQTT message
    ///
    /// Extracts correlation ID and delivers response to waiting Future.
    pub(crate) async fn handle_message(&self, publish: Publish) {
        // ---
        // Parse response to extract correlation_id
        let response_result: Result<serde_json::Value> =
            serde_json::from_slice(&publish.payload).map_err(Error::from);

        if let Ok(response_json) = response_result {
            if let Some(correlation_id_str) =
                response_json.get("correlation_id").and_then(|v| v.as_str())
            {
                let correlation_id = CorrelationId::from(correlation_id_str.to_string());
                let mut pending = self.pending.lock().await;
                pending.complete(&correlation_id, publish.payload.clone());
            }
        }
    }

    /// Internal method to send a request
    async fn send_request<Req>(
        &self,
        topic: &str,
        payload: &Req,
        correlation_id: CorrelationId,
    ) -> Result<()>
    where
        Req: Serialize,
    {
        // ---
        let response_topic = format!("responses/{}", self.client_id);
        let request = RpcRequest::new(correlation_id, response_topic, payload);
        let payload_json = serde_json::to_vec(&request)?;

        let request_topic = format!("{}/request", topic);

        self.mqtt
            .publish(request_topic, QoS::AtLeastOnce, false, payload_json)
            .await
            .map_err(|e| rumqttc::ClientError::from(e));

        Ok(())
    }
}
