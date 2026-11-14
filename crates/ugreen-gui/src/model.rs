//! What the GUI knows about a headset, independent of egui and of Bluetooth.

use serde_json::Value;
use ugreen_core::protocol::{ProtocolCommand, ProtocolType};

/// Equalizer presets the UG1 protocol code knows (values 1..=8).
pub const EQ_PRESETS: [&str; 8] = [
    "classic",
    "bass",
    "treble",
    "pop",
    "rock",
    "jazz",
    "electronic",
    "classical",
];

/// Voice prompt languages the protocol code knows.
pub const PROMPT_LANGUAGES: [&str; 2] = ["English", "Chinese"];

/// Range accepted by the CLI's `vol` command.
pub const PROMPT_VOLUME_RANGE: std::ops::RangeInclusive<u8> = 1..=15;

/// Protocol used by a paired device, when the GUI supports it.
///
/// Only the HiTune Max5c is accepted: it is the only model this GUI has been
/// exercised against, and its noise-control byte layout is what
/// [`AncChoice`] assumes.
pub fn supported_protocol(device_name: &str) -> Option<ProtocolType> {
    let key: String = device_name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    key.contains("max5c").then_some(ProtocolType::Ug1)
}

/// The headset's single noise-control setting.
///
/// The protocol decoder reports it as two strings (`ancMode`, `ancDepth`)
/// where `ancMode == "adaptive"` really means "noise cancelling on"; this
/// folds them back into the one choice the headset stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AncChoice {
    Off,
    Transparency,
    Light,
    Medium,
    Deep,
    Adaptive,
}

impl AncChoice {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Transparency,
        Self::Light,
        Self::Medium,
        Self::Deep,
        Self::Adaptive,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Transparency => "Transparency",
            Self::Light => "Light",
            Self::Medium => "Medium",
            Self::Deep => "Deep",
            Self::Adaptive => "Adaptive",
        }
    }

    /// The `mode` string `ProtocolCommand::SetAnc` expects.
    pub fn protocol_mode(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Transparency => "transparent",
            Self::Light => "light",
            Self::Medium => "medium",
            Self::Deep => "deep",
            Self::Adaptive => "adaptive",
        }
    }

    pub fn from_protocol_mode(mode: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|choice| choice.protocol_mode() == mode)
    }

    /// Folds the decoder's `ancMode` / `ancDepth` pair into one choice.
    pub fn from_report(mode: &str, depth: &str) -> Option<Self> {
        match (mode, depth) {
            ("off", _) => Some(Self::Off),
            ("transparent", _) => Some(Self::Transparency),
            ("adaptive", "light") => Some(Self::Light),
            ("adaptive", "medium") => Some(Self::Medium),
            ("adaptive", "deep") => Some(Self::Deep),
            ("adaptive", "adaptive") => Some(Self::Adaptive),
            _ => None,
        }
    }

    /// The noise-cancelling strengths, weakest first, then automatic.
    pub const INTENSITIES: [Self; 4] = [Self::Light, Self::Medium, Self::Deep, Self::Adaptive];

    /// Whether this choice is "noise cancelling on" at some strength.
    pub fn is_noise_cancelling(self) -> bool {
        Self::INTENSITIES.contains(&self)
    }

    /// The strength the headset still remembers while noise cancelling is
    /// off or in transparency, when the report says (`ancDepth` of `light`,
    /// `medium` or `deep`).
    pub fn remembered_intensity(depth: &str) -> Option<Self> {
        match depth {
            "light" => Some(Self::Light),
            "medium" => Some(Self::Medium),
            "deep" => Some(Self::Deep),
            _ => None,
        }
    }

    /// Inverse of [`Self::from_report`], used by the demo headset.
    pub fn to_report(self) -> (&'static str, &'static str) {
        match self {
            Self::Off => ("off", "adaptive"),
            Self::Transparency => ("transparent", "adaptive"),
            Self::Light => ("adaptive", "light"),
            Self::Medium => ("adaptive", "medium"),
            Self::Deep => ("adaptive", "deep"),
            Self::Adaptive => ("adaptive", "adaptive"),
        }
    }
}

