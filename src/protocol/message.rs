use super::CorrelationId;
use serde::{Deserialize, Serialize};

/// RPC request message with correlation tracking
///
/// This is the wire format for requests. The correlation_id and response_topic
/// are included at the top level to make it easy for hand-coded implementations
/// to extract without full deserialization.
///
/// # JSON Format
///
/// ```json
/// {
///   "correlation_id": "550e8400-e29b-41d4-a9b6-446655440000",
///   "response_topic": "responses/client-01",
///   "field1": "value1",
///   "field2": 42
/// }
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct RpcRequest<T> {
    // ---
    /// Correlation ID for matching response
    pub correlation_id: CorrelationId,

    /// Topic where response should be published
    pub response_topic: String,

    /// Request payload (flattened into JSON)
    #[serde(flatten)]
    pub payload: T,
}

impl<T> RpcRequest<T> {
    // ---

    /// Create a new request with the given correlation ID, response topic, and payload
    pub fn new(correlation_id: CorrelationId, response_topic: String, payload: T) -> Self {
        // ---
        Self {
            correlation_id,
            response_topic,
            payload,
        }
    }
}

/// RPC response message with correlation tracking
///
/// This is the wire format for responses. The correlation_id is included
/// at the top level to make it easy to match with pending requests.
///
/// # JSON Format
///
/// ```json
/// {
///   "correlation_id": "550e8400-e29b-41d4-a9b6-446655440000",
///   "result": "success",
///   "data": 123
/// }
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct RpcResponse<T> {
    // ---
    /// Correlation ID matching the request
    pub correlation_id: CorrelationId,

    /// Response payload (flattened into JSON)
    #[serde(flatten)]
    pub payload: T,
}

impl<T> RpcResponse<T> {
    // ---

    /// Create a new response with the given correlation ID and payload
    pub fn new(correlation_id: CorrelationId, payload: T) -> Self {
        // ---
        Self {
            correlation_id,
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    // ---
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestPayload {
        // ---
        value: i32,
    }

    #[test]
    fn test_request_serialization() {
        // ---
        let correlation_id = CorrelationId::from("test-id".to_string());
        let request = RpcRequest::new(
            correlation_id.clone(),
            "responses/test".to_string(),
            TestPayload { value: 42 },
        );

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: RpcRequest<TestPayload> = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.correlation_id, correlation_id);
        assert_eq!(deserialized.response_topic, "responses/test");
        assert_eq!(deserialized.payload.value, 42);
    }

    #[test]
    fn test_response_serialization() {
        // ---
        let correlation_id = CorrelationId::from("test-id".to_string());
        let response = RpcResponse::new(correlation_id.clone(), TestPayload { value: 123 });

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: RpcResponse<TestPayload> = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.correlation_id, correlation_id);
        assert_eq!(deserialized.payload.value, 123);
    }

    #[test]
    fn test_flattened_serialization() {
        // ---
        let correlation_id = CorrelationId::from("test-id".to_string());
        let request = RpcRequest::new(
            correlation_id,
            "responses/test".to_string(),
            TestPayload { value: 42 },
        );

        let json = serde_json::to_value(&request).unwrap();

        // Verify correlation_id, response_topic, and payload fields are at same level
        assert!(json.get("correlation_id").is_some());
        assert!(json.get("response_topic").is_some());
        assert!(json.get("value").is_some());
        assert!(json.get("payload").is_none()); // Should be flattened
    }
}
