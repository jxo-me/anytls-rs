//! AnyTLS protocol implementation in Rust
//!
//! A proxy protocol attempting to mitigate TLS in TLS fingerprinting issues.

pub mod protocol;
pub mod session;
pub mod padding;
pub mod util;
pub mod client;
pub mod server;

pub use protocol::*;
pub use session::*;
pub use padding::*;
pub use util::*;
pub use client::*;

// Re-export commonly used types
pub use util::error::{AnyTlsError, Result};
pub use util::auth::{hash_password, authenticate_client, send_authentication};
