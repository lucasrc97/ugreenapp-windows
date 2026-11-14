//! Golden (characterization) tests for the wire format.
//!
//! The expected bytes below were captured from the code as it was BEFORE the
//! workspace/Windows refactor and are frozen here. They pin current behaviour,
//! including quirks; they do not claim the behaviour is what the headset wants.
//!
//! Request frames for UG1 `QueryInfo`, `QueryFirmware`, `SetAnc`, `SetEq`,
//! `SetPromptLanguage` and `SetPromptVolume` are the ones reported as working
//! on a real HiTune Max5c over Linux RFCOMM. Every *response* frame in this
//! file is SYNTHETIC (built from the decoder's own layout, with a valid CRC),
//! not a capture from real hardware. Their trailing checksums were updated
//! from CRC-16/MODBUS to CRC-16/CCITT-FALSE when the decoder was corrected to
//! check what the headset really sends (see `real_captures.rs`); request
//! bytes were not touched.
//!
//! Only the public API is used so this file stays byte-identical across moves.

use crate::protocol::{
    ProtocolCommand, ProtocolFacade, ProtocolType, UgProtocolFacade, find_complete_rcsp_frame,
    find_complete_ug1_frame, unwrap_rcsp_custom, wrap_rcsp_custom,
};
use serde_json::json;

fn encode(protocol: ProtocolType, command: &ProtocolCommand) -> anyhow::Result<Vec<u8>> {
    let payload = command.payload(protocol);
    UgProtocolFacade::new().encode_inner(protocol, command, &payload, 5)
}

fn assert_encodes(protocol: ProtocolType, cases: &[(ProtocolCommand, &[u8])]) {
    for (command, expected) in cases {
        let actual = encode(protocol, command).expect("encode failed");
        assert_eq!(
            actual.as_slice(),
            *expected,
            "wire bytes changed for {command:?}"
        );
    }
}

fn anc(mode: &str) -> ProtocolCommand {
    ProtocolCommand::SetAnc {
        mode: mode.into(),
        depth: None,
    }
}

fn anc_depth(depth: u8) -> ProtocolCommand {
    ProtocolCommand::SetAnc {
        mode: "anc".into(),
        depth: Some(depth),
    }
}

fn eq(mode: &str) -> ProtocolCommand {
    ProtocolCommand::SetEq { mode: mode.into() }
}

fn lang(value: &str) -> ProtocolCommand {
    ProtocolCommand::SetPromptLanguage {
        value: value.into(),
    }
}

fn vol(value: u8) -> ProtocolCommand {
    ProtocolCommand::SetPromptVolume { value }
}

// ---------------------------------------------------------------- UG1 encode

#[test]
fn ug1_queries() {
    assert_encodes(
        ProtocolType::Ug1,
        &[
            (
                ProtocolCommand::QueryInfo,
                &[0xAA, 0xBB, 0xCC, 0x04, 0x01, 0x00, 0x31, 0x91],
            ),
            (
                ProtocolCommand::QueryFirmware,
                &[0xAA, 0xBB, 0xCC, 0x01, 0x01, 0x00, 0x21, 0x90],
            ),
        ],
    );
}

#[test]
fn ug1_anc_modes() {
    assert_encodes(
        ProtocolType::Ug1,
        &[
            (
                anc("off"),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xD0, 0xA1, 0xCE],
            ),
            (
                anc("transparency"),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xD2, 0x20, 0x0F],
            ),
            (
                anc("transparent"),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xD2, 0x20, 0x0F],
            ),
            (
                anc("light"),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xC1, 0x61, 0xC2],
            ),
            (
                anc("medium"),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xB1, 0x60, 0x26],
            ),
            (
                anc("deep"),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xA1, 0x61, 0xEA],
            ),
            (
                anc("adaptive"),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xD1, 0x60, 0x0E],
            ),
            // Quirk: `anc` and any unknown mode silently become adaptive.
            (
                anc("anc"),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xD1, 0x60, 0x0E],
            ),
            (
                anc("bogus"),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xD1, 0x60, 0x0E],
            ),
        ],
    );
}

