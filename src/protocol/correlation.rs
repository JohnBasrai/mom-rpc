use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Correlation ID for matching requests to responses
///
/// Uses UUID v4 in standard 36-byte string format for collision-free
/// identification across device reboots and concurrent requests.
///
/// # Format
///
/// Standard UUID format: `550e8400-e29b-41d4-a9b6-446655440000`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationId(String);

impl CorrelationId {
    // ---

    /// Generate a new unique correlation ID
    pub fn generate() -> Self {
        // ---
        Self(Uuid::new_v4().to_string())
    }

    /// Get the correlation ID as a string slice
    pub fn as_str(&self) -> &str {
        // ---
        &self.0
    }
}

impl fmt::Display for CorrelationId {
    // ---

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // ---
        write!(f, "{}", self.0)
    }
}

impl From<String> for CorrelationId {
    // ---

    fn from(s: String) -> Self {
        // ---
        Self(s)
    }
}

impl From<Uuid> for CorrelationId {
    // ---

    fn from(uuid: Uuid) -> Self {
        // ---
        Self(uuid.to_string())
    }
}

#[cfg(test)]
mod tests {
    // ---
    use super::*;

    #[test]
    fn test_generate_unique() {
        // ---
        let id1 = CorrelationId::generate();
        let id2 = CorrelationId::generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_format() {
        // ---
        let id = CorrelationId::generate();
        let s = id.to_string();
        assert_eq!(s.len(), 36); // Standard UUID format
    }
}
