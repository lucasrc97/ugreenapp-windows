//! What the GUI knows about a headset, independent of egui and of Bluetooth.

use serde_json::Value;
use ugreen_core::protocol::{ProtocolCommand, ProtocolType};

/// Equalizer presets the UG1 protocol code knows, in the order the UGREEN
/// phone app lists them.
pub const EQ_PRESETS: [&str; 8] = [
    "classic",
    "jazz",
    "electronic",
    "pop",
    "classical",
    "rock",
    "bass",
    "treble",
];

/// Name of an equalizer preset as the UGREEN phone app shows it.
pub fn eq_label(preset: &str) -> &'static str {
    match preset {
        "classic" => "Classic",
        "jazz" => "Jazz",
        "electronic" => "Electronic",
        "pop" => "Popular",
        "classical" => "Classical",
        "rock" => "Rock",
        "bass" => "Bass",
        "treble" => "Treble",
        _ => "Unknown",
    }
}

/// Broadcast voices (prompt languages) the protocol code knows. The phone
/// app also has "Ringtone", whose value is not known here.
pub const PROMPT_LANGUAGES: [&str; 2] = ["Chinese", "English"];

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

/// Whether a model has wear (in-ear) detection.
///
/// Of the models this project knows, only the H6 Pro earbuds document it
/// (auto pause when an earbud is removed). The HiTune Max5c has no such
/// sensor, and it stops answering when sent the command.
pub fn has_wear_detection(device_name: &str) -> bool {
    let key: String = device_name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    key.contains("h6pro")
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

    /// Name as the UGREEN phone app shows it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Transparency => "Ambient sound",
            Self::Light => "Gentle",
            Self::Medium => "General",
            Self::Deep => "Ultra",
            Self::Adaptive => "Adaptive",
        }
    }

    /// Where a noise cancelling mode is meant to be used, if it says.
    pub fn hint(self) -> Option<&'static str> {
        match self {
            Self::Light => Some("Ideal for quiet places"),
            Self::Medium => Some("Ideal for noisy places (downtown, cafe)"),
            Self::Deep => Some("Ideal for extremely noisy places"),
            _ => None,
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

    /// The noise cancelling modes: automatic, then weakest first.
    pub const INTENSITIES: [Self; 4] = [Self::Adaptive, Self::Light, Self::Medium, Self::Deep];

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

/// An on/off feature of the headset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    GameMode,
    Multipoint,
    SpatialAudio,
    Ldac,
    WearDetection,
}

impl Feature {
    pub const ALL: [Self; 5] = [
        Self::GameMode,
        Self::Multipoint,
        Self::SpatialAudio,
        Self::Ldac,
        Self::WearDetection,
    ];

    /// Name as the UGREEN phone app shows it. "Dual link" is multipoint and
    /// "Hi-Res audio decoding" is the LDAC codec.
    pub fn label(self) -> &'static str {
        match self {
            Self::GameMode => "Game mode",
            Self::Multipoint => "Dual link",
            Self::SpatialAudio => "Spatial sound effects",
            Self::Ldac => "Hi-Res audio decoding",
            Self::WearDetection => "Wear detection",
        }
    }

    /// The field of a decoded response that carries this feature.
    pub fn report_key(self) -> &'static str {
        match self {
            Self::GameMode => "gameMode",
            Self::Multipoint => "dualLink",
            Self::SpatialAudio => "spatialAudio",
            Self::Ldac => "highQuality",
            Self::WearDetection => "wearDetection",
        }
    }

    /// Game mode and multipoint cannot be used together with LDAC: the
    /// protocol decoder reports both as off whenever LDAC is on.
    pub fn excluded_by_ldac(self) -> bool {
        matches!(self, Self::GameMode | Self::Multipoint)
    }

    /// Changing LDAC makes the headset drop its Bluetooth connection, and it
    /// does not come back by itself: on a Max5c (firmware 0.5.7) it had to be
    /// switched off and on, then connected to the PC again.
    pub fn drops_connection(self) -> bool {
        self == Self::Ldac
    }

    /// Why the GUI shows this feature but does not let it be changed on the
    /// given device, if that is the case.
    pub fn locked_reason(self, device_name: &str) -> Option<&'static str> {
        match self {
            // Changing it cut the Bluetooth link of a Max5c on Windows until
            // the adapter was replugged and the headset paired again.
            Self::Ldac => Some(
                "Hi-Res audio decoding (LDAC) cannot be changed from this app yet: on \
                 Windows the change breaks the Bluetooth connection. Use the UGREEN \
                 phone app.",
            ),
            Self::WearDetection if !has_wear_detection(device_name) => {
                Some("This model does not have wear detection.")
            }
            _ => None,
        }
    }

    pub fn command(self, enabled: bool) -> ProtocolCommand {
        match self {
            Self::GameMode => ProtocolCommand::SetGameMode { enabled },
            Self::Multipoint => ProtocolCommand::SetDualConnect { enabled },
            Self::SpatialAudio => ProtocolCommand::SetSpatialAudio { enabled },
            Self::Ldac => ProtocolCommand::SetHighQuality { enabled },
            Self::WearDetection => ProtocolCommand::SetWearDetection { enabled },
        }
    }

    /// Inverse of [`Self::command`], used by the demo headset.
    pub fn from_command(command: &ProtocolCommand) -> Option<(Self, bool)> {
        Some(match *command {
            ProtocolCommand::SetGameMode { enabled } => (Self::GameMode, enabled),
            ProtocolCommand::SetDualConnect { enabled } => (Self::Multipoint, enabled),
            ProtocolCommand::SetSpatialAudio { enabled } => (Self::SpatialAudio, enabled),
            ProtocolCommand::SetHighQuality { enabled } => (Self::Ldac, enabled),
            ProtocolCommand::SetWearDetection { enabled } => (Self::WearDetection, enabled),
            _ => return None,
        })
    }
}