#[test]
fn ug1_anc_depth_flag() {
    // `--depth` 0|1 => light, 2|3 => medium, 4+ => deep.
    assert_encodes(
        ProtocolType::Ug1,
        &[
            (
                anc_depth(0),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xC1, 0x61, 0xC2],
            ),
            (
                anc_depth(1),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xC1, 0x61, 0xC2],
            ),
            (
                anc_depth(2),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xB1, 0x60, 0x26],
            ),
            (
                anc_depth(3),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xB1, 0x60, 0x26],
            ),
            (
                anc_depth(4),
                &[0xAA, 0xBB, 0xCC, 0x09, 0x01, 0xA1, 0x61, 0xEA],
            ),
        ],
    );
}

#[test]
fn ug1_eq_presets() {
    assert_encodes(
        ProtocolType::Ug1,
        &[
            (
                eq("jazz"),
                &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x01, 0xA1, 0x91],
            ),
            (
                eq("electronic"),
                &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x02, 0xE1, 0x90],
            ),
            (eq("pop"), &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x03, 0x20, 0x50]),
            (
                eq("classical"),
                &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x04, 0x61, 0x92],
            ),
            (
                eq("rock"),
                &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x05, 0xA0, 0x52],
            ),
            (
                eq("bass"),
                &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x06, 0xE0, 0x53],
            ),
            (
                eq("treble"),
                &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x07, 0x21, 0x93],
            ),
            (
                eq("classic"),
                &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x08, 0x61, 0x97],
            ),
            // Quirk: `balanced` and any unknown preset become classic (8).
            (
                eq("balanced"),
                &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x08, 0x61, 0x97],
            ),
            (
                eq("bogus"),
                &[0xAA, 0xBB, 0xCC, 0x05, 0x01, 0x08, 0x61, 0x97],
            ),
        ],
    );
}

#[test]
fn ug1_prompt_language_and_volume() {
    assert_encodes(
        ProtocolType::Ug1,
        &[
            (
                lang("english"),
                &[0xAA, 0xBB, 0xCC, 0x0C, 0x01, 0x00, 0xB0, 0x53],
            ),
            (
                lang("chinese"),
                &[0xAA, 0xBB, 0xCC, 0x0C, 0x01, 0x01, 0x71, 0x93],
            ),
            // Quirk: unknown language silently becomes English.
            (
                lang("bogus"),
                &[0xAA, 0xBB, 0xCC, 0x0C, 0x01, 0x00, 0xB0, 0x53],
            ),
            (vol(1), &[0xAA, 0xBB, 0xCC, 0x11, 0x01, 0x01, 0xE1, 0x95]),
            (vol(8), &[0xAA, 0xBB, 0xCC, 0x11, 0x01, 0x08, 0x21, 0x93]),
            (vol(15), &[0xAA, 0xBB, 0xCC, 0x11, 0x01, 0x0F, 0x60, 0x51]),
        ],
    );
}

#[test]
fn ug1_switches() {
    use ProtocolCommand::*;
    assert_encodes(
        ProtocolType::Ug1,
        &[
            (
                SetWearDetection { enabled: true },
                &[0xAA, 0xBB, 0xCC, 0x13, 0x01, 0x01, 0x40, 0x55],
            ),
            (
                SetWearDetection { enabled: false },
                &[0xAA, 0xBB, 0xCC, 0x13, 0x01, 0x00, 0x81, 0x95],
            ),
            (
                SetGameMode { enabled: true },
                &[0xAA, 0xBB, 0xCC, 0x08, 0x01, 0x01, 0x30, 0x52],
            ),
            (
                SetGameMode { enabled: false },
                &[0xAA, 0xBB, 0xCC, 0x08, 0x01, 0x00, 0xF1, 0x92],
            ),
            (
                SetDualConnect { enabled: true },
                &[0xAA, 0xBB, 0xCC, 0x06, 0x01, 0x01, 0x51, 0x91],
            ),
            (
                SetDualConnect { enabled: false },
                &[0xAA, 0xBB, 0xCC, 0x06, 0x01, 0x00, 0x90, 0x51],
            ),
            (
                SetSpatialAudio { enabled: true },
                &[0xAA, 0xBB, 0xCC, 0x12, 0x01, 0x01, 0x11, 0x95],
            ),
            (
                SetSpatialAudio { enabled: false },
                &[0xAA, 0xBB, 0xCC, 0x12, 0x01, 0x00, 0xD0, 0x55],
            ),
            (
                SetHighQuality { enabled: true },
                &[0xAA, 0xBB, 0xCC, 0x0B, 0x01, 0x01, 0xC0, 0x52],
            ),
            (
                SetHighQuality { enabled: false },
                &[0xAA, 0xBB, 0xCC, 0x0B, 0x01, 0x00, 0x01, 0x92],
            ),
        ],
    );
}

