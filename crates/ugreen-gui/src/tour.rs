//! Development aid, debug builds only: walks through every screen of the
//! demo headset and saves a screenshot of each (binary PPM files).
//!
//! `UGREEN_GUI_TOUR=<directory> cargo run -p ugreen-gui`

use std::io::Write;
use std::path::PathBuf;

use eframe::egui;

use super::{App, Page, Sheet, Tab};
use crate::model::Feature;
use crate::settings::ThemeChoice;
use crate::worker::Request;

type Setup = fn(&mut App, &egui::Context);

fn show(app: &mut App, tab: Tab, page: Page, sheet: Option<Sheet>) {
    if let Some(connection) = &mut app.connection {
        connection.tab = tab;
        connection.page = page;
        connection.sheet = sheet;
    }
}

const STEPS: &[(&str, Setup)] = &[
    ("01-devices", |_, _| {}),
    ("01b-settings", |app, _| app.settings_open = true),
    ("01c-settings-theme", |app, _| app.theme_sheet = true),
    ("02-device", |app, _| {
        app.settings_open = false;
        app.theme_sheet = false;
        if let Some(device) = app.devices.first().cloned() {
            app.request(Request::Connect(device), "Connecting…");
        }
    }),
    ("03-noise-mode", |app, _| {
        show(app, Tab::Device, Page::Main, Some(Sheet::NoiseMode));
    }),
    ("04-equalizer", |app, _| {
        show(app, Tab::Device, Page::Equalizer, None);
    }),
    ("05-game-mode", |app, _| {
        show(app, Tab::Device, Page::Feature(Feature::GameMode), None);
    }),
    ("06-dual-link", |app, _| {
        show(app, Tab::Device, Page::Feature(Feature::Multipoint), None);
    }),
    ("07-hi-res", |app, _| {
        show(app, Tab::Device, Page::Feature(Feature::Ldac), None);
    }),
    ("08-more", |app, _| show(app, Tab::More, Page::Main, None)),
    ("09-broadcast-voice", |app, _| {
        show(app, Tab::More, Page::BroadcastVoice, None);
    }),
    ("10-factory-reset", |app, _| {
        show(app, Tab::More, Page::Main, Some(Sheet::FactoryReset));
    }),
    ("11-theme", |app, _| {
        show(app, Tab::More, Page::Main, None);
        app.theme_sheet = true;
    }),
    ("12-dark-device", |app, ctx| {
        app.theme_sheet = false;
        app.theme = ThemeChoice::Dark;
        ctx.set_theme(egui::ThemePreference::Dark);
        show(app, Tab::Device, Page::Main, None);
    }),
    ("13-dark-more", |app, _| {
        show(app, Tab::More, Page::Main, None)
    }),
    ("14-dark-equalizer", |app, _| {
        show(app, Tab::Device, Page::Equalizer, None);
    }),
    // Only the main window is captured; this checks the console opens.
    ("15-debug-console-open", |app, _| app.debug_open = true),
];

/// Frames to let animations settle before a screenshot.
const SETTLE_FRAMES: u32 = 30;

pub struct Tour {
    directory: PathBuf,
    step: usize,
    prepared: bool,
    settled: u32,
    waiting_for_image: bool,
}

impl Tour {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            step: 0,
            prepared: false,
            settled: 0,
            waiting_for_image: false,
        }
    }

    pub fn drive(&mut self, app: &mut App, ctx: &egui::Context) {
        ctx.request_repaint();
        let Some((name, setup)) = STEPS.get(self.step) else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            if let Err(error) = self.save(name, &image) {
                eprintln!("tour: could not save {name}: {error}");
            }
            self.step += 1;
            self.prepared = false;
            self.waiting_for_image = false;
            return;
        }
        if self.waiting_for_image {
            return;
        }
        if !self.prepared {
            setup(app, ctx);
            self.prepared = true;
            self.settled = 0;
        }
        // Wait for the worker (scan, connect) and then for animations.
        if app.idle() && app.scanned_once {
            self.settled += 1;
        }
        if self.settled > SETTLE_FRAMES {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.waiting_for_image = true;
        }
    }

    fn save(&self, name: &str, image: &egui::ColorImage) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.directory)?;
        let mut file = std::fs::File::create(self.directory.join(format!("{name}.ppm")))?;
        write!(file, "P6\n{} {}\n255\n", image.width(), image.height())?;
        let bytes: Vec<u8> = image
            .pixels
            .iter()
            .flat_map(|pixel| [pixel.r(), pixel.g(), pixel.b()])
            .collect();
        file.write_all(&bytes)
    }
}
