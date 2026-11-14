//! The egui front end. It never touches Bluetooth: it sends [`Request`]s to
//! the worker thread and draws whatever [`Event`]s came back.

use std::sync::mpsc::{Receiver, Sender};

use eframe::egui::{self, Color32, RichText};

use crate::model::{
    AncChoice, EQ_PRESETS, HeadsetState, PROMPT_LANGUAGES, PROMPT_VOLUME_RANGE, Setting,
};
use crate::worker::{DeviceEntry, Event, Request};

pub struct App {
    requests: Sender<Request>,
    events: Receiver<Event>,
    demo: bool,
    /// What the worker is doing right now; controls are disabled meanwhile.
    busy: Option<String>,
    devices: Vec<DeviceEntry>,
    scanned_once: bool,
    selected_mac: Option<String>,
    connection: Option<Connection>,
    error: Option<String>,
}

struct Connection {
    device: DeviceEntry,
    state: HeadsetState,
    /// Slider position while the user drags it; sent on release.
    volume_draft: u8,
    /// Strength used when "Noise cancelling" is switched on.
    last_intensity: AncChoice,
}

impl Connection {
    fn new(device: DeviceEntry, state: HeadsetState) -> Self {
        let volume_draft = state.prompt_volume.unwrap_or(*PROMPT_VOLUME_RANGE.start());
        Self {
            last_intensity: state.anc_intensity.unwrap_or(AncChoice::Adaptive),
            device,
            state,
            volume_draft,
        }
    }

    fn update(&mut self, mut state: HeadsetState) {
        // The settings response carries no firmware version; keep ours.
        if state.firmware.is_none() {
            state.firmware = self.state.firmware.take();
        }
        if let Some(volume) = state.prompt_volume {
            self.volume_draft = volume;
        }
        if let Some(intensity) = state.anc_intensity {
            self.last_intensity = intensity;
        }
        self.state = state;
    }
}

impl App {
    pub fn new(requests: Sender<Request>, events: Receiver<Event>, demo: bool) -> Self {
        let mut app = Self {
            requests,
            events,
            demo,
            busy: None,
            devices: Vec::new(),
            scanned_once: false,
            selected_mac: None,
            connection: None,
            error: None,
        };
        app.request(Request::Scan, "Looking for paired headsets…");
        app
    }

    fn request(&mut self, request: Request, activity: &str) {
        self.error = None;
        if self.requests.send(request).is_ok() {
            self.busy = Some(activity.to_string());
        } else {
            self.error = Some("The Bluetooth worker stopped; restart the app.".into());
        }
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.busy = None;
            match event {
                Event::Devices(Ok(devices)) => {
                    self.scanned_once = true;
                    let still_listed = |mac: &String| devices.iter().any(|d| &d.mac == mac);
                    if !self.selected_mac.as_ref().is_some_and(still_listed) {
                        self.selected_mac = devices
                            .iter()
                            .find(|device| device.supported)
                            .map(|device| device.mac.clone());
                    }
                    self.devices = devices;
                }
                Event::Devices(Err(error)) => {
                    self.scanned_once = true;
                    self.devices.clear();
                    self.selected_mac = None;
                    self.error = Some(error);
                }
                Event::Connected(device, state) => {
                    self.connection = Some(Connection::new(device, state));
                }
                Event::ConnectFailed(error) => {
                    self.connection = None;
                    self.error = Some(error);
                }
                Event::State(state) => {
                    if let Some(connection) = &mut self.connection {
                        connection.update(state);
                    }
                }
                Event::Failed(error) => {
                    if let Some(connection) = &mut self.connection {
                        // Put the slider back on the last value read.
                        if let Some(volume) = connection.state.prompt_volume {
                            connection.volume_draft = volume;
                        }
                    }
                    self.error = Some(error);
                }
            }
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        // Wrapped, so a long error message is never cut off.
        ui.horizontal_wrapped(|ui| {
            if let Some(activity) = &self.busy {
                ui.spinner();
                ui.label(activity);
            } else if let Some(error) = &self.error {
                ui.label(RichText::new(error).color(Color32::from_rgb(220, 80, 80)));
            } else if self.connection.is_some() {
                ui.label("Showing the values last read from the headset.");
            } else {
                ui.label("Pick a paired headset and connect.");
            }
        });
    }

    fn connection_screen(&mut self, ui: &mut egui::Ui) {
        let idle = self.busy.is_none();
        ui.heading("Paired devices");
        ui.label("Devices already paired in the system Bluetooth settings.");
        ui.add_space(8.0);

        if self.devices.is_empty() && self.scanned_once && idle && self.error.is_none() {
            ui.label("No paired Bluetooth devices were found.");
        }
        let mut clicked = None;
        for device in &self.devices {
            let selected = self.selected_mac.as_deref() == Some(device.mac.as_str());
            let link = match device.connected {
                Some(true) => " · connected",
                Some(false) => " · not connected",
                None => "",
            };
            let text = format!("{}\n{}{}", device.name, device.mac, link);
            let response = ui
                .add_enabled(
                    idle && device.supported,
                    egui::Button::selectable(selected, text)
                        .min_size(egui::vec2(ui.available_width(), 0.0)),
                )
                .on_disabled_hover_text(if device.supported {
                    "Busy…"
                } else {
                    "Not a supported headset (only the HiTune Max5c is)."
                });
            if response.clicked() {
                clicked = Some(device.mac.clone());
            }
        }
        if clicked.is_some() {
            self.selected_mac = clicked;
        }

        ui.add_space(12.0);
        let selected = self
            .devices
            .iter()
            .find(|device| Some(&device.mac) == self.selected_mac.as_ref() && device.supported)
            .cloned();
        ui.horizontal(|ui| {
            let connect = ui.add_enabled(
                idle && selected.is_some(),
                egui::Button::new(RichText::new("Connect").strong()),
            );
            if connect.clicked()
                && let Some(device) = selected
            {
                let activity = format!("Connecting to {}…", device.name);
                self.request(Request::Connect(device), &activity);
            }
            if ui
                .add_enabled(idle, egui::Button::new("Refresh list"))
                .clicked()
            {
                self.request(Request::Scan, "Looking for paired headsets…");
            }
        });
    }

