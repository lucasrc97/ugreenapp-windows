//! The egui front end. It never touches Bluetooth: it sends [`Request`]s to
//! the worker thread and draws whatever [`Event`]s came back.
//!
//! The screens follow the layout of the UGREEN phone app: a list of devices,
//! then for a connected headset a "Device" tab and a "More" tab, each with
//! sub-pages for the individual settings.

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};

use crate::model::{
    AncChoice, EQ_PRESETS, Feature, HeadsetState, PROMPT_LANGUAGES, PROMPT_VOLUME_RANGE, Setting,
    eq_label, has_wear_detection,
};
use crate::settings::{self, ThemeChoice};
use crate::widgets::{self, NoiseIcon, Palette, TabIcon};
use crate::worker::{DeviceEntry, Event, Request};

#[cfg(debug_assertions)]
#[path = "tour.rs"]
pub mod tour;

/// Pause after each headset command before the next one may be sent.
/// Changing settings in quick succession has frozen a HiTune Max5c (it
/// would not even power off), so the controls stay disabled for a moment.
const COMMAND_COOLDOWN: Duration = Duration::from_secs(1);

pub struct App {
    requests: Sender<Request>,
    events: Receiver<Event>,
    demo: bool,
    /// What the worker is doing right now; controls are disabled meanwhile.
    busy: Option<String>,
    /// When the last headset command finished.
    last_command_done: Option<Instant>,
    devices: Vec<DeviceEntry>,
    scanned_once: bool,
    connection: Option<Connection>,
    error: Option<String>,
    /// Neutral message for the status bar (for example after a reset).
    notice: Option<String>,
    theme: ThemeChoice,
    /// Screenshot tour of every screen (debug builds, `UGREEN_GUI_TOUR`).
    #[cfg(debug_assertions)]
    pub tour: Option<tour::Tour>,
}

/// The two tabs of a connected headset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Device,
    More,
}

/// What a tab is showing: its main list or one of its sub-pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Main,
    Equalizer,
    Feature(Feature),
    BroadcastVoice,
    Theme,
}

/// A dialog over the current page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sheet {
    NoiseMode,
    FactoryReset,
}

/// What the user asked for on the headset screens.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Send(Setting),
    Refresh,
    /// Back to the device list.
    Leave,
    FactoryReset,
    SetTheme(ThemeChoice),
}

struct Connection {
    device: DeviceEntry,
    state: HeadsetState,
    /// Slider position while the user drags it; sent on release.
    volume_draft: u8,
    /// Mode used when noise cancelling is switched on.
    last_intensity: AncChoice,
    tab: Tab,
    page: Page,
    sheet: Option<Sheet>,
}

