//! The Debug console: a separate window that shows, while the app runs, what
//! it asks of the headset and what comes back, raw bytes included.
//!
//! Everything logged through the `log` crate by this program and by
//! `ugreen-core` is kept in memory from start-up, so the console can be
//! opened after something went wrong. The main window only shows plain
//! messages; the technical ones end up here.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui::{self, RichText};
use log::Level;

use crate::widgets::Palette;

/// Oldest lines are dropped beyond this many.
const MAX_LINES: usize = 2000;

struct Line {
    at: Duration,
    level: Level,
    text: String,
}

impl Line {
    fn render(&self) -> String {
        let seconds = self.at.as_secs();
        format!(
            "{:02}:{:02}.{:03}  {:<5}  {}",
            seconds / 60,
            seconds % 60,
            self.at.subsec_millis(),
            self.level,
            self.text
        )
    }
}

struct Console {
    started: Instant,
    lines: Mutex<VecDeque<Line>>,
    /// Repaints the windows when a line arrives from the worker thread.
    wake: egui::Context,
}

static CONSOLE: OnceLock<Console> = OnceLock::new();

fn lines() -> Option<MutexGuard<'static, VecDeque<Line>>> {
    let console = CONSOLE.get()?;
    Some(
        console
            .lines
            .lock()
            .unwrap_or_else(|lock| lock.into_inner()),
    )
}

/// Whether a log record is ours: the GUI's or `ugreen-core`'s, not the
/// windowing libraries'.
fn wanted(target: &str) -> bool {
    target.starts_with("ugreen")
}

impl log::Log for Console {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        wanted(metadata.target())
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = Line {
            at: self.started.elapsed(),
            level: record.level(),
            text: record.args().to_string(),
        };
        let mut lines = self.lines.lock().unwrap_or_else(|lock| lock.into_inner());
        if lines.len() == MAX_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
        drop(lines);
        self.wake.request_repaint();
    }

    fn flush(&self) {}
}

/// Starts collecting log lines. Call once at start-up.
pub fn install(ctx: &egui::Context) {
    let console = CONSOLE.get_or_init(|| Console {
        started: Instant::now(),
        lines: Mutex::new(VecDeque::new()),
        wake: ctx.clone(),
    });
    if log::set_logger(console).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}

fn contents(ui: &mut egui::Ui) {
    let palette = Palette::for_ui(ui);
    ui.horizontal(|ui| {
        if ui.button("Copy all").clicked() {
            let text = lines()
                .map(|lines| {
                    lines
                        .iter()
                        .map(Line::render)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            ui.ctx().copy_text(text);
        }
        if ui.button("Clear").clicked()
            && let Some(mut lines) = lines()
        {
            lines.clear();
        }
        ui.label(
            RichText::new("What the app asks of the headset and what comes back.")
                .color(palette.weak)
                .size(13.0),
        );
    });
    ui.separator();
    egui::ScrollArea::both()
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            let Some(lines) = lines() else {
                return;
            };
            if lines.is_empty() {
                ui.label(RichText::new("Nothing yet.").color(palette.weak));
            }
            for line in lines.iter() {
                let color = match line.level {
                    Level::Error => palette.danger,
                    Level::Warn => egui::Color32::from_rgb(200, 150, 40),
                    Level::Info => palette.text,
                    Level::Debug | Level::Trace => palette.weak,
                };
                let text = RichText::new(line.render())
                    .monospace()
                    .size(12.5)
                    .color(color);
                ui.add(egui::Label::new(text).extend());
            }
        });
}

/// Shows the console in a window of its own for as long as `open` is set;
/// closing that window clears it.
pub fn show(ctx: &egui::Context, open: &mut bool) {
    let builder = egui::ViewportBuilder::default()
        .with_title("UGREEN Headset: Debug console")
        .with_inner_size([700.0, 440.0])
        .with_min_inner_size([360.0, 200.0])
        .with_icon(crate::icon());
    let id = egui::ViewportId::from_hash_of("debug_console");
    ctx.show_viewport_immediate(id, builder, |ctx, class| {
        if class == egui::ViewportClass::Embedded {
            // No separate windows on this backend: a floating one instead.
            egui::Window::new("Debug console")
                .open(open)
                .show(ctx, contents);
            return;
        }
        let palette = Palette::of(ctx.theme());
        let frame = egui::Frame::new().fill(palette.card).inner_margin(10);
        egui::CentralPanel::default()
            .frame(frame)
            .show(ctx, contents);
        if ctx.input(|input| input.viewport().close_requested()) {
            *open = false;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_shows_time_level_and_text() {
        let line = Line {
            at: Duration::from_millis(75_042),
            level: Level::Warn,
            text: "RX DDEEFF".into(),
        };
        assert_eq!(line.render(), "01:15.042  WARN   RX DDEEFF");
    }

    #[test]
    fn only_this_program_and_the_core_are_logged() {
        assert!(wanted("ugreen_gui::app"));
        assert!(wanted("ugreen_core::transport::exchange"));
        assert!(!wanted("winit::window"));
    }
}