#[test]
fn ug1_factory_reset_bytes() {
    // Encode only. This frame is never sent by any test.
    assert_encodes(
        ProtocolType::Ug1,
        &[(
            ProtocolCommand::FactoryReset,
            &[0xAA, 0xBB, 0xCC, 0x0E, 0x01, 0x00, 0x11, 0x93],
        )],
    );
}

#[test]
fn ug1_crc_is_crc16_modbus_little_endian() {
    // CRC-16/MODBUS over [cmd, len, data...], appended low byte first.
    // Independent check: MODBUS CRC of 04 01 00 is 0x9131.
    let frame = encode(ProtocolType::Ug1, &ProtocolCommand::QueryInfo).unwrap();
    assert_eq!(&frame[6..], &[0x31, 0x91]);
}

// ---------------------------------------------------------------- UG2 encode

#[test]
fn ug2_queries() {
    assert_encodes(
        ProtocolType::Ug2,
        &[
            (
                ProtocolCommand::QueryInfo,
                // Quirk: `highQualityDecoding` and `controlInfo` have no tag and
                // fall back to the battery tag (0x01).
                &[
                    0x27, 0x01, 0x00, 0x22, 0xFF, 0x00, 0x01, 0x00, 0x02, 0x00, 0x0C, 0x00, 0x91,
                    0x00, 0x04, 0x00, 0x08, 0x00, 0x03, 0x00, 0x90, 0x00, 0x95, 0x00, 0x01, 0x00,
                    0x94, 0x00, 0x1A, 0x00, 0x01, 0x00, 0x18, 0x00, 0x19, 0x00, 0x0A, 0x00,
                ],
            ),
            (
                ProtocolCommand::QueryFirmware,
                &[0x27, 0x01, 0x00, 0x02, 0x02, 0x00],
            ),
            (ProtocolCommand::FactoryReset, &[0x24, 0x01, 0x00, 0x00]),
        ],
    );
}

#[test]
fn ug2_setters() {
    assert_encodes(
        ProtocolType::Ug2,
        &[
            (anc("off"), &[0x35, 0x01, 0x00, 0x03, 0x01, 0x01, 0x00]),
            (
                anc("transparency"),
                &[0x35, 0x01, 0x00, 0x03, 0x01, 0x01, 0x02],
            ),
            (anc("adaptive"), &[0x35, 0x01, 0x00, 0x03, 0x01, 0x01, 0x01]),
            // Quirk: on UG2, light/medium/deep as a *mode* encode as 0 (off).
            (anc("light"), &[0x35, 0x01, 0x00, 0x03, 0x01, 0x01, 0x00]),
            (anc("medium"), &[0x35, 0x01, 0x00, 0x03, 0x01, 0x01, 0x00]),
            (anc("deep"), &[0x35, 0x01, 0x00, 0x03, 0x01, 0x01, 0x00]),
            (anc_depth(0), &[0x35, 0x01, 0x00, 0x03, 0x02, 0x01, 0x02]),
            (anc_depth(2), &[0x35, 0x01, 0x00, 0x03, 0x02, 0x01, 0x01]),
            (anc_depth(4), &[0x35, 0x01, 0x00, 0x03, 0x02, 0x01, 0x00]),
            (eq("jazz"), &[0x20, 0x01, 0x00, 0x01, 0x01]),
            (eq("bass"), &[0x20, 0x01, 0x00, 0x01, 0x06]),
            (eq("classic"), &[0x20, 0x01, 0x00, 0x01, 0x08]),
            (lang("english"), &[0x29, 0x01, 0x00, 0x03, 0x01, 0x01, 0x00]),
            (lang("chinese"), &[0x29, 0x01, 0x00, 0x03, 0x01, 0x01, 0x01]),
            (vol(1), &[0x29, 0x01, 0x00, 0x03, 0x02, 0x01, 0x01]),
            (vol(15), &[0x29, 0x01, 0x00, 0x03, 0x02, 0x01, 0x0F]),
        ],
    );
}

