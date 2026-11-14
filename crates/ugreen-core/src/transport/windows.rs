//! Windows transport. Stub until phase 2 (Winsock RFCOMM via `windows-sys`).

use anyhow::{Result, anyhow};
use serde_json::Value;

use super::{Transport, TransportTarget};
use crate::protocol::{DecodedFrame, ProtocolCommand};

#[derive(Default)]
pub struct WindowsTransport;

impl WindowsTransport {
    pub fn new() -> Self {
        Self
    }
}

impl Transport for WindowsTransport {
    fn send(
        &mut self,
        _target: &TransportTarget,
        _command: &ProtocolCommand,
        _payload: &Value,
    ) -> Result<DecodedFrame> {
        Err(anyhow!(
            "the Windows Bluetooth transport is not implemented yet"
        ))
    }
}