impl Connection {
    fn new(device: DeviceEntry, state: HeadsetState) -> Self {
        let volume_draft = state.prompt_volume.unwrap_or(*PROMPT_VOLUME_RANGE.start());
        Self {
            last_intensity: state.anc_intensity.unwrap_or(AncChoice::Adaptive),
            device,
            state,
            volume_draft,
            tab: Tab::Device,
            page: Page::Main,
            sheet: None,
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

    fn noise_cancelling(&self) -> bool {
        self.state.anc.is_some_and(AncChoice::is_noise_cancelling)
    }

    /// Features that get a row on the Device tab, in the phone app's order.
    fn listed_features(&self) -> Vec<Feature> {
        let mut features = vec![Feature::SpatialAudio, Feature::Multipoint, Feature::Ldac];
        // The phone app has no such row for models without the sensor.
        if has_wear_detection(&self.device.name) {
            features.push(Feature::WearDetection);
        }
        features
    }
}

impl App {
    pub fn new(
        requests: Sender<Request>,
        events: Receiver<Event>,
        demo: bool,
        theme: ThemeChoice,
    ) -> Self {
        let mut app = Self {
            requests,
            events,
            demo,
            busy: None,
            last_command_done: None,
            devices: Vec::new(),
            scanned_once: false,
            connection: None,
            error: None,
            notice: None,
            theme,
            #[cfg(debug_assertions)]
            tour: None,
        };
        app.request(Request::Scan, "Looking for paired headsets…");
        app
    }

    fn request(&mut self, request: Request, activity: &str) {
        self.error = None;
        self.notice = None;
        if self.requests.send(request).is_ok() {
            self.busy = Some(activity.to_string());
        } else {
            self.error = Some("The Bluetooth worker stopped; restart the app.".into());
        }
    }

    /// Time left before the headset may be sent another command.
    fn cooldown_left(&self) -> Option<Duration> {
        cooldown_remaining(self.last_command_done?.elapsed())
    }

    /// Whether a new command may be sent right now.
    fn idle(&self) -> bool {
        self.busy.is_none() && self.cooldown_left().is_none()
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.busy = None;
            // Listing paired devices does not talk to the headset.
            if !matches!(event, Event::Devices(_)) {
                self.last_command_done = Some(Instant::now());
            }
            match event {
                Event::Devices(Ok(devices)) => {
                    self.scanned_once = true;
                    self.devices = devices;
                }
                Event::Devices(Err(error)) => {
                    self.scanned_once = true;
                    self.devices.clear();
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
                Event::ConnectionDropped(change) => {
                    self.connection = None;
                    self.notice = Some(format!(
                        "Changed {change}. The headset dropped its Bluetooth connection. \
                         Switch it off and on, reconnect it in the system Bluetooth \
                         settings, then connect here again."
                    ));
                }
                Event::ResetDone => {
                    self.connection = None;
                    self.notice = Some(
                        "Factory reset sent. The headset is back to its defaults and \
                         has to be paired again."
                            .into(),
                    );
                }
            }
        }
    }

    /// The line above the tabs: only there while something is worth saying.
    fn status_bar(&self, ui: &mut egui::Ui) -> bool {
        let palette = Palette::for_ui(ui);
        let (spinner, text, color) = if let Some(activity) = &self.busy {
            (true, activity.as_str(), palette.weak)
        } else if let Some(error) = &self.error {
            (false, error.as_str(), palette.danger)
        } else if let Some(notice) = &self.notice {
            (false, notice.as_str(), palette.text)
        } else if self.cooldown_left().is_some() {
            (true, "Giving the headset a moment…", palette.weak)
        } else {
            return false;
        };
        // Wrapped, so a long error message is never cut off.
        ui.horizontal_wrapped(|ui| {
            if spinner {
                ui.spinner();
            }
            ui.label(RichText::new(text).color(color).size(13.5));
        });
        true
    }

    fn has_status(&self) -> bool {
        self.busy.is_some()
            || self.error.is_some()
            || self.notice.is_some()
            || self.cooldown_left().is_some()
    }

    fn devices_screen(&mut self, ui: &mut egui::Ui) {
        let idle = self.idle();
        let palette = Palette::for_ui(ui);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("My Devices({})", self.devices.len())).size(26.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let refresh =
                    egui::Button::new(RichText::new("Refresh").color(palette.accent)).frame(false);
                if ui.add_enabled(idle, refresh).clicked() {
                    self.request(Request::Scan, "Looking for paired headsets…");
                }
            });
        });
        ui.add_space(12.0);

        if self.devices.is_empty() && self.scanned_once && idle && self.error.is_none() {
            ui.label(RichText::new("No paired Bluetooth devices were found.").color(palette.weak));
        }
        let mut chosen = None;
        for device in &self.devices {
            ui.add_enabled_ui(idle && device.supported, |ui| {
                let response =
                    device_card(ui, device).on_disabled_hover_text(if device.supported {
                        "Busy…"
                    } else {
                        "Not a supported headset (only the HiTune Max5c is)."
                    });
                if response.clicked() {
                    chosen = Some(device.clone());
                }
            });
            ui.add_space(8.0);
        }
        if let Some(device) = chosen {
            let activity = format!("Connecting to {}…", device.name);
            self.request(Request::Connect(device), &activity);
        }

        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Headsets already paired in the system Bluetooth settings are listed here. \
                 Click one to connect.",
            )
            .color(palette.weak)
            .size(13.0),
        );
    }

    fn handle(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Send(setting) => {
                let activity = format!("Setting {}…", setting.describe());
                self.request(Request::Set(setting), &activity);
            }
            Action::Refresh => self.request(Request::Refresh, "Reading the headset…"),
            Action::FactoryReset => self.request(Request::FactoryReset, "Resetting the headset…"),
            Action::Leave => {
                let idle = self.idle();
                self.connection = None;
                self.error = None;
                // The worker answers nothing to this, so the UI is not "busy".
                let _ = self.requests.send(Request::Disconnect);
                if idle {
                    self.request(Request::Scan, "Looking for paired headsets…");
                }
            }
            Action::SetTheme(theme) => {
                self.theme = theme;
                ctx.set_theme(theme.preference());
                settings::save_theme(theme);
            }
        }
    }
}