#[test]
fn unimplemented_encodes_are_errors() {
    use ProtocolCommand::*;
    for command in [
        SetWearDetection { enabled: true },
        SetGameMode { enabled: true },
        SetDualConnect { enabled: true },
        SetSpatialAudio { enabled: true },
        SetHighQuality { enabled: true },
    ] {
        assert!(encode(ProtocolType::Ug2, &command).is_err(), "{command:?}");
    }
    assert!(encode(ProtocolType::Ug3, &QueryInfo).is_err());
    assert!(
        UgProtocolFacade::new()
            .decode_inner(ProtocolType::Ug3, &[0; 8])
            .is_err()
    );
}

// ------------------------------------------------------------------ payloads

#[test]
fn query_info_payloads() {
    assert_eq!(
        ProtocolCommand::QueryInfo.payload(ProtocolType::Ug1),
        json!({
            "includeMaxPacketLength": true,
            "list": [
                "maxPacketLength", "battery", "firmwareVersion", "ANCType", "ANCDepth",
                "equalizer", "gameMode", "deviceName", "boxMac", "windNoise",
                "highQualityDecoding", "inBox", "connectedDeviceInfo", "controlInfo",
                "spatialSoundEffects", "deviceDualConnection", "promptSoundInfo"
            ]
        })
    );
    assert_eq!(
        ProtocolCommand::QueryInfo.payload(ProtocolType::Ug3),
        json!({
            "includeMaxPacketLength": false,
            "list": [
                "INFO_BATTERY", "INFO_VERSION", "INFO_BLE_NAME", "INFO_EQ", "INFO_BUTTONS",
                "INFO_GAME", "INFO_PROMPT", "INFO_ANC", "INFO_ANC_DEPTH", "INFO_SPATIAL",
                "INFO_DUAL", "INFO_HQ", "INFO_BOX_MAC", "INFO_WIND_NOISE", "INFO_WEAR",
                "INFO_AI", "INFO_IN_BOX", "INFO_CONNECTED", "INFO_CUSTOM_EQ",
                "INFO_TOUCH_SENSITIVITY"
            ]
        })
    );
}

// ---------------------------------------------------------------- UG1 decode
// All response frames below are synthetic (see module docs).

const UG1_INFO_RESPONSE: [u8; 34] = [
    0xDD, 0xEE, 0xFF, 0x04, 0x01, 0x1A, // header, GET_INFO, status ok, 26 bytes
    0xFF, 0x55, 0xFF, // battery: left n/a, right 85, box n/a
    0xB1, // anc: medium
    0x06, // eq: bass
    0x01, 0x00, 0x00, // dual link on, game off, high quality off
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [8..16] unused by decoder
    0x00, // [16] prompt language: English
    0x00, 0x00, // [17..19] unused by decoder
    0x09, // [19] prompt volume
    0x01, 0x00, // [20] spatial on, [21] wear off
    0x00, 0x00, // [22..24] unused by decoder
    0x00, 0x00, // [24] audio share off, [25] wind noise off
    0xC1, 0x1B, // crc
];

fn decode_ug1(bytes: &[u8]) -> (bool, String, serde_json::Value, Vec<u8>) {
    let frame = UgProtocolFacade::new()
        .decode_inner(ProtocolType::Ug1, bytes)
        .expect("decode failed");
    (frame.success, frame.command, frame.data, frame.raw)
}

#[test]
fn ug1_decode_device_state() {
    let (success, command, data, raw) = decode_ug1(&UG1_INFO_RESPONSE);
    assert!(success);
    assert_eq!(command, "GET_INFO");
    assert_eq!(raw, UG1_INFO_RESPONSE);
    assert_eq!(
        data,
        json!({
            "ancDepth": "medium",
            "ancMode": "adaptive",
            "audioShare": false,
            "battery": { "right": 85 },
            "crcValid": true,
            "dualLink": true,
            "eqMode": "bass",
            "gameMode": false,
            "highQuality": false,
            "promptLanguage": "English",
            "promptVolume": 9,
            "soundType": 0,
            "spatialAudio": true,
            "success": true,
            "wearDetection": false,
            "windNoise": false
        })
    );
}

