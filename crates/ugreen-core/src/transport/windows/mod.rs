//! Windows transport: Winsock Bluetooth (RFCOMM) through `windows-sys`.
//!
//! Like the Linux transport, every command opens a connection, does one
//! exchange and closes it. All `unsafe` is confined to `socket.rs`, `sdp.rs`
//! and `devices.rs`.
#![deny(clippy::undocumented_unsafe_blocks)]

mod devices;
mod sdp;
mod socket;

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

use self::devices::PairedDevice;
use self::socket::{RfcommSocket, WsaError, ensure_winsock};
use super::error::{ConnectFailure, classify_winsock_connect_error};
use super::exchange::{Channel, ReadOutcome, exchange};
use super::{Transport, TransportError, TransportTarget, bth_addr_to_mac, mac_to_bth_addr};
use crate::discovery::{BluetoothDevice, RfcommService};
use crate::protocol::{DecodedFrame, ProtocolCommand, ProtocolFacade, UgProtocolFacade};

/// Same value the Linux transport uses for SO_SNDTIMEO / SO_RCVTIMEO.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(3);

pub struct WindowsTransport {
    facade: Box<dyn ProtocolFacade>,
    sequence: u8,
    /// RFCOMM channel that last worked, per device address.
    known_channels: HashMap<u64, u8>,
}

impl Default for WindowsTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsTransport {
    pub fn new() -> Self {
        Self {
            facade: Box::new(UgProtocolFacade::new()),
            sequence: 0,
            known_channels: HashMap::new(),
        }
    }

    fn next_sequence(&mut self) -> u8 {
        let seq = self.sequence;
        self.sequence = self.sequence.wrapping_add(1) % 16;
        seq
    }

    /// Connects on the last known (or requested) channel; if that fails, asks
    /// the headset's SDP server for its Serial Port channel(s) and tries those.
    fn connect(&mut self, target: &TransportTarget) -> Result<RfcommSocket> {
        let mac = target.mac_address.as_str();
        let address = mac_to_bth_addr(mac)?;
        start_winsock()?;
        let device = require_paired(address, mac)?;

        let first_channel = self
            .known_channels
            .get(&address)
            .copied()
            .unwrap_or(target.rfcomm_channel);
        let first_error = match open_channel(address, first_channel) {
            Ok(socket) => {
                log::info!("connected to {mac} on RFCOMM channel {first_channel}");
                self.known_channels.insert(address, first_channel);
                return Ok(socket);
            }
            Err(code) => code,
        };
        self.known_channels.remove(&address);

        if classify_winsock_connect_error(first_error) == ConnectFailure::RadioUnavailable {
            return Err(connect_error(mac, first_channel, first_error, &device).into());
        }

        log::warn!(
            "RFCOMM channel {first_channel} on {mac} failed ({}); asking SDP for the Serial Port channel",
            describe_wsa_error(first_error)
        );
        // ASSUMPTION: the control channel is advertised as Serial Port
        // (0x1101). Other RFCOMM services (Hands-Free, Headset) are never
        // tried because they speak a different protocol.
        match sdp::lookup(address, sdp::UUID_SERIAL_PORT, true) {
            Ok(services) => {
                for service in services.iter().filter(|s| s.channel != first_channel) {
                    match open_channel(address, service.channel) {
                        Ok(socket) => {
                            log::info!(
                                "connected to {mac} on RFCOMM channel {} (found via SDP, service `{}`)",
                                service.channel,
                                service.name
                            );
                            self.known_channels.insert(address, service.channel);
                            return Ok(socket);
                        }
                        Err(code) => log::warn!(
                            "RFCOMM channel {} (from SDP) on {mac} failed ({})",
                            service.channel,
                            describe_wsa_error(code)
                        ),
                    }
                }
                if services.iter().all(|s| s.channel == first_channel) {
                    log::warn!("SDP offered no other Serial Port channel for {mac}");
                }
            }
            Err(code) => log::warn!("SDP lookup on {mac} failed ({})", describe_wsa_error(code)),
        }

        Err(connect_error(mac, first_channel, first_error, &device).into())
    }
}

impl Transport for WindowsTransport {
    fn send(
        &mut self,
        target: &TransportTarget,
        command: &ProtocolCommand,
        payload: &Value,
    ) -> Result<DecodedFrame> {
        let seq = self.next_sequence();
        let inner = self
            .facade
            .encode_inner(target.protocol, command, payload, seq)
            .context("failed to encode protocol packet")?;
        let mut socket = self.connect(target)?;
        exchange(
            &mut socket,
            self.facade.as_ref(),
            target.protocol,
            command,
            &inner,
            seq,
        )
    }
}

