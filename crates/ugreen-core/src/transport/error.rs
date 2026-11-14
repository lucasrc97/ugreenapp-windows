//! Typed transport failures, so callers (CLI, GUI) can tell them apart.
//!
//! Errors are returned inside `anyhow::Error`; use
//! `error.downcast_ref::<TransportError>()` to branch on the kind.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// No usable Bluetooth radio: adapter missing, disabled or turned off.
    BluetoothUnavailable { detail: String },
    /// The OS does not list the device as paired.
    NotPaired { mac: String },
    /// Paired, but it did not answer the connection attempt in time.
    DeviceUnreachable { mac: String, detail: String },
    /// Another program already holds this RFCOMM channel.
    ChannelBusy { mac: String, channel: u8 },
    /// Any other connection failure.
    ConnectFailed {
        mac: String,
        channel: u8,
        detail: String,
    },
    /// Connected and sent, but no complete response arrived in time.
    Timeout { detail: String },
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BluetoothUnavailable { detail } => write!(
                f,
                "Bluetooth is off or no Bluetooth adapter was found ({detail})"
            ),
            Self::NotPaired { mac } => write!(
                f,
                "{mac} is not paired with this computer; pair the headset in the system Bluetooth settings first"
            ),
            Self::DeviceUnreachable { mac, detail } => write!(
                f,
                "timed out connecting to {mac}; is the headset on and in range? ({detail})"
            ),
            Self::ChannelBusy { mac, channel } => write!(
                f,
                "RFCOMM channel {channel} on {mac} is busy; another program (or another copy of this one) is using it"
            ),
            Self::ConnectFailed {
                mac,
                channel,
                detail,
            } => write!(
                f,
                "RFCOMM connect to {mac} channel {channel} failed ({detail})"
            ),
            Self::Timeout { detail } => {
                write!(f, "timed out waiting for the headset to answer ({detail})")
            }
        }
    }
}

impl std::error::Error for TransportError {}

/// What a failed Winsock `socket()`/`connect()` on a Bluetooth socket means.
#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectFailure {
    RadioUnavailable,
    Unreachable,
    Busy,
    Refused,
    Other,
}

/// Maps a Winsock error code to a [`ConnectFailure`].
///
/// ASSUMPTION: which code Windows returns in each situation is taken from
/// Winsock documentation and field reports, not from tests on this hardware.
/// Every error message also carries the raw code so mismatches are visible.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn classify_winsock_connect_error(code: i32) -> ConnectFailure {
    const WSAEPROTONOSUPPORT: i32 = 10043;
    const WSAEAFNOSUPPORT: i32 = 10047;
    const WSAEADDRINUSE: i32 = 10048;
    const WSAENETDOWN: i32 = 10050;
    const WSAENETUNREACH: i32 = 10051;
    const WSAETIMEDOUT: i32 = 10060;
    const WSAECONNREFUSED: i32 = 10061;
    const WSAEHOSTDOWN: i32 = 10064;
    const WSAEHOSTUNREACH: i32 = 10065;

    match code {
        WSAEAFNOSUPPORT | WSAEPROTONOSUPPORT | WSAENETDOWN => ConnectFailure::RadioUnavailable,
        WSAETIMEDOUT | WSAEHOSTDOWN | WSAEHOSTUNREACH | WSAENETUNREACH => {
            ConnectFailure::Unreachable
        }
        WSAEADDRINUSE => ConnectFailure::Busy,
        WSAECONNREFUSED => ConnectFailure::Refused,
        _ => ConnectFailure::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_winsock_codes() {
        use ConnectFailure::*;
        for (code, expected) in [
            (10047, RadioUnavailable),
            (10043, RadioUnavailable),
            (10050, RadioUnavailable),
            (10060, Unreachable),
            (10064, Unreachable),
            (10065, Unreachable),
            (10051, Unreachable),
            (10048, Busy),
            (10061, Refused),
            (10013, Other),
            (0, Other),
        ] {
            assert_eq!(classify_winsock_connect_error(code), expected, "{code}");
        }
    }

    #[test]
    fn messages_name_the_problem() {
        let mac = "AA:BB:CC:DD:EE:FF".to_string();
        let off = TransportError::BluetoothUnavailable { detail: "x".into() }.to_string();
        assert!(off.contains("Bluetooth is off"));
        let unpaired = TransportError::NotPaired { mac: mac.clone() }.to_string();
        assert!(unpaired.contains("not paired"));
        let busy = TransportError::ChannelBusy {
            mac: mac.clone(),
            channel: 1,
        }
        .to_string();
        assert!(busy.contains("channel 1") && busy.contains("busy"));
        let timeout = TransportError::Timeout { detail: "x".into() }.to_string();
        assert!(timeout.contains("timed out"));
        let unreachable = TransportError::DeviceUnreachable {
            mac,
            detail: "x".into(),
        }
        .to_string();
        assert!(unreachable.contains("timed out connecting"));
    }
}
