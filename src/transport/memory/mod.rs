// src/transport/memory/mod.rs

//! In-memory transport implementation.
//!
//! This module provides a pure in-process implementation of the domain-level
//! `Transport` trait. It is intended primarily for testing, local execution,
//! and as a reference for transport semantics.
//!
//! ## Reference Semantics
//!
//! The in-memory transport defines the **reference behavior** for the transport
//! layer. All other transport implementations are expected to approximate this
//! behavior as closely as their underlying systems allow and to document any
//! unavoidable deviations.
//!
//! In particular, the in-memory transport establishes the following expectations:
//!
//! - Once `subscribe()` returns successfully, messages published *after* that
//!   point and matching the subscription are deliverable.
//! - Message delivery is deterministic within a single process.
//! - No messages are dropped due to timing, scheduling, or background IO.
//!
//! ## Non-Goals
//!
//! This transport does not attempt to emulate the failure modes, persistence,
//! or delivery guarantees of any specific broker. It exists to provide a clear,
//! deterministic baseline against which higher-level behavior can be validated.

mod transport;

use super::SubscriptionManager;
pub use transport::create_transport;