/// A change the GUI can ask for. There is deliberately no factory reset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    Anc(AncChoice),
    Eq(&'static str),
    PromptLanguage(&'static str),
    PromptVolume(u8),
}

impl Setting {
    pub fn command(&self) -> ProtocolCommand {
        match self {
            Self::Anc(choice) => ProtocolCommand::SetAnc {
                mode: choice.protocol_mode().to_string(),
                depth: None,
            },
            Self::Eq(mode) => ProtocolCommand::SetEq {
                mode: (*mode).to_string(),
            },
            Self::PromptLanguage(value) => ProtocolCommand::SetPromptLanguage {
                value: (*value).to_string(),
            },
            Self::PromptVolume(value) => ProtocolCommand::SetPromptVolume { value: *value },
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Anc(choice) => format!("noise control → {}", choice.label()),
            Self::Eq(mode) => format!("equalizer → {mode}"),
            Self::PromptLanguage(value) => format!("prompt language → {value}"),
            Self::PromptVolume(value) => format!("prompt volume → {value}"),
        }
    }
}

/// Last values read from the headset. `None` means "not reported".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeadsetState {
    pub firmware: Option<String>,
    /// Single level: the Max5c is over-ear and reports one battery.
    pub battery: Option<u8>,
    pub anc: Option<AncChoice>,
    /// Noise-cancelling strength to go back to when it is switched on again.
    pub anc_intensity: Option<AncChoice>,
    pub eq: Option<String>,
    pub prompt_language: Option<String>,
    pub prompt_volume: Option<u8>,
}

