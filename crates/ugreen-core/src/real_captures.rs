//! Decoder tests against frames captured from real hardware.
//!
//! Source: UGREEN HiTune Max5c, firmware 0.5.7, Windows RFCOMM channel 1
//! (SDP service `JL_SPP`), captured with `ugreen-cli --verbose`. Unlike
//! `golden_tests.rs`, nothing here is synthetic.
//!
//! What the *fields mean* is still the decoder's interpretation; these tests
//! only pin how the decoder reads bytes the headset really sent.
//!
//! These captures showed that the trailing checksum of a response is
//! CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF) over `[cmd, status, len,
//! payload]`, stored low byte first, not the CRC-16/MODBUS used on requests.
//! The decoder's `crcValid` was corrected accordingly.

use crate::protocol::{ProtocolFacade, ProtocolType, UgProtocolFacade, find_complete_ug1_frame};

/// Response to `QueryFirmware` (`AA BB CC 01 01 00 21 90`).
const FIRMWARE_RESPONSE: [u8; 14] = [
    0xDD, 0xEE, 0xFF, 0x01, 0x01, 0x06, 0x00, 0x05, 0x07, 0x00, 0x00, 0x00, 0xDD, 0x67,
];

/// Response to `QueryInfo` (`AA BB CC 04 01 00 31 91`). The state payload is
/// 22 bytes on this firmware, shorter than the 26 the decoder can read.
const INFO_RESPONSE: [u8; 30] = [
    0xDD, 0xEE, 0xFF, 0x04, 0x01, 0x16, 0x5A, 0xFF, 0xFF, 0xA0, 0x07, 0x00, 0x00, 0x01, 0x08, 0x00,
    0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x95, 0xE3,
];

#[test]
fn firmware_response() {
    assert_eq!(
        find_complete_ug1_frame(&FIRMWARE_RESPONSE).as_deref(),
        Some(&FIRMWARE_RESPONSE[..])
    );
    let frame = UgProtocolFacade::new()
        .decode_inner(ProtocolType::Ug1, &FIRMWARE_RESPONSE)
        .unwrap();
    assert!(frame.success);
    assert_eq!(frame.command, "GET_VERSION");
    assert_eq!(frame.data["crcValid"], true);
    assert_eq!(frame.data["firmwareVersion"], "0.5.7");
}

#[test]
fn info_response() {
    assert_eq!(
        find_complete_ug1_frame(&INFO_RESPONSE).as_deref(),
        Some(&INFO_RESPONSE[..])
    );
    let frame = UgProtocolFacade::new()
        .decode_inner(ProtocolType::Ug1, &INFO_RESPONSE)
        .unwrap();
    assert!(frame.success);
    assert_eq!(frame.command, "GET_INFO");
    let data = &frame.data;
    assert_eq!(data["crcValid"], true);
    // Over-ear headset: a single battery value, reported in the `left` slot.
    assert_eq!(data["battery"], serde_json::json!({ "left": 90 }));
    assert_eq!(data["ancMode"], "off");
    assert_eq!(data["ancDepth"], "deep");
    assert_eq!(data["eqMode"], "treble");
    assert_eq!(data["dualLink"], false);
    assert_eq!(data["gameMode"], false);
    assert_eq!(data["highQuality"], true);
    assert_eq!(data["promptLanguage"], "English");
    assert_eq!(data["promptVolume"], 9);
    assert_eq!(data["spatialAudio"], false);
    assert_eq!(data["wearDetection"], false);
}

fn crc16_ccitt_false(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[test]
fn response_checksum_is_crc16_ccitt_false_little_endian() {
    assert_eq!(
        crc16_ccitt_false(b"123456789"),
        0x29B1,
        "standard check value"
    );
    for frame in [&FIRMWARE_RESPONSE[..], &INFO_RESPONSE[..]] {
        let (body, checksum) = frame[3..].split_at(frame.len() - 5);
        assert_eq!(
            crc16_ccitt_false(body),
            u16::from_le_bytes([checksum[0], checksum[1]])
        );
    }
}

#[test]
fn incomplete_responses_are_never_returned_as_frames() {
    // While a response is still arriving, no prefix of it may be mistaken for
    // a complete frame (the reassembler falls back to a checksum scan when
    // the declared length is not buffered yet).
    for frame in [&FIRMWARE_RESPONSE[..], &INFO_RESPONSE[..]] {
        for len in 0..frame.len() {
            assert_eq!(find_complete_ug1_frame(&frame[..len]), None, "prefix {len}");
        }
    }
}

#[test]
fn checksum_scan_recovers_a_frame_whose_length_byte_overstates() {
    // SYNTHETIC: the real firmware response with its length byte changed from
    // 6 to 9 and the checksum recomputed. The declared size never arrives, so
    // the frame can only be delimited by its CRC-16/CCITT-FALSE checksum.
    let mut frame = FIRMWARE_RESPONSE;
    frame[5] = 9;
    let checksum = crc16_ccitt_false(&frame[3..12]).to_le_bytes();
    frame[12..].copy_from_slice(&checksum);
    assert_eq!(find_complete_ug1_frame(&frame).as_deref(), Some(&frame[..]));
}