/// One paired device on the first screen.
fn device_card(ui: &mut egui::Ui, device: &DeviceEntry) -> egui::Response {
    widgets::card(ui, |ui| {
        let palette = Palette::for_ui(ui);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 68.0), egui::Sense::click());
        let painter = ui.painter();
        painter.text(
            egui::pos2(rect.left(), rect.center().y - 12.0),
            egui::Align2::LEFT_CENTER,
            &device.name,
            egui::FontId::proportional(17.0),
            palette.text,
        );
        let (dot, status) = match (device.supported, device.connected) {
            (false, _) => (palette.weak, "Not supported"),
            (true, Some(true)) => (palette.accent, "Connected"),
            (true, _) => (palette.weak, "Not Connected"),
        };
        let line_y = rect.center().y + 14.0;
        painter.circle_filled(egui::pos2(rect.left() + 6.0, line_y), 5.0, dot);
        painter.text(
            egui::pos2(rect.left() + 20.0, line_y),
            egui::Align2::LEFT_CENTER,
            status,
            egui::FontId::proportional(14.0),
            palette.weak,
        );
        if ui.is_enabled() {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    })
}

fn on_off(value: Option<bool>) -> Option<&'static str> {
    value.map(|on| if on { "On" } else { "Off" })
}

/// Draws the current tab or sub-page of a connected headset.
fn headset_screen(
    ui: &mut egui::Ui,
    connection: &mut Connection,
    idle: bool,
    theme: ThemeChoice,
) -> Option<Action> {
    match (connection.tab, connection.page) {
        (_, Page::Equalizer) => equalizer_page(ui, connection, idle),
        (_, Page::Feature(feature)) => feature_page(ui, connection, feature, idle),
        (_, Page::BroadcastVoice) => broadcast_voice_page(ui, connection, idle),
        (_, Page::Theme) => theme_page(ui, connection, theme),
        (Tab::Device, Page::Main) => device_tab(ui, connection, idle),
        (Tab::More, Page::Main) => more_tab(ui, connection, theme),
    }
}

