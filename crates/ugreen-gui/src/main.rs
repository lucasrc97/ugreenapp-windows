//! Desktop GUI for UGREEN headsets (egui / eframe).
//!
//! `ugreen-gui`         talks to the real headset.
//! `ugreen-gui --demo`  uses a pretend headset, for trying the UI without
//!                      Bluetooth (for example inside WSL).

// No console window next to the GUI on Windows release builds.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod model;
mod settings;
mod widgets;
mod worker;

use std::time::Duration;

use eframe::egui;

fn main() -> eframe::Result {
    // Debug builds only: `UGREEN_GUI_TOUR=<dir>` screenshots every screen of
    // the demo headset into that directory, then exits.
    #[cfg(debug_assertions)]
    let tour_directory = std::env::var_os("UGREEN_GUI_TOUR").map(std::path::PathBuf::from);
    #[cfg(not(debug_assertions))]
    let tour_directory: Option<std::path::PathBuf> = None;

    let demo = tour_directory.is_some() || std::env::args().skip(1).any(|arg| arg == "--demo");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 800.0])
            .with_min_inner_size([360.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "UGREEN Headset",
        options,
        Box::new(move |cc| {
            let theme = if tour_directory.is_some() {
                settings::ThemeChoice::Light
            } else {
                settings::load_theme()
            };
            widgets::install(&cc.egui_ctx);
            cc.egui_ctx.set_theme(theme.preference());

            let ctx = cc.egui_ctx.clone();
            let wake = move || ctx.request_repaint();
            let (requests, events) = if demo {
                worker::spawn(
                    || worker::DemoBackend::new(Duration::from_millis(600)),
                    wake,
                )
            } else {
                worker::spawn(worker::BluetoothBackend::new, wake)
            };
            #[allow(unused_mut)]
            let mut app = app::App::new(requests, events, demo, theme);
            #[cfg(debug_assertions)]
            {
                app.tour = tour_directory.map(app::tour::Tour::new);
            }
            Ok(Box::new(app))
        }),
    )
}
