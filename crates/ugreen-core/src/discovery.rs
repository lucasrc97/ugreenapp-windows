//! Finding Bluetooth devices the OS already knows about. Nothing here starts
//! a Bluetooth inquiry or pairs anything.

use anyhow::Result;

/// A Bluetooth device as reported by the OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    /// `AA:BB:CC:DD:EE:FF`
    pub mac: String,
    pub name: String,
    /// Whether the OS currently has a link to it, when the OS says.
    pub connected: Option<bool>,
}

/// An RFCOMM service advertised in a device's SDP records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfcommService {
    pub channel: u8,
    pub name: String,
    /// Well-known profile this channel belongs to, when recognised.
    pub profile: Option<&'static str>,
}

/// Lists candidate devices.
///
/// - Linux: devices BlueZ reports as *connected* (`bluetoothctl devices Connected`).
/// - Windows: devices that are *paired*, connected or not.
pub fn list_devices() -> Result<Vec<BluetoothDevice>> {
    #[cfg(target_os = "linux")]
    return linux::connected_devices();
    #[cfg(target_os = "windows")]
    return crate::transport::windows::paired_bluetooth_devices();
}

/// Lists the RFCOMM services of a paired device via SDP. Diagnostic aid for
/// finding the control channel. Windows only for now.
pub fn rfcomm_services(mac: &str) -> Result<Vec<RfcommService>> {
    #[cfg(target_os = "windows")]
    return crate::transport::windows::rfcomm_services(mac);
    #[cfg(not(target_os = "windows"))]
    {
        let _ = mac;
        Err(anyhow::anyhow!(
            "SDP service listing is only implemented on Windows"
        ))
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::BluetoothDevice;
    use anyhow::{Result, anyhow};
    use std::process::Command;

    pub(super) fn connected_devices() -> Result<Vec<BluetoothDevice>> {
        let output = Command::new("bluetoothctl")
            .args(["devices", "Connected"])
            .output()?;
        if !output.status.success() {
            return Err(anyhow!("bluetoothctl devices Connected failed"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .filter_map(parse_bluetoothctl_device_line)
            .map(|(mac, name)| BluetoothDevice {
                mac,
                name,
                connected: Some(true),
            })
            .collect())
    }

    fn parse_bluetoothctl_device_line(line: &str) -> Option<(String, String)> {
        let trimmed = line.trim();
        let remainder = trimmed.strip_prefix("Device ")?;
        let mut parts = remainder.splitn(2, ' ');
        let mac = parts.next()?.trim().to_string();
        let name = parts.next()?.trim().to_string();
        Some((mac, name))
    }

    #[cfg(test)]
    mod tests {
        use super::parse_bluetoothctl_device_line;

        #[test]
        fn parses_bluetoothctl_lines() {
            assert_eq!(
                parse_bluetoothctl_device_line("Device 3C:78:AD:8D:E2:EA UGREEN HiTune Max5c\n"),
                Some(("3C:78:AD:8D:E2:EA".into(), "UGREEN HiTune Max5c".into()))
            );
            assert_eq!(
                parse_bluetoothctl_device_line("No default controller"),
                None
            );
            assert_eq!(
                parse_bluetoothctl_device_line("Device AA:BB:CC:DD:EE:FF"),
                None
            );
        }
    }
}