fn device_tab(ui: &mut egui::Ui, connection: &mut Connection, idle: bool) -> Option<Action> {
    let mut action = None;
    let (back, refresh) = widgets::header(ui, &connection.device.name, Some("Refresh"));
    if back {
        action = Some(Action::Leave);
    } else if refresh && idle {
        action = Some(Action::Refresh);
    }

    widgets::headphones(ui);
    widgets::card(ui, |ui| widgets::battery(ui, connection.state.battery));
    ui.add_space(10.0);

    let noise_cancelling = connection.noise_cancelling();
    widgets::card(ui, |ui| {
        widgets::card_title(ui, "Noise cancelling control");
        ui.add_enabled_ui(idle, |ui| {
            let anc = connection.state.anc;
            let choices = [
                // Switching it on goes back to the last mode used.
                (
                    "On",
                    NoiseIcon::On,
                    noise_cancelling,
                    connection.last_intensity,
                ),
                (
                    "Off",
                    NoiseIcon::Off,
                    anc == Some(AncChoice::Off),
                    AncChoice::Off,
                ),
                (
                    "Ambient sound",
                    NoiseIcon::Ambient,
                    anc == Some(AncChoice::Transparency),
                    AncChoice::Transparency,
                ),
            ];
            ui.columns(3, |columns| {
                for (column, (label, icon, selected, choice)) in columns.iter_mut().zip(choices) {
                    if widgets::round_choice(column, label, icon, selected).clicked() && !selected {
                        action = Some(Action::Send(Setting::Anc(choice)));
                    }
                }
            });
        });
        widgets::divider(ui);
        // The mode only matters, and can only be picked, while it is on.
        ui.add_enabled_ui(noise_cancelling, |ui| {
            let mode = connection.last_intensity.label();
            let row = widgets::nav_row(ui, "Noise cancelling mode", Some(mode), true)
                .on_disabled_hover_text("Only while noise cancelling is on.");
            if row.clicked() {
                connection.sheet = Some(Sheet::NoiseMode);
            }
        });
        if connection.state.anc.is_none() {
            let palette = Palette::for_ui(ui);
            ui.label(
                RichText::new("The headset reported an unknown mode.")
                    .color(palette.weak)
                    .size(13.0),
            );
        }
    });
    ui.add_space(10.0);

    ui.columns(2, |columns| {
        let preset = connection.state.eq.as_deref().map_or("", eq_label);
        if widgets::tile(&mut columns[0], "Equalizer", preset).clicked() {
            connection.page = Page::Equalizer;
        }
        let game_mode = on_off(connection.state.features.get(Feature::GameMode)).unwrap_or("");
        if widgets::tile(&mut columns[1], Feature::GameMode.label(), game_mode).clicked() {
            connection.page = Page::Feature(Feature::GameMode);
        }
    });
    ui.add_space(10.0);

    widgets::card(ui, |ui| {
        let features = connection.listed_features();
        for (index, feature) in features.iter().enumerate() {
            if index > 0 {
                widgets::divider(ui);
            }
            let value = on_off(connection.state.features.get(*feature));
            if widgets::nav_row(ui, feature.label(), value, true).clicked() {
                connection.page = Page::Feature(*feature);
            }
        }
    });
    ui.add_space(10.0);
    action
}

/// Back arrow of a sub-page: returns to the tab's main list.
fn sub_page_header(ui: &mut egui::Ui, connection: &mut Connection, title: &str) {
    let (back, _) = widgets::header(ui, title, None);
    if back {
        connection.page = Page::Main;
    }
    ui.add_space(6.0);
}

fn equalizer_page(ui: &mut egui::Ui, connection: &mut Connection, idle: bool) -> Option<Action> {
    let mut action = None;
    sub_page_header(ui, connection, "Equalizer");
    ui.add_enabled_ui(idle, |ui| {
        for pair in EQ_PRESETS.chunks(2) {
            ui.columns(2, |columns| {
                for (column, preset) in columns.iter_mut().zip(pair) {
                    let selected = connection.state.eq.as_deref() == Some(*preset);
                    let clicked = widgets::card(column, |ui| {
                        widgets::option_row(ui, eq_label(preset), None, selected).clicked()
                    });
                    if clicked && !selected {
                        action = Some(Action::Send(Setting::Eq(preset)));
                    }
                }
            });
            ui.add_space(4.0);
        }
    });
    action
}

/// What the page of a feature says under its switch.
fn feature_notes(feature: Feature) -> &'static [&'static str] {
    match feature {
        Feature::GameMode => &[
            "Lowers the audio delay, for games and video.",
            "Where there is strong wireless interference (stations, airports) the sound \
             may cut out or the headset may disconnect. Best used at home.",
            "It switches itself off when the headset disconnects.",
        ],
        Feature::SpatialAudio => {
            &["Makes movies, games and music sound more spacious, like surround sound."]
        }
        Feature::Multipoint => &[
            "Lets the headset stay connected to two Bluetooth devices at the same time.",
            "There may be a short delay when the sound moves from one device to the other.",
        ],
        Feature::Ldac => &[
            "Higher quality wireless audio (the LDAC codec), at the cost of battery life \
             and a less stable connection.",
            "While it is on, Game mode and Dual link are not available.",
        ],
        Feature::WearDetection => &["Pauses playback when the headset is taken off."],
    }
}

