//! ade-core — runtime layer of the Agent Development Environment.
//!
//! Owns the state model mirrored from an `opencode serve` instance:
//! SSE events are low-latency hints, REST polling is authoritative.

pub mod config;
pub mod context;
pub mod runtime;
pub mod server;
pub mod state;

pub use config::AppConfig;
pub use runtime::{Command, PermissionResponse, RuntimeHandle};
pub use server::AdeServer;
pub use state::{ConnState, MessageEntry, PendingPermission, Store, TokenTotals};