#[test]
fn ug1_decode_simple_responses() {
    let cases: [(&[u8], &str, serde_json::Value); 5] = [
        (
            &[
                0xDD, 0xEE, 0xFF, 0x01, 0x01, 0x06, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x2A, 0x01,
            ],
            "GET_VERSION",
            json!({ "crcValid": true, "firmwareVersion": "1.0.7", "success": true }),
        ),
        (
            &[0xDD, 0xEE, 0xFF, 0x09, 0x01, 0x01, 0xD2, 0x89, 0x98],
            "ANC_MODE",
            json!({
                "ancDepth": "adaptive", "ancMode": "transparent",
                "crcValid": true, "success": true
            }),
        ),
        (
            &[0xDD, 0xEE, 0xFF, 0x05, 0x01, 0x01, 0x06, 0x42, 0x5C],
            "EQ_SET",
            json!({ "crcValid": true, "eqMode": "bass", "success": true }),
        ),
        (
            &[0xDD, 0xEE, 0xFF, 0x0C, 0x01, 0x01, 0x00, 0xF3, 0xCF],
            "PROMPT_LANG",
            json!({ "crcValid": true, "promptLanguage": "English", "success": true }),
        ),
        (
            &[0xDD, 0xEE, 0xFF, 0x11, 0x01, 0x01, 0x09, 0xFB, 0x7C],
            "PROMPT_VOL",
            json!({ "crcValid": true, "promptVolume": 9, "success": true }),
        ),
    ];
    for (bytes, expected_command, expected_data) in cases {
        let (success, command, data, _) = decode_ug1(bytes);
        assert!(success, "{expected_command}");
        assert_eq!(command, expected_command);
        assert_eq!(data, expected_data);
    }
}

#[test]
fn ug1_decode_status_zero_is_failure() {
    let (success, command, data, _) =
        decode_ug1(&[0xDD, 0xEE, 0xFF, 0x05, 0x00, 0x01, 0x06, 0x72, 0x6B]);
    assert!(!success);
    assert_eq!(command, "EQ_SET");
    assert_eq!(
        data,
        json!({ "crcValid": true, "eqMode": "bass", "success": false })
    );
}

#[test]
fn ug1_decode_bad_crc_is_reported_not_rejected() {
    // Quirk: a CRC mismatch only flips `crcValid`; the frame is still decoded.
    let (success, _, data, _) = decode_ug1(&[0xDD, 0xEE, 0xFF, 0x05, 0x01, 0x01, 0x06, 0x00, 0x00]);
    assert!(success);
    assert_eq!(data["crcValid"], false);

    // The MODBUS checksum used for requests is not valid on a response.
    let (_, _, data, _) = decode_ug1(&[0xDD, 0xEE, 0xFF, 0x05, 0x01, 0x01, 0x06, 0xD0, 0xBA]);
    assert_eq!(data["crcValid"], false);
}

#[test]
fn ug1_decode_hardware_reports() {
    let (success, command, data, raw) = decode_ug1(&[0x85, 0x86, 0x87, 0x00, 0x02, 0xD2]);
    assert!(success);
    assert_eq!(command, "HW_REPORT_02");
    assert_eq!(raw, [0x85, 0x86, 0x87, 0x00, 0x02, 0xD2]);
    assert_eq!(
        data,
        json!({ "ancDepth": "adaptive", "ancMode": "transparent", "noiseMode": 210 })
    );

    let (_, command, data, _) = decode_ug1(&[0x85, 0x86, 0x87, 0x00, 0x01, 0x5A, 0xFF, 0xFF]);
    assert_eq!(command, "HW_REPORT_01");
    assert_eq!(data, json!({ "battery": { "left": 90 } }));
}

#[test]
fn ug1_decode_rejects_garbage() {
    let facade = UgProtocolFacade::new();
    assert!(facade.decode_inner(ProtocolType::Ug1, &[]).is_err());
    assert!(
        facade
            .decode_inner(ProtocolType::Ug1, &[0xAA, 0xBB, 0xCC, 4, 1, 0, 0x31, 0x91])
            .is_err()
    );
}

// ---------------------------------------------------------------- UG2 decode

