//! Checksum calculation and verification for EMWIN protocol.
//!
//! This module provides simple checksum utilities used for data integrity
//! validation in both V1 and V2 protocol frames.

/// Calculates a 16-bit checksum by summing all bytes and masking to 16 bits.
///
/// This is a simple additive checksum used by the EMWIN protocol
/// for basic data integrity verification.
///
/// # Arguments
///
/// * `data` - The byte slice to checksum
///
/// # Returns
///
/// A 16-bit checksum value
pub fn calculate_qbt_checksum(data: &[u8]) -> u16 {
    (data.iter().map(|v| *v as u32).sum::<u32>() & 0xFFFF) as u16
}

/// Verifies that the calculated checksum matches the expected value.
///
/// # Arguments
///
/// * `data` - The byte slice to verify
/// * `expected` - The expected checksum value (can be larger than 16 bits for V1 protocol)
///
/// # Returns
///
/// `true` if the checksum matches, `false` otherwise
///
/// # Notes
///
/// Negative expected values are always considered invalid.
/// For V1 protocol, only the lower 16 bits of the expected value are used.
pub fn verify_checksum(data: &[u8], expected: i64) -> bool {
    if expected < 0 {
        return false;
    }
    calculate_qbt_checksum(data) as i64 == (expected & 0xFFFF)
}

#[cfg(test)]
mod tests {
    use super::{calculate_qbt_checksum, verify_checksum};

    #[test]
    fn checksum_matches_reference() {
        let data = [1u8, 2, 3, 4, 255];
        let expected = ((1u32 + 2 + 3 + 4 + 255) & 0xFFFF) as u16;
        assert_eq!(calculate_qbt_checksum(&data), expected);
        assert!(verify_checksum(&data, i64::from(expected)));
    }

    #[test]
    fn checksum_negative_expected_is_invalid() {
        let data = [1u8, 2, 3];
        assert!(!verify_checksum(&data, -1));
    }
}
