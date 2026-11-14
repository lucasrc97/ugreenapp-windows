//! The Bluetooth worker thread.
//!
//! Every Bluetooth call blocks (a failed connect can take ~20 s), so none of
//! them run on the UI thread. The UI sends [`Request`]s over a channel and
//! reads [`Event`]s back; the worker wakes the UI after each event.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::json;
use ugreen_core::discovery::{self, BluetoothDevice};
use ugreen_core::protocol::{DecodedFrame, ProtocolCommand, ProtocolType};
use ugreen_core::transport::{PlatformTransport, Transport, TransportTarget};

use crate::model::{AncChoice, HeadsetState, Setting, supported_protocol};

/// RFCOMM channel tried first; the Windows transport falls back to SDP.
const DEFAULT_RFCOMM_CHANNEL: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceEntry {
    pub mac: String,
    pub name: String,
    pub connected: Option<bool>,
    pub supported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Scan,
    Connect(DeviceEntry),
    Refresh,
    Set(Setting),
    Disconnect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Devices(Result<Vec<DeviceEntry>, String>),
    Connected(DeviceEntry, HeadsetState),
    ConnectFailed(String),
    /// State re-read from the headset (after a refresh or a change).
    State(HeadsetState),
    /// A refresh or change failed; the previous state is still shown.
    Failed(String),
}

/// What the worker needs from the outside world.
pub trait Backend {
    fn list_devices(&mut self) -> Result<Vec<BluetoothDevice>>;
    fn send(&mut self, mac: &str, command: &ProtocolCommand) -> Result<DecodedFrame>;
}

/// The real headset, through the platform transport of `ugreen-core`.
pub struct BluetoothBackend {
    transport: PlatformTransport,
}

impl BluetoothBackend {
    pub fn new() -> Self {
        Self {
            transport: PlatformTransport::new(),
        }
    }
}

impl Backend for BluetoothBackend {
    fn list_devices(&mut self) -> Result<Vec<BluetoothDevice>> {
        discovery::list_devices()
    }

    fn send(&mut self, mac: &str, command: &ProtocolCommand) -> Result<DecodedFrame> {
        let protocol = ProtocolType::Ug1;
        let target = TransportTarget {
            mac_address: mac.to_string(),
            protocol,
            rfcomm_channel: DEFAULT_RFCOMM_CHANNEL,
        };
        self.transport
            .send(&target, command, &command.payload(protocol))
    }
}

/// A pretend headset for trying the GUI without Bluetooth (`--demo`).
pub struct DemoBackend {
    delay: Duration,
    anc: AncChoice,
    eq: String,
    language: String,
    volume: u8,
}

impl DemoBackend {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            anc: AncChoice::Off,
            eq: "treble".into(),
            language: "English".into(),
            volume: 9,
        }
    }
}

impl Backend for DemoBackend {
    fn list_devices(&mut self) -> Result<Vec<BluetoothDevice>> {
        thread::sleep(self.delay);
        Ok(vec![
            BluetoothDevice {
                mac: "00:11:22:33:44:55".into(),
                name: "UGREEN HiTune Max5c (demo)".into(),
                connected: Some(true),
            },
            BluetoothDevice {
                mac: "00:11:22:33:44:66".into(),
                name: "Some Other Speaker (demo)".into(),
                connected: Some(false),
            },
        ])
    }

    fn send(&mut self, _mac: &str, command: &ProtocolCommand) -> Result<DecodedFrame> {
        thread::sleep(self.delay);
        let data = match command {
            ProtocolCommand::QueryFirmware => json!({ "firmwareVersion": "0.0.0-demo" }),
            ProtocolCommand::QueryInfo => {
                let (mode, depth) = self.anc.to_report();
                json!({
                    "battery": { "left": 90 },
                    "ancMode": mode,
                    "ancDepth": depth,
                    "eqMode": self.eq,
                    "promptLanguage": self.language,
                    "promptVolume": self.volume,
                })
            }
            ProtocolCommand::SetAnc { mode, .. } => {
                self.anc = AncChoice::from_protocol_mode(mode)
                    .ok_or_else(|| anyhow!("demo headset: unknown noise mode `{mode}`"))?;
                json!({ "success": true })
            }
            ProtocolCommand::SetEq { mode } => {
                self.eq = mode.clone();
                json!({ "success": true })
            }
            ProtocolCommand::SetPromptLanguage { value } => {
                self.language = value.clone();
                json!({ "success": true })
            }
            ProtocolCommand::SetPromptVolume { value } => {
                self.volume = *value;
                json!({ "success": true })
            }
            other => {
                return Err(anyhow!("demo headset: `{}` is not available", other.name()));
            }
        };
        Ok(DecodedFrame {
            success: true,
            command: command.name().to_string(),
            raw: Vec::new(),
            data,
        })
    }
}