impl HeadsetState {
    /// Merges the `data` of a decoded response; absent fields are kept.
    pub fn apply(&mut self, data: &Value) {
        if let Some(version) = data.get("firmwareVersion").and_then(Value::as_str) {
            self.firmware = Some(version.to_string());
        }
        if let Some(battery) = data.get("battery") {
            let level = ["level", "left", "right"]
                .iter()
                .find_map(|key| battery.get(key).and_then(Value::as_u64));
            if let Some(level) = level {
                self.battery = Some(level.min(100) as u8);
            }
        }
        if let Some(mode) = data.get("ancMode").and_then(Value::as_str) {
            let depth = data.get("ancDepth").and_then(Value::as_str).unwrap_or("");
            self.anc = AncChoice::from_report(mode, depth);
            self.anc_intensity = match self.anc {
                Some(choice) if choice.is_noise_cancelling() => Some(choice),
                _ => AncChoice::remembered_intensity(depth),
            };
        }
        if let Some(eq) = data.get("eqMode").and_then(Value::as_str) {
            self.eq = Some(eq.to_string());
        }
        if let Some(language) = data.get("promptLanguage").and_then(Value::as_str) {
            self.prompt_language = Some(language.to_string());
        }
        if let Some(volume) = data.get("promptVolume").and_then(Value::as_u64) {
            self.prompt_volume = Some(volume.min(255) as u8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use ugreen_core::protocol::{ProtocolFacade, UgProtocolFacade};

    /// Real `info` response of a HiTune Max5c (firmware 0.5.7): ANC off,
    /// EQ treble, English prompts at volume 9, battery 90.
    const REAL_INFO: [u8; 30] = [
        0xDD, 0xEE, 0xFF, 0x04, 0x01, 0x16, 0x5A, 0xFF, 0xFF, 0xA0, 0x07, 0x00, 0x00, 0x01, 0x08,
        0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x95, 0xE3,
    ];
    const REAL_FIRMWARE: [u8; 14] = [
        0xDD, 0xEE, 0xFF, 0x01, 0x01, 0x06, 0x00, 0x05, 0x07, 0x00, 0x00, 0x00, 0xDD, 0x67,
    ];

    #[test]
    fn state_from_real_headset_frames() {
        let facade = UgProtocolFacade::new();
        let mut state = HeadsetState::default();
        for frame in [&REAL_FIRMWARE[..], &REAL_INFO[..]] {
            let decoded = facade.decode_inner(ProtocolType::Ug1, frame).unwrap();
            state.apply(&decoded.data);
        }
        assert_eq!(
            state,
            HeadsetState {
                firmware: Some("0.5.7".into()),
                battery: Some(90),
                anc: Some(AncChoice::Off),
                // Raw byte 0xA0: off, with "deep" remembered.
                anc_intensity: Some(AncChoice::Deep),
                eq: Some("treble".into()),
                prompt_language: Some("English".into()),
                prompt_volume: Some(9),
            }
        );
    }

    #[test]
    fn anc_choices_round_trip_through_the_decoder_strings() {
        for choice in AncChoice::ALL {
            let (mode, depth) = choice.to_report();
            assert_eq!(AncChoice::from_report(mode, depth), Some(choice));
            assert_eq!(
                AncChoice::from_protocol_mode(choice.protocol_mode()),
                Some(choice)
            );
        }
        assert_eq!(AncChoice::from_report("unknown", "unknown"), None);
        // What the headset reported after `anc deep` and after `anc off`.
        assert_eq!(
            AncChoice::from_report("adaptive", "deep"),
            Some(AncChoice::Deep)
        );
        assert_eq!(AncChoice::from_report("off", "deep"), Some(AncChoice::Off));
    }

    /// Each choice must encode to its own byte and decode back to itself.
    #[test]
    fn anc_choices_survive_the_ug1_wire_format() {
        let facade = UgProtocolFacade::new();
        let expected = [0xD0, 0xD2, 0xC1, 0xB1, 0xA1, 0xD1];
        for (choice, byte) in AncChoice::ALL.into_iter().zip(expected) {
            let command = Setting::Anc(choice).command();
            let payload = command.payload(ProtocolType::Ug1);
            let frame = facade
                .encode_inner(ProtocolType::Ug1, &command, &payload, 0)
                .unwrap();
            // AA BB CC cmd len data crc crc
            assert_eq!(frame[5], byte, "{choice:?}");
        }
    }

    #[test]
    fn intensity_follows_the_report() {
        let mut state = HeadsetState::default();
        state.apply(&json!({ "ancMode": "adaptive", "ancDepth": "light" }));
        assert_eq!(state.anc_intensity, Some(AncChoice::Light));
        state.apply(&json!({ "ancMode": "transparent", "ancDepth": "medium" }));
        assert_eq!(state.anc, Some(AncChoice::Transparency));
        assert_eq!(state.anc_intensity, Some(AncChoice::Medium));
        // Plain "off" (0xD0) decodes with depth "adaptive": nothing remembered.
        state.apply(&json!({ "ancMode": "off", "ancDepth": "adaptive" }));
        assert_eq!(state.anc_intensity, None);
        assert!(
            AncChoice::INTENSITIES
                .iter()
                .all(|c| c.is_noise_cancelling())
        );
        assert!(!AncChoice::Off.is_noise_cancelling());
        assert!(!AncChoice::Transparency.is_noise_cancelling());
    }

    #[test]
    fn apply_keeps_fields_a_response_does_not_mention() {
        let mut state = HeadsetState {
            firmware: Some("0.5.7".into()),
            ..Default::default()
        };
        state.apply(&json!({ "battery": { "left": 55 }, "eqMode": "bass" }));
        assert_eq!(state.firmware.as_deref(), Some("0.5.7"));
        assert_eq!(state.battery, Some(55));
        assert_eq!(state.eq.as_deref(), Some("bass"));
        assert_eq!(state.anc, None);
    }

    #[test]
    fn only_the_max5c_is_supported() {
        assert_eq!(
            supported_protocol("UGREEN HiTune Max5c"),
            Some(ProtocolType::Ug1)
        );
        assert_eq!(supported_protocol("HiTune Max 5c"), Some(ProtocolType::Ug1));
        assert_eq!(supported_protocol("UGREEN HiTune Max5"), None);
        assert_eq!(supported_protocol("Some Speaker"), None);
    }

    #[test]
    fn no_setting_can_produce_a_factory_reset() {
        let settings = AncChoice::ALL
            .into_iter()
            .map(Setting::Anc)
            .chain(EQ_PRESETS.into_iter().map(Setting::Eq))
            .chain(PROMPT_LANGUAGES.into_iter().map(Setting::PromptLanguage))
            .chain(PROMPT_VOLUME_RANGE.map(Setting::PromptVolume));
        for setting in settings {
            assert_ne!(setting.command().name(), "FactoryReset", "{setting:?}");
        }
    }
}
