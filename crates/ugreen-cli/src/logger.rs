//! Minimal `log` backend: everything goes to stderr, so stdout (including
//! `--json` output) stays clean.

use log::{Level, LevelFilter, Log, Metadata, Record};

struct StderrLogger;

static LOGGER: StderrLogger = StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let label = match record.level() {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug | Level::Trace => "debug",
        };
        eprintln!("[{label}] {}", record.args());
    }

    fn flush(&self) {}
}

/// Installs the logger. Info and above by default; `verbose` adds debug
/// output such as the raw bytes sent and received.
pub fn init(verbose: bool) {
    // Fails only if a logger is already installed, which is harmless.
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(level_for(verbose));
}

pub fn level_for(verbose: bool) -> LevelFilter {
    if verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    }
}