/// Worker state: the backend plus the device the UI is talking to.
pub struct Worker<B> {
    backend: B,
    active_mac: Option<String>,
}

impl<B: Backend> Worker<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            active_mac: None,
        }
    }

    /// Runs one request to completion. `None` means there is nothing to tell
    /// the UI.
    pub fn handle(&mut self, request: Request) -> Option<Event> {
        Some(match request {
            Request::Scan => Event::Devices(self.scan().map_err(describe)),
            Request::Connect(device) => match self.connect(&device) {
                Ok(state) => {
                    self.active_mac = Some(device.mac.clone());
                    Event::Connected(device, state)
                }
                Err(error) => {
                    self.active_mac = None;
                    Event::ConnectFailed(describe(error))
                }
            },
            Request::Refresh => match self.read_info() {
                Ok(state) => Event::State(state),
                Err(error) => Event::Failed(format!("Could not read the headset: {error:#}")),
            },
            Request::Set(setting) => match self.apply(&setting) {
                Ok(state) => Event::State(state),
                Err(error) => Event::Failed(format!(
                    "Could not change {}: {error:#}",
                    setting.describe()
                )),
            },
            Request::Disconnect => {
                self.active_mac = None;
                return None;
            }
        })
    }

    fn scan(&mut self) -> Result<Vec<DeviceEntry>> {
        let mut devices: Vec<DeviceEntry> = self
            .backend
            .list_devices()?
            .into_iter()
            .map(|device| DeviceEntry {
                supported: supported_protocol(&device.name).is_some(),
                mac: device.mac,
                name: device.name,
                connected: device.connected,
            })
            .collect();
        // Supported headsets first, then by name.
        devices.sort_by(|a, b| (!a.supported, &a.name).cmp(&(!b.supported, &b.name)));
        Ok(devices)
    }

    fn connect(&mut self, device: &DeviceEntry) -> Result<HeadsetState> {
        if !device.supported {
            return Err(anyhow!("`{}` is not a supported headset", device.name));
        }
        let mut state = HeadsetState::default();
        let firmware = self
            .backend
            .send(&device.mac, &ProtocolCommand::QueryFirmware)?;
        state.apply(&firmware.data);
        let info = self
            .backend
            .send(&device.mac, &ProtocolCommand::QueryInfo)?;
        state.apply(&info.data);
        Ok(state)
    }

    fn active_mac(&self) -> Result<String> {
        self.active_mac
            .clone()
            .ok_or_else(|| anyhow!("no headset is connected"))
    }

    /// Reads the settings and battery. Firmware is not part of this response,
    /// so the UI keeps the version it got when connecting.
    fn read_info(&mut self) -> Result<HeadsetState> {
        let mac = self.active_mac()?;
        let info = self.backend.send(&mac, &ProtocolCommand::QueryInfo)?;
        let mut state = HeadsetState::default();
        state.apply(&info.data);
        Ok(state)
    }

    /// Sends a change, then re-reads the headset so the UI shows what the
    /// headset actually stored rather than what was asked for.
    fn apply(&mut self, setting: &Setting) -> Result<HeadsetState> {
        let mac = self.active_mac()?;
        let response = self.backend.send(&mac, &setting.command())?;
        if !response.success {
            return Err(anyhow!("the headset rejected the command"));
        }
        self.read_info()
    }
}

fn describe(error: anyhow::Error) -> String {
    format!("{error:#}")
}

