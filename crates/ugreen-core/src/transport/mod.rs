//! Transport abstraction: one request/response exchange with a device.
//!
//! The platform implementation is selected at compile time and exposed as
//! [`PlatformTransport`], so callers never name an OS-specific type.

use anyhow::Result;
use serde_json::Value;

use crate::protocol::{DecodedFrame, ProtocolCommand, ProtocolType};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::LinuxTransport;
#[cfg(target_os = "windows")]
pub use windows::WindowsTransport;

/// The transport for the OS this crate was compiled for.
#[cfg(target_os = "linux")]
pub type PlatformTransport = LinuxTransport;
/// The transport for the OS this crate was compiled for.
#[cfg(target_os = "windows")]
pub type PlatformTransport = WindowsTransport;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
compile_error!("ugreen-core only has Bluetooth transports for Linux and Windows");

pub struct TransportTarget {
    pub mac_address: String,
    pub protocol: ProtocolType,
    pub rfcomm_channel: u8,
}

pub trait Transport {
    fn send(
        &mut self,
        target: &TransportTarget,
        command: &ProtocolCommand,
        payload: &Value,
    ) -> Result<DecodedFrame>;
}