#[test]
fn ug2_decode_device_info() {
    let frame = UgProtocolFacade::new()
        .decode_inner(
            ProtocolType::Ug2,
            &[
                39, 1, 0, 13, // GET_INFO, 13 payload bytes
                1, 3, 80, 81, 90, // battery
                2, 3, 1, 0, 7, // firmware
                12, 1, 2, // anc type
            ],
        )
        .unwrap();
    assert!(frame.success);
    assert_eq!(frame.command, "GET_INFO");
    assert_eq!(
        frame.data,
        json!({
            "ancMode": "transparent",
            "battery": { "box": 90, "left": 80, "right": 81 },
            "firmwareVersion": "1.0.7",
            "success": true
        })
    );
}

// ---------------------------------------------------------- stream reassembly

#[test]
fn ug1_frame_is_found_inside_a_stream() {
    let mut stream = vec![0x00, 0x11];
    stream.extend_from_slice(&UG1_INFO_RESPONSE);
    stream.extend_from_slice(&[0xDD, 0xEE]);
    assert_eq!(
        find_complete_ug1_frame(&stream).as_deref(),
        Some(&UG1_INFO_RESPONSE[..])
    );
}

#[test]
fn ug1_partial_frame_is_not_returned() {
    assert_eq!(find_complete_ug1_frame(&[]), None);
    assert_eq!(find_complete_ug1_frame(&UG1_INFO_RESPONSE[..7]), None);
    assert_eq!(find_complete_ug1_frame(&UG1_INFO_RESPONSE[..20]), None);
}

#[test]
fn ug1_hardware_frame_in_stream() {
    // Quirk: nothing is recognised until at least 8 bytes are buffered, so a
    // lone 6-byte hardware report is not returned on its own.
    assert_eq!(
        find_complete_ug1_frame(&[0x85, 0x86, 0x87, 0x00, 0x02, 0xD2]),
        None
    );
    assert_eq!(
        find_complete_ug1_frame(&[0x85, 0x86, 0x87, 0x00, 0x02, 0xD2, 0xDD, 0xEE]).as_deref(),
        Some(&[0x85, 0x86, 0x87, 0x00, 0x02, 0xD2][..])
    );
}

// ----------------------------------------------------------------------- RCSP

const RCSP_COMMAND: [u8; 16] = [
    0xFE, 0xDC, 0xBA, 0xC0, 0xFF, 0x00, 0x08, 0x05, 0xFF, 0x27, 0x01, 0x00, 0x02, 0x01, 0x00, 0xEF,
];

#[test]
fn rcsp_wrap() {
    assert_eq!(
        wrap_rcsp_custom(&[0x27, 0x01, 0x00, 0x02, 0x01, 0x00], 5),
        RCSP_COMMAND
    );
}

#[test]
fn rcsp_unwrap_command_and_response() {
    assert_eq!(
        unwrap_rcsp_custom(&RCSP_COMMAND).unwrap(),
        [0x27, 0x01, 0x00, 0x02, 0x01, 0x00]
    );
    // Response framing (flag bit 7 clear): [status, seq, 0xFF, inner...].
    let response = [
        0xFE, 0xDC, 0xBA, 0x00, 0xFF, 0x00, 0x05, 0x00, 0x05, 0xFF, 0xAB, 0xCD, 0xEF,
    ];
    assert_eq!(unwrap_rcsp_custom(&response).unwrap(), [0xAB, 0xCD]);
}

#[test]
fn rcsp_unwrap_rejects_bad_frames() {
    assert!(unwrap_rcsp_custom(&RCSP_COMMAND[..8]).is_err());
    let mut bad_tail = RCSP_COMMAND;
    bad_tail[15] = 0x00;
    assert!(unwrap_rcsp_custom(&bad_tail).is_err());
    let mut bad_opcode = RCSP_COMMAND;
    bad_opcode[4] = 0x01;
    assert!(unwrap_rcsp_custom(&bad_opcode).is_err());
    let mut bad_length = RCSP_COMMAND;
    bad_length[6] = 0x09;
    assert!(unwrap_rcsp_custom(&bad_length).is_err());
}

#[test]
fn rcsp_frame_is_found_inside_a_stream() {
    let mut stream = vec![0x42];
    stream.extend_from_slice(&RCSP_COMMAND);
    stream.push(0x99);
    assert_eq!(
        find_complete_rcsp_frame(&stream).as_deref(),
        Some(&RCSP_COMMAND[..])
    );
    assert_eq!(find_complete_rcsp_frame(&RCSP_COMMAND[..15]), None);
}
