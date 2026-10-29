//! refresh-core: the transport, identity, and pairing layer shared by
//! every Refresh client (Windows host, Android client, and any future
//! platform). This crate deliberately contains no screen-capture,
//! input-injection, or UI code — only the parts that have to be
//! correct for the whole system to be secure.

pub mod identity;
pub mod io;
pub mod pairing;
pub mod protocol;
pub mod transport;

/// Fixed UDP port Refresh listens on. Bound on all interfaces, which in
/// practice means it is reachable over whatever network interfaces the
/// OS has — including a Tailscale virtual interface — without any
/// Refresh-specific network configuration.
pub const DEFAULT_PORT: u16 = 7878;

pub use identity::Identity;
pub use pairing::TrustStore;