/// Starts the worker thread. `make_backend` runs on that thread, because the
/// platform transport is not `Send`. `wake` is called after every event.
pub fn spawn<B, F, W>(make_backend: F, wake: W) -> (Sender<Request>, Receiver<Event>)
where
    B: Backend,
    F: FnOnce() -> B + Send + 'static,
    W: Fn() + Send + 'static,
{
    let (request_tx, request_rx) = channel::<Request>();
    let (event_tx, event_rx) = channel::<Event>();
    thread::Builder::new()
        .name("bluetooth-worker".into())
        .spawn(move || {
            let mut worker = Worker::new(make_backend());
            // Ends when the UI drops its sender (window closed).
            for request in request_rx {
                if let Some(event) = worker.handle(request) {
                    if event_tx.send(event).is_err() {
                        break;
                    }
                    wake();
                }
            }
        })
        .expect("failed to start the Bluetooth worker thread");
    (request_tx, event_rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn demo_worker() -> Worker<DemoBackend> {
        Worker::new(DemoBackend::new(Duration::ZERO))
    }

    fn connect(worker: &mut Worker<DemoBackend>) -> HeadsetState {
        let Some(Event::Devices(Ok(devices))) = worker.handle(Request::Scan) else {
            panic!("scan failed");
        };
        match worker.handle(Request::Connect(devices[0].clone())) {
            Some(Event::Connected(_, state)) => state,
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn scan_lists_supported_headsets_first() {
        let Some(Event::Devices(Ok(devices))) = demo_worker().handle(Request::Scan) else {
            panic!("scan failed");
        };
        assert_eq!(devices.len(), 2);
        assert!(devices[0].supported && devices[0].name.contains("Max5c"));
        assert!(!devices[1].supported);
    }

    #[test]
    fn connect_reads_firmware_and_settings() {
        let state = connect(&mut demo_worker());
        assert_eq!(state.firmware.as_deref(), Some("0.0.0-demo"));
        assert_eq!(state.battery, Some(90));
        assert_eq!(state.anc, Some(AncChoice::Off));
        assert_eq!(state.eq.as_deref(), Some("treble"));
    }

    #[test]
    fn unsupported_devices_are_refused_without_touching_bluetooth() {
        let mut worker = demo_worker();
        let Some(Event::Devices(Ok(devices))) = worker.handle(Request::Scan) else {
            panic!("scan failed");
        };
        let event = worker.handle(Request::Connect(devices[1].clone()));
        assert!(matches!(event, Some(Event::ConnectFailed(_))), "{event:?}");
    }

    #[test]
    fn a_change_is_followed_by_a_fresh_read() {
        let mut worker = demo_worker();
        connect(&mut worker);
        for choice in AncChoice::ALL {
            let event = worker.handle(Request::Set(Setting::Anc(choice)));
            let Some(Event::State(state)) = event else {
                panic!("unexpected {event:?}");
            };
            assert_eq!(state.anc, Some(choice));
        }
        let Some(Event::State(state)) = worker.handle(Request::Set(Setting::PromptVolume(4)))
        else {
            panic!("volume change failed");
        };
        assert_eq!(state.prompt_volume, Some(4));
        assert_eq!(state.anc, Some(AncChoice::Adaptive));
    }

    #[test]
    fn requests_without_a_connection_fail_cleanly() {
        let mut worker = demo_worker();
        assert!(matches!(
            worker.handle(Request::Refresh),
            Some(Event::Failed(_))
        ));
        connect(&mut worker);
        assert_eq!(worker.handle(Request::Disconnect), None);
        assert!(matches!(
            worker.handle(Request::Set(Setting::Eq("bass"))),
            Some(Event::Failed(_))
        ));
    }

    #[test]
    fn spawned_worker_answers_and_wakes_the_ui() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        let (requests, events) = spawn(
            || DemoBackend::new(Duration::ZERO),
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        );
        requests.send(Request::Scan).unwrap();
        let event = events.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(event, Event::Devices(Ok(_))));
        drop(requests);
        // The wake callback runs right after the send; give it a moment.
        for _ in 0..100 {
            if wakes.load(Ordering::SeqCst) == 1 {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("the worker never woke the UI");
    }
}