fn feature_page(
    ui: &mut egui::Ui,
    connection: &mut Connection,
    feature: Feature,
    idle: bool,
) -> Option<Action> {
    let mut action = None;
    sub_page_header(ui, connection, feature.label());

    let features = connection.state.features;
    let current = features.get(feature);
    let locked = feature.locked_reason(&connection.device.name);
    // While Hi-Res audio decoding is on the headset refuses these two.
    let blocked = feature.excluded_by_ldac() && features.get(Feature::Ldac) == Some(true);
    let usable = idle && current.is_some() && locked.is_none() && !blocked;

    widgets::card(ui, |ui| {
        ui.add_enabled_ui(usable, |ui| {
            let on = current.unwrap_or(false);
            if widgets::switch_row(ui, feature.label(), on).clicked() {
                action = Some(Action::Send(Setting::Toggle(feature, !on)));
            }
        });
    });

    let mut lines: Vec<&str> = Vec::new();
    if let Some(reason) = locked {
        lines.push(reason);
    } else if current.is_none() {
        lines.push("The headset did not report this setting.");
    } else if blocked {
        lines.push("Not available while Hi-Res audio decoding is on.");
    }
    lines.extend(feature_notes(feature));
    widgets::notes(ui, &lines);
    action
}

fn more_tab(ui: &mut egui::Ui, connection: &mut Connection, theme: ThemeChoice) -> Option<Action> {
    let mut action = None;
    let (back, _) = widgets::header(ui, &connection.device.name, None);
    if back {
        action = Some(Action::Leave);
    }
    ui.add_space(6.0);

    widgets::card(ui, |ui| {
        widgets::nav_row(ui, "Device name", Some(&connection.device.name), false);
        widgets::divider(ui);
        let firmware = connection
            .state
            .firmware
            .as_deref()
            .unwrap_or("not reported");
        widgets::nav_row(ui, "Firmware version", Some(firmware), false);
        widgets::divider(ui);
        let voice = connection.state.prompt_language.as_deref();
        if widgets::nav_row(ui, "Broadcast voice", voice, true).clicked() {
            connection.page = Page::BroadcastVoice;
        }
        widgets::divider(ui);
        if widgets::nav_row(ui, "Theme", Some(theme.label()), true).clicked() {
            connection.page = Page::Theme;
        }
        widgets::divider(ui);
        if widgets::nav_row(ui, "Factory reset", None, true).clicked() {
            connection.sheet = Some(Sheet::FactoryReset);
        }
    });

    ui.add_space(24.0);
    let palette = Palette::for_ui(ui);
    let width = ui.available_width();
    if widgets::pill_button(ui, "Disconnect", palette.danger, palette.card, width).clicked() {
        action = Some(Action::Leave);
    }
    action
}

fn broadcast_voice_page(
    ui: &mut egui::Ui,
    connection: &mut Connection,
    idle: bool,
) -> Option<Action> {
    let mut action = None;
    sub_page_header(ui, connection, "Broadcast voice");
    ui.add_enabled_ui(idle, |ui| {
        widgets::card(ui, |ui| {
            for (index, language) in PROMPT_LANGUAGES.into_iter().enumerate() {
                if index > 0 {
                    widgets::divider(ui);
                }
                let selected = connection.state.prompt_language.as_deref() == Some(language);
                if widgets::option_row(ui, language, None, selected).clicked() && !selected {
                    action = Some(Action::Send(Setting::PromptLanguage(language)));
                }
            }
        });
        ui.add_space(10.0);
        widgets::card(ui, |ui| {
            widgets::card_title(ui, "Prompt's volume");
            ui.add_space(6.0);
            // The rail takes the card's width, less the number next to it.
            ui.spacing_mut().slider_width = (ui.available_width() - 60.0).max(120.0);
            let slider = ui.add(egui::Slider::new(
                &mut connection.volume_draft,
                PROMPT_VOLUME_RANGE,
            ));
            // Send once, when the drag ends (or on a click / key press), not
            // on every step of the drag.
            let committed = slider.drag_stopped() || (slider.changed() && !slider.dragged());
            if committed && connection.state.prompt_volume != Some(connection.volume_draft) {
                action = Some(Action::Send(Setting::PromptVolume(connection.volume_draft)));
            }
            ui.add_space(8.0);
        });
    });
    widgets::notes(
        ui,
        &["The voice and its volume are for the headset's spoken announcements, not the music."],
    );
    action
}

