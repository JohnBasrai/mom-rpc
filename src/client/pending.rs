use crate::protocol::CorrelationId;
use bytes::Bytes;
use std::collections::HashMap;
use tokio::sync::oneshot;

/// Tracks pending requests waiting for responses
///
/// Uses a HashMap to map correlation IDs to oneshot channels.
/// When a response arrives, the channel is used to deliver the response
/// to the waiting Future.
pub(super) struct PendingRequests {
    // ---
    requests: HashMap<CorrelationId, oneshot::Sender<Bytes>>,
}

impl PendingRequests {
    // ---

    /// Create a new empty pending requests tracker
    pub fn new() -> Self {
        // ---
        Self {
            requests: HashMap::new(),
        }
    }

    /// Register a new pending request
    ///
    /// Returns a receiver that will be notified when the response arrives.
    pub fn register(&mut self, correlation_id: CorrelationId) -> oneshot::Receiver<Bytes> {
        // ---
        let (tx, rx) = oneshot::channel();
        self.requests.insert(correlation_id, tx);
        rx
    }

    /// Complete a pending request with the response payload
    ///
    /// Returns true if the correlation_id was found and the response was delivered.
    pub fn complete(&mut self, correlation_id: &CorrelationId, response: Bytes) -> bool {
        // ---
        if let Some(tx) = self.requests.remove(correlation_id) {
            // Send response (ignore if receiver dropped due to timeout)
            let _ = tx.send(response);
            true
        } else {
            false
        }
    }

    /// Remove a pending request without delivering a response
    ///
    /// Used for timeout cleanup.
    pub fn remove(&mut self, correlation_id: &CorrelationId) -> bool {
        // ---
        self.requests.remove(correlation_id).is_some()
    }

    /// Get the number of pending requests
    pub fn len(&self) -> usize {
        // ---
        self.requests.len()
    }
}

#[cfg(test)]
mod tests {
    // ---
    use super::*;

    #[test]
    fn test_register_and_complete() {
        // ---
        let mut pending = PendingRequests::new();
        let correlation_id = CorrelationId::generate();

        let rx = pending.register(correlation_id.clone());
        assert_eq!(pending.len(), 1);

        let response = Bytes::from("test response");
        assert!(pending.complete(&correlation_id, response.clone()));

        // Should be removed after completion
        assert_eq!(pending.len(), 0);

        // Receiver should get the response
        let received = rx.blocking_recv().unwrap();
        assert_eq!(received, response);
    }

    #[test]
    fn test_remove() {
        // ---
        let mut pending = PendingRequests::new();
        let correlation_id = CorrelationId::generate();

        let _rx = pending.register(correlation_id.clone());
        assert_eq!(pending.len(), 1);

        assert!(pending.remove(&correlation_id));
        assert_eq!(pending.len(), 0);

        // Second remove should return false
        assert!(!pending.remove(&correlation_id));
    }

    #[test]
    fn test_complete_unknown_id() {
        // ---
        let mut pending = PendingRequests::new();
        let correlation_id = CorrelationId::generate();

        let response = Bytes::from("test");
        assert!(!pending.complete(&correlation_id, response));
    }
}
