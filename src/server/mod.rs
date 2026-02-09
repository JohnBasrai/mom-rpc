//! RPC client.

mod handler;
mod rpc_server;

use handler::{wrap_handler, BoxedHandler};
pub use rpc_server::RpcServer;