impl Channel for RfcommSocket {
    fn write_all(&mut self, mut bytes: &[u8]) -> Result<()> {
        while !bytes.is_empty() {
            let sent = self
                .send(bytes)
                .map_err(|code| anyhow!("send failed: {}", describe_wsa_error(code)))?;
            if sent == 0 {
                return Err(anyhow!("send accepted no bytes"));
            }
            bytes = &bytes[sent..];
        }
        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> Result<ReadOutcome> {
        const WSAETIMEDOUT: WsaError = 10060;

        let readable = self
            .wait_readable(timeout)
            .map_err(|code| anyhow!("select failed: {}", describe_wsa_error(code)))?;
        if !readable {
            return Ok(ReadOutcome::TimedOut);
        }
        match self.recv(buffer) {
            Ok(0) => Ok(ReadOutcome::Closed),
            Ok(read) => Ok(ReadOutcome::Data(read)),
            Err(WSAETIMEDOUT) => Ok(ReadOutcome::TimedOut),
            Err(code) => Err(anyhow!("recv failed: {}", describe_wsa_error(code))),
        }
    }
}

fn start_winsock() -> Result<()> {
    ensure_winsock().map_err(|code| anyhow!("WSAStartup failed: {}", describe_wsa_error(code)))
}

fn open_channel(address: u64, channel: u8) -> Result<RfcommSocket, WsaError> {
    let socket = RfcommSocket::open()?;
    socket.set_timeouts(SOCKET_TIMEOUT)?;
    socket.connect(address, channel)?;
    Ok(socket)
}

fn list_paired() -> Result<Vec<PairedDevice>> {
    if !devices::radio_present() {
        return Err(TransportError::BluetoothUnavailable {
            detail: "Windows reports no Bluetooth radio".into(),
        }
        .into());
    }
    devices::paired_devices().map_err(|code| {
        anyhow!(
            "listing paired Bluetooth devices failed: {}",
            std::io::Error::from_raw_os_error(code as i32)
        )
    })
}

fn require_paired(address: u64, mac: &str) -> Result<PairedDevice> {
    list_paired()?
        .into_iter()
        .find(|device| device.address == address)
        .ok_or_else(|| {
            TransportError::NotPaired {
                mac: mac.to_string(),
            }
            .into()
        })
}

fn connect_error(mac: &str, channel: u8, code: WsaError, device: &PairedDevice) -> TransportError {
    let mac = mac.to_string();
    let mut detail = describe_wsa_error(code);
    if !device.connected {
        detail.push_str("; Windows shows the headset as not connected");
    }
    match classify_winsock_connect_error(code) {
        ConnectFailure::RadioUnavailable => TransportError::BluetoothUnavailable { detail },
        ConnectFailure::Unreachable => TransportError::DeviceUnreachable { mac, detail },
        ConnectFailure::Busy => TransportError::ChannelBusy { mac, channel },
        ConnectFailure::Refused | ConnectFailure::Other => TransportError::ConnectFailed {
            mac,
            channel,
            detail,
        },
    }
}

/// "<system message> (os error <code>)".
fn describe_wsa_error(code: WsaError) -> String {
    std::io::Error::from_raw_os_error(code).to_string()
}

/// Paired devices, for `crate::discovery`.
pub(crate) fn paired_bluetooth_devices() -> Result<Vec<BluetoothDevice>> {
    Ok(list_paired()?
        .into_iter()
        .map(|device| BluetoothDevice {
            mac: bth_addr_to_mac(device.address),
            name: device.name,
            connected: Some(device.connected),
        })
        .collect())
}

/// Every RFCOMM service the device advertises over SDP, for `crate::discovery`.
pub(crate) fn rfcomm_services(mac: &str) -> Result<Vec<RfcommService>> {
    let address = mac_to_bth_addr(mac)?;
    start_winsock()?;
    require_paired(address, mac)?;

    let query = |uuid: u16, flush_cache: bool| {
        sdp::lookup(address, uuid, flush_cache)
            .map_err(|code| anyhow!("SDP lookup on {mac} failed: {}", describe_wsa_error(code)))
    };
    // One over-the-air query for everything on RFCOMM, then cached queries to
    // label the well-known profiles.
    let all = query(sdp::UUID_RFCOMM, true)?;
    let mut profiles = HashMap::new();
    for (uuid, label) in [
        (sdp::UUID_HEADSET, "Headset"),
        (sdp::UUID_HANDSFREE, "Hands-Free"),
        (sdp::UUID_SERIAL_PORT, "Serial Port"),
    ] {
        for service in query(uuid, false)? {
            profiles.insert(service.channel, label);
        }
    }

    let mut services = all
        .into_iter()
        .map(|service| RfcommService {
            profile: profiles.get(&service.channel).copied(),
            name: service.name,
            channel: service.channel,
        })
        .collect::<Vec<_>>();
    services.sort_by_key(|service| service.channel);
    services.dedup_by_key(|service| service.channel);
    Ok(services)
}