/// On/off features as last reported. `None` means "not reported".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features {
    pub game_mode: Option<bool>,
    pub multipoint: Option<bool>,
    pub spatial_audio: Option<bool>,
    pub ldac: Option<bool>,
    pub wear_detection: Option<bool>,
}

impl Features {
    pub fn get(&self, feature: Feature) -> Option<bool> {
        match feature {
            Feature::GameMode => self.game_mode,
            Feature::Multipoint => self.multipoint,
            Feature::SpatialAudio => self.spatial_audio,
            Feature::Ldac => self.ldac,
            Feature::WearDetection => self.wear_detection,
        }
    }

    pub fn set(&mut self, feature: Feature, value: Option<bool>) {
        *match feature {
            Feature::GameMode => &mut self.game_mode,
            Feature::Multipoint => &mut self.multipoint,
            Feature::SpatialAudio => &mut self.spatial_audio,
            Feature::Ldac => &mut self.ldac,
            Feature::WearDetection => &mut self.wear_detection,
        } = value;
    }
}

/// A change the GUI can ask for.
///
/// Factory reset is deliberately not a setting: it has its own request
/// (`Request::FactoryReset`), which the UI only sends after a confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    Anc(AncChoice),
    Eq(&'static str),
    PromptLanguage(&'static str),
    PromptVolume(u8),
    Toggle(Feature, bool),
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
            Self::Toggle(feature, enabled) => feature.command(*enabled),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Anc(choice) => format!("Noise cancelling → {}", choice.label()),
            Self::Eq(mode) => format!("Equalizer → {}", eq_label(mode)),
            Self::PromptLanguage(value) => format!("Broadcast voice → {value}"),
            Self::PromptVolume(value) => format!("Prompt's volume → {value}"),
            Self::Toggle(feature, enabled) => {
                format!(
                    "{} → {}",
                    feature.label(),
                    if *enabled { "on" } else { "off" }
                )
            }
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
    pub features: Features,
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
        for feature in Feature::ALL {
            if let Some(enabled) = data.get(feature.report_key()).and_then(Value::as_bool) {
                self.features.set(feature, Some(enabled));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use ugreen_core::protocol::{ProtocolFacade, UgProtocolFacade};

    /// Real `info` response of a HiTune Max5c (firmware 0.5.7): ANC off,
    /// EQ treble, English prompts at volume 9, battery 90, LDAC on.
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
                features: Features {
                    game_mode: Some(false),
                    multipoint: Some(false),
                    spatial_audio: Some(false),
                    ldac: Some(true),
                    wear_detection: Some(false),
                },
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
            .chain(PROMPT_VOLUME_RANGE.map(Setting::PromptVolume))
            .chain(Feature::ALL.into_iter().flat_map(|feature| {
                [true, false].map(|enabled| Setting::Toggle(feature, enabled))
            }));
        for setting in settings {
            assert_ne!(setting.command().name(), "FactoryReset", "{setting:?}");
        }
    }

    /// Each toggle must encode to its own UG1 command with a 0/1 byte, and
    /// map back to the same feature.
    #[test]
    fn toggles_survive_the_ug1_wire_format() {
        let facade = UgProtocolFacade::new();
        let command_ids = [8, 6, 18, 11, 19];
        for (feature, command_id) in Feature::ALL.into_iter().zip(command_ids) {
            for enabled in [true, false] {
                let command = Setting::Toggle(feature, enabled).command();
                assert_eq!(Feature::from_command(&command), Some((feature, enabled)));
                let payload = command.payload(ProtocolType::Ug1);
                let frame = facade
                    .encode_inner(ProtocolType::Ug1, &command, &payload, 0)
                    .unwrap();
                // AA BB CC cmd len data crc crc
                assert_eq!(
                    frame[3..6],
                    [command_id, 1, u8::from(enabled)],
                    "{feature:?}"
                );
            }
        }
    }

    #[test]
    fn features_follow_the_report() {
        let mut state = HeadsetState::default();
        state.apply(&json!({ "gameMode": true, "highQuality": false }));
        assert_eq!(state.features.get(Feature::GameMode), Some(true));
        assert_eq!(state.features.get(Feature::Ldac), Some(false));
        assert_eq!(state.features.get(Feature::Multipoint), None);
        assert!(Feature::GameMode.excluded_by_ldac());
        assert!(Feature::Multipoint.excluded_by_ldac());
        assert!(!Feature::SpatialAudio.excluded_by_ldac());
        assert!(Feature::Ldac.drops_connection());
        let max5c = "UGREEN HiTune Max5c";
        assert!(Feature::Ldac.locked_reason(max5c).is_some());
        assert!(Feature::WearDetection.locked_reason(max5c).is_some());
        assert!(
            Feature::WearDetection
                .locked_reason("UGREEN H6 Pro")
                .is_none()
        );
        for feature in [
            Feature::GameMode,
            Feature::Multipoint,
            Feature::SpatialAudio,
        ] {
            assert_eq!(feature.locked_reason(max5c), None);
        }
        assert!(!Feature::Multipoint.drops_connection());
    }
}
