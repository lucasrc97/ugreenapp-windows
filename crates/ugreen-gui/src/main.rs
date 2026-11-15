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
mod debug;
mod model;
mod settings;
mod widgets;
mod worker;

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use eframe::egui;

/// Window and taskbar icon; the executable's own icon comes from build.rs.
fn icon() -> Arc<egui::IconData> {
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    ICON.get_or_init(|| {
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
            .expect("the bundled icon is a valid PNG");
        Arc::new(icon)
    })
    .clone()
}

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
            .with_min_inner_size([360.0, 480.0])
            .with_icon(icon()),
        // The window opens where, and as large as, it was closed. The
        // screenshot tour always uses the default size.
        persist_window: tour_directory.is_none(),
        persistence_path: settings::window_file(),
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
            debug::install(&cc.egui_ctx);
            log::info!(
                "ugreen-gui {} on {}{}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                if demo { ", demo mode" } else { "" }
            );
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
