//! Shared request/response types for the math example.
//!
//! This demonstrates the recommended pattern for production applications:
//! define shared wire types in a separate module (or crate) that both
//! client and server depend on.
//!
//! For real applications, create a dedicated crate:
//! ```text
//! my-app/
//! ├── my-rpc-types/     # Shared types
//! ├── my-server/        # Depends on my-rpc-types
//! └── my-client/        # Depends on my-rpc-types
//! ```
use serde::{Deserialize, Serialize};

/// Request to add two numbers.
#[derive(Debug, Serialize, Deserialize)]
pub struct AddRequest {
    pub a: i32,
    pub b: i32,
}

/// Response containing the sum.
#[derive(Debug, Serialize, Deserialize)]
pub struct AddResponse {
    pub sum: i32,
}
