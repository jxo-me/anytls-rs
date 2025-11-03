//! AnyTLS protocol implementation in Rust
//!
//! A proxy protocol attempting to mitigate TLS in TLS fingerprinting issues.
//!
//! # Architecture
//!
//! - **protocol**: Frame and codec implementation
//! - **session**: Session and stream management
//! - **padding**: Traffic obfuscation padding
//! - **util**: Utilities (error handling, auth, TLS config)
//! - **client**: Client implementation
//! - **server**: Server implementation

/// Protocol layer: Frame and codec implementation
pub mod protocol;
/// Session layer: Session and stream management
pub mod session;
/// Padding module for traffic obfuscation
pub mod padding;
/// Utility modules (error, auth, TLS, etc.)
pub mod util;
/// Client implementation
pub mod client;
/// Server implementation
pub mod server;

pub use protocol::*;
pub use session::*;
pub use padding::*;
pub use util::*;
pub use client::*;

// Re-export commonly used types
pub use util::error::{AnyTlsError, Result};
pub use util::auth::{hash_password, authenticate_client, send_authentication};
