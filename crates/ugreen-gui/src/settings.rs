//! The one preference the GUI keeps between runs: the colour theme.

use std::path::PathBuf;

use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    /// Follow the operating system.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.key() == key.trim())
    }

    pub fn preference(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Light => egui::ThemePreference::Light,
            Self::Dark => egui::ThemePreference::Dark,
        }
    }
}

/// `%APPDATA%\ugreen-gui\theme` on Windows, `~/.config/ugreen-gui/theme`
/// (or under `$XDG_CONFIG_HOME`) elsewhere.
fn theme_file() -> Option<PathBuf> {
    let non_empty = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    let base = if cfg!(target_os = "windows") {
        PathBuf::from(non_empty("APPDATA")?)
    } else if let Some(dir) = non_empty("XDG_CONFIG_HOME") {
        PathBuf::from(dir)
    } else {
        PathBuf::from(non_empty("HOME")?).join(".config")
    };
    Some(base.join("ugreen-gui").join("theme"))
}

/// The saved theme; `System` when nothing (readable) was saved.
pub fn load_theme() -> ThemeChoice {
    theme_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| ThemeChoice::from_key(&text))
        .unwrap_or_default()
}

/// Best effort: a theme that cannot be saved still applies to this run.
pub fn save_theme(choice: ThemeChoice) {
    let Some(path) = theme_file() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, choice.key());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_keys_round_trip() {
        for choice in ThemeChoice::ALL {
            assert_eq!(ThemeChoice::from_key(choice.key()), Some(choice));
        }
        assert_eq!(ThemeChoice::from_key("dark\n"), Some(ThemeChoice::Dark));
        assert_eq!(ThemeChoice::from_key("purple"), None);
        assert_eq!(ThemeChoice::default(), ThemeChoice::System);
    }
}
