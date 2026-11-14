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
mod worker;

use std::time::Duration;

use eframe::egui;

fn main() -> eframe::Result {
    let demo = std::env::args().skip(1).any(|arg| arg == "--demo");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 600.0])
            .with_min_inner_size([360.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "UGREEN Headset",
        options,
        Box::new(move |cc| {
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
            Ok(Box::new(app::App::new(requests, events, demo)))
        }),
    )
}
