//! Domain layer public interface.
//!
//! This module defines domain-level abstractions that are independent of
//! transport implementations, protocols, or infrastructure concerns.
//!
//! All domain consumers must import symbols via this module, not by
//! referencing individual files directly.

mod transport;

// --- Transport domain re-exports ---

#[allow(unused)]
pub use transport::{
    //
    Address,
    Envelope,
    PublishOptions,
    Subscription,
    SubscriptionHandle,
    Transport,
    TransportPtr,
};