fn theme_page(
    ui: &mut egui::Ui,
    connection: &mut Connection,
    theme: ThemeChoice,
) -> Option<Action> {
    let mut action = None;
    sub_page_header(ui, connection, "Theme");
    widgets::card(ui, |ui| {
        for (index, choice) in ThemeChoice::ALL.into_iter().enumerate() {
            if index > 0 {
                widgets::divider(ui);
            }
            let description =
                (choice == ThemeChoice::System).then_some("Follow the system setting");
            let selected = choice == theme;
            if widgets::option_row(ui, choice.label(), description, selected).clicked() && !selected
            {
                action = Some(Action::SetTheme(choice));
            }
        }
    });
    action
}

/// The dialog open over the page, if any. Esc or a click outside closes it.
fn sheet(ctx: &egui::Context, connection: &mut Connection, idle: bool) -> Option<Action> {
    let open = connection.sheet?;
    let mut action = None;
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("sheet")).show(ctx, |ui| {
        ui.set_width(310.0);
        let palette = Palette::for_ui(ui);
        match open {
            Sheet::NoiseMode => {
                widgets::card_title(ui, "Noise cancelling mode");
                ui.add_enabled_ui(idle, |ui| {
                    for (index, choice) in AncChoice::INTENSITIES.into_iter().enumerate() {
                        if index > 0 {
                            widgets::divider(ui);
                        }
                        let selected = connection.state.anc == Some(choice);
                        let row = widgets::option_row(ui, choice.label(), choice.hint(), selected);
                        if row.clicked() {
                            if !selected {
                                action = Some(Action::Send(Setting::Anc(choice)));
                            }
                            close = true;
                        }
                    }
                });
            }
            Sheet::FactoryReset => {
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Confirm Factory Reset").size(19.0));
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "All pairing records will be cleared after the headset is \
                             reset to factory settings. This cannot be undone.",
                        )
                        .color(palette.weak),
                    );
                });
                ui.add_space(14.0);
                let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                ui.horizontal(|ui| {
                    if widgets::pill_button(ui, "Cancel", palette.weak, palette.track, width)
                        .clicked()
                    {
                        close = true;
                    }
                    ui.add_enabled_ui(idle, |ui| {
                        let confirm = widgets::pill_button(
                            ui,
                            "Confirm",
                            egui::Color32::WHITE,
                            palette.danger,
                            width,
                        );
                        if confirm.clicked() {
                            action = Some(Action::FactoryReset);
                            close = true;
                        }
                    });
                });
                ui.add_space(4.0);
            }
        }
    });
    if close || modal.should_close() {
        connection.sheet = None;
    }
    action
}

