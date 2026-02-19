//! Redis protocol transports.
//!
//! This module contains transport implementations for Redis.
//! Currently supports:
//! - redis - Redis Pub/Sub via redis library (redis.rs)

#[cfg(feature = "transport_redis")]
#[allow(clippy::module_inception)]
mod redis;

#[cfg(feature = "transport_redis")]
pub use redis::create_transport as create_redis_transport;

#[cfg(not(feature = "transport_redis"))]
pub async fn create_redis_transport(
    _config: crate::TransportConfig,
) -> crate::Result<crate::TransportPtr> {
    Err(crate::RpcError::Transport(
        "transport_redis feature is not enabled".into(),
    ))
}