    fn headset_screen(&mut self, ui: &mut egui::Ui) {
        let idle = self.busy.is_none();
        let mut pending: Option<(Request, String)> = None;
        let mut leave = false;
        let Some(connection) = &mut self.connection else {
            return;
        };
        let state = &connection.state;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(&connection.device.name);
                ui.label(RichText::new(&connection.device.mac).weak());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Devices").clicked() {
                    leave = true;
                }
                if ui.add_enabled(idle, egui::Button::new("Refresh")).clicked() {
                    pending = Some((Request::Refresh, "Reading the headset…".into()));
                }
            });
        });
        ui.separator();

        section(ui, "Status", |ui| {
            egui::Grid::new("status").num_columns(2).show(ui, |ui| {
                ui.label("Firmware");
                ui.label(state.firmware.as_deref().unwrap_or("not reported"));
                ui.end_row();
                ui.label("Battery");
                match state.battery {
                    Some(level) => {
                        ui.add(
                            egui::ProgressBar::new(f32::from(level) / 100.0)
                                .text(format!("{level}%"))
                                .desired_width(180.0),
                        );
                    }
                    None => {
                        ui.label("not reported");
                    }
                }
                ui.end_row();
            });
        });

        ui.add_enabled_ui(idle, |ui| {
            section(ui, "Noise control", |ui| {
                let noise_cancelling = state.anc.is_some_and(AncChoice::is_noise_cancelling);
                egui::Grid::new("noise").num_columns(2).show(ui, |ui| {
                    ui.label("Mode");
                    ui.horizontal(|ui| {
                        let modes = [
                            ("Off", state.anc == Some(AncChoice::Off), AncChoice::Off),
                            (
                                "Transparency",
                                state.anc == Some(AncChoice::Transparency),
                                AncChoice::Transparency,
                            ),
                            // Switching it on goes back to the last strength.
                            (
                                "Noise cancelling",
                                noise_cancelling,
                                connection.last_intensity,
                            ),
                        ];
                        for (label, selected, choice) in modes {
                            if ui.selectable_label(selected, label).clicked() && !selected {
                                pending = Some(set(Setting::Anc(choice)));
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Intensity");
                    ui.add_enabled_ui(noise_cancelling, |ui| {
                        ui.horizontal(|ui| {
                            for choice in AncChoice::INTENSITIES {
                                let selected = state.anc == Some(choice);
                                let button = ui
                                    .selectable_label(selected, choice.label())
                                    .on_disabled_hover_text("Only with noise cancelling on.");
                                if button.clicked() && !selected {
                                    pending = Some(set(Setting::Anc(choice)));
                                }
                            }
                        });
                    });
                    ui.end_row();
                });
                if state.anc.is_none() {
                    ui.label(RichText::new("The headset reported an unknown mode.").weak());
                }
            });

            section(ui, "Equalizer", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for preset in EQ_PRESETS {
                        let selected = state.eq.as_deref() == Some(preset);
                        if ui.selectable_label(selected, capitalize(preset)).clicked() && !selected
                        {
                            pending = Some(set(Setting::Eq(preset)));
                        }
                    }
                });
            });

            section(ui, "Voice prompts", |ui| {
                egui::Grid::new("prompts").num_columns(2).show(ui, |ui| {
                    ui.label("Language");
                    ui.horizontal(|ui| {
                        for language in PROMPT_LANGUAGES {
                            let selected = state.prompt_language.as_deref() == Some(language);
                            if ui.selectable_label(selected, language).clicked() && !selected {
                                pending = Some(set(Setting::PromptLanguage(language)));
                            }
                        }
                    });
                    ui.end_row();
                    ui.label("Volume");
                    let slider = ui.add(egui::Slider::new(
                        &mut connection.volume_draft,
                        PROMPT_VOLUME_RANGE,
                    ));
                    // Send once, when the drag ends (or on a click / key press),
                    // not on every step of the drag.
                    let committed =
                        slider.drag_stopped() || (slider.changed() && !slider.dragged());
                    if committed && state.prompt_volume != Some(connection.volume_draft) {
                        pending = Some(set(Setting::PromptVolume(connection.volume_draft)));
                    }
                    ui.end_row();
                });
            });
        });

        if leave {
            self.connection = None;
            self.error = None;
            // The worker answers nothing to this, so the UI is not "busy".
            let _ = self.requests.send(Request::Disconnect);
            if idle {
                self.request(Request::Scan, "Looking for paired headsets…");
            }
        } else if let Some((request, activity)) = pending {
            self.request(request, &activity);
        }
    }
}

fn set(setting: Setting) -> (Request, String) {
    let activity = format!("Setting {}…", setting.describe());
    (Request::Set(setting), activity)
}

fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).strong());
    ui.add_space(2.0);
    body(ui);
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            self.status_bar(ui);
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.demo {
                ui.label(
                    RichText::new("Demo mode: pretend headset, no Bluetooth.")
                        .color(Color32::from_rgb(200, 150, 40)),
                );
                ui.add_space(6.0);
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.connection.is_some() {
                    self.headset_screen(ui);
                } else {
                    self.connection_screen(ui);
                }
            });
        });
    }
}
