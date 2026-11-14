//! Bluetooth address conversions. Pure code, compiled and tested on every OS.

use anyhow::{Result, anyhow};

/// Parses `AA:BB:CC:DD:EE:FF` (or with `-`) into bytes in the order written,
/// i.e. `[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]`.
pub fn parse_mac(value: &str) -> Result<[u8; 6]> {
    let invalid = || anyhow!("invalid Bluetooth address `{value}` (expected AA:BB:CC:DD:EE:FF)");

    let parts = value.trim().split([':', '-']).collect::<Vec<_>>();
    if parts.len() != 6 {
        return Err(invalid());
    }

    let mut bytes = [0u8; 6];
    for (byte, part) in bytes.iter_mut().zip(parts) {
        if part.len() != 2 || !part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        *byte = u8::from_str_radix(part, 16).map_err(|_| invalid())?;
    }
    Ok(bytes)
}

/// Converts `AA:BB:CC:DD:EE:FF` to a Windows `BTH_ADDR`: a `u64` whose low 48
/// bits hold the address with `AA` as the most significant byte
/// (`0x0000_AABB_CCDD_EEFF`).
pub fn mac_to_bth_addr(value: &str) -> Result<u64> {
    Ok(parse_mac(value)?
        .iter()
        .fold(0u64, |addr, byte| (addr << 8) | u64::from(*byte)))
}

/// Inverse of [`mac_to_bth_addr`]. Bits above the low 48 are ignored.
pub fn bth_addr_to_mac(addr: u64) -> String {
    let bytes = addr.to_be_bytes();
    bytes[2..]
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colon_and_dash_forms() {
        let expected = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        assert_eq!(parse_mac("AA:BB:CC:DD:EE:FF").unwrap(), expected);
        assert_eq!(parse_mac("aa-bb-cc-dd-ee-ff").unwrap(), expected);
        assert_eq!(parse_mac(" AA:BB:CC:DD:EE:FF\n").unwrap(), expected);
    }

    #[test]
    fn rejects_malformed_addresses() {
        for bad in [
            "",
            "AA:BB:CC:DD:EE",
            "AA:BB:CC:DD:EE:FF:00",
            "AA:BB:CC:DD:EE:GG",
            "A:BB:CC:DD:EE:FF",
            "AAA:BB:CC:DD:EE:F",
            "+A:BB:CC:DD:EE:FF",
            "AABBCCDDEEFF",
            "dev-max6",
        ] {
            assert!(parse_mac(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn bth_addr_has_first_octet_most_significant() {
        assert_eq!(
            mac_to_bth_addr("AA:BB:CC:DD:EE:FF").unwrap(),
            0x0000_AABB_CCDD_EEFF
        );
        assert_eq!(
            mac_to_bth_addr("3C:78:AD:8D:E2:EA").unwrap(),
            0x0000_3C78_AD8D_E2EA
        );
        assert_eq!(mac_to_bth_addr("00:00:00:00:00:01").unwrap(), 1);
    }

    #[test]
    fn bth_addr_round_trips() {
        for mac in [
            "AA:BB:CC:DD:EE:FF",
            "00:01:02:03:04:05",
            "3C:78:AD:8D:E2:EA",
        ] {
            assert_eq!(bth_addr_to_mac(mac_to_bth_addr(mac).unwrap()), mac);
        }
        assert_eq!(bth_addr_to_mac(0xFFFF_0000_0000_0001), "00:00:00:00:00:01");
    }
}