/// Cooldown still to wait, given the time since the last command finished.
fn cooldown_remaining(elapsed: Duration) -> Option<Duration> {
    COMMAND_COOLDOWN
        .checked_sub(elapsed)
        .filter(|left| !left.is_zero())
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        #[cfg(debug_assertions)]
        if let Some(mut tour) = self.tour.take() {
            tour.drive(self, ctx);
            self.tour = Some(tour);
        }
        // Nothing else wakes the UI when the cooldown ends.
        if let Some(left) = self.cooldown_left() {
            ctx.request_repaint_after(left);
        }

        let palette = Palette::of(ctx.theme());
        let idle = self.idle();
        let mut action = None;

        if let Some(connection) = &mut self.connection {
            let bar = egui::Frame::new()
                .fill(palette.card)
                .inner_margin(egui::Margin::symmetric(16, 4));
            egui::TopBottomPanel::bottom("tabs")
                .frame(bar)
                .show_separator_line(false)
                .show(ctx, |ui| {
                    ui.columns(2, |columns| {
                        let tabs = [
                            ("Device", TabIcon::Device, Tab::Device),
                            ("More", TabIcon::More, Tab::More),
                        ];
                        for (column, (label, icon, tab)) in columns.iter_mut().zip(tabs) {
                            let selected = connection.tab == tab;
                            if widgets::tab_button(column, label, icon, selected).clicked() {
                                connection.tab = tab;
                                connection.page = Page::Main;
                            }
                        }
                    });
                });
        }
        if self.has_status() {
            let strip = egui::Frame::new()
                .fill(palette.background)
                .inner_margin(egui::Margin::symmetric(18, 6));
            egui::TopBottomPanel::bottom("status")
                .frame(strip)
                .show_separator_line(false)
                .show(ctx, |ui| {
                    self.status_bar(ui);
                });
        }

        let page = egui::Frame::new()
            .fill(palette.background)
            .inner_margin(egui::Margin::symmetric(16, 8));
        egui::CentralPanel::default().frame(page).show(ctx, |ui| {
            if self.demo {
                ui.label(
                    RichText::new("Demo mode: pretend headset, no Bluetooth.")
                        .color(egui::Color32::from_rgb(200, 150, 40))
                        .size(13.0),
                );
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Keeps the cards clear of the scroll bar.
                ui.set_width(ui.available_width());
                match &mut self.connection {
                    Some(connection) => {
                        action = headset_screen(ui, connection, idle, self.theme);
                    }
                    None => self.devices_screen(ui),
                }
            });
        });

        if let Some(connection) = &mut self.connection
            && let Some(from_sheet) = sheet(ctx, connection, idle)
        {
            action = Some(from_sheet);
        }
        if let Some(action) = action {
            self.handle(action, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn max5c() -> DeviceEntry {
        DeviceEntry {
            mac: "00:11:22:33:44:55".into(),
            name: "UGREEN HiTune Max5c".into(),
            connected: Some(true),
            supported: true,
        }
    }

    #[test]
    fn cooldown_lasts_one_second() {
        assert_eq!(COMMAND_COOLDOWN, Duration::from_secs(1));
        assert_eq!(
            cooldown_remaining(Duration::from_millis(300)),
            Some(Duration::from_millis(700))
        );
        assert_eq!(cooldown_remaining(Duration::from_secs(1)), None);
        assert_eq!(cooldown_remaining(Duration::from_secs(5)), None);
    }

    #[test]
    fn controls_stay_locked_right_after_a_headset_command() {
        let (requests, _request_rx) = channel();
        let (event_tx, events) = channel();
        let mut app = App::new(requests, events, true, ThemeChoice::System);
        assert!(!app.idle(), "the initial scan is in flight");

        // Listing devices does not touch the headset: no cooldown.
        event_tx.send(Event::Devices(Ok(Vec::new()))).unwrap();
        app.drain_events();
        assert!(app.idle());
        assert!(!app.has_status());

        // A finished headset command starts the cooldown.
        app.request(Request::Set(Setting::Anc(AncChoice::Off)), "test");
        event_tx
            .send(Event::State(HeadsetState::default()))
            .unwrap();
        app.drain_events();
        assert!(app.busy.is_none());
        assert!(!app.idle());
        assert!(app.has_status());

        // ...which ends once the cooldown time has passed.
        app.last_command_done = Instant::now().checked_sub(COMMAND_COOLDOWN);
        assert!(app.idle());
    }

    #[test]
    fn a_finished_reset_goes_back_to_the_device_list() {
        let (requests, request_rx) = channel();
        let (event_tx, events) = channel();
        let mut app = App::new(requests, events, true, ThemeChoice::System);
        event_tx
            .send(Event::Connected(max5c(), HeadsetState::default()))
            .unwrap();
        app.drain_events();

        // Opening the dialog sends nothing by itself.
        let connection = app.connection.as_mut().unwrap();
        connection.tab = Tab::More;
        connection.sheet = Some(Sheet::FactoryReset);
        let sent: Vec<Request> = request_rx.try_iter().collect();
        assert_eq!(sent, [Request::Scan]);

        event_tx.send(Event::ResetDone).unwrap();
        app.drain_events();
        assert!(app.connection.is_none());
        assert!(app.notice.is_some());
        assert!(app.error.is_none());
    }

    #[test]
    fn a_max5c_lists_the_phone_app_features_without_wear_detection() {
        let connection = Connection::new(max5c(), HeadsetState::default());
        assert_eq!(
            connection.listed_features(),
            [Feature::SpatialAudio, Feature::Multipoint, Feature::Ldac]
        );
        let earbuds = DeviceEntry {
            name: "UGREEN H6 Pro".into(),
            ..max5c()
        };
        let connection = Connection::new(earbuds, HeadsetState::default());
        assert!(
            connection
                .listed_features()
                .contains(&Feature::WearDetection)
        );
        assert_eq!(connection.tab, Tab::Device);
        assert_eq!(connection.page, Page::Main);
    }
}
