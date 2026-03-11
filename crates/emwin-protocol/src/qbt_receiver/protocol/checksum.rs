//! Calculate and verify the additive checksum used by QBT frames.

/// Calculates the 16-bit additive checksum used on the wire.
pub fn calculate_qbt_checksum(data: &[u8]) -> u16 {
    (data.iter().map(|v| *v as u32).sum::<u32>() & 0xFFFF) as u16
}

/// Verifies a frame checksum against the protocol's 16-bit comparison rules.
///
/// Negative expected values are rejected immediately. For V1 compatibility only the low 16 bits
/// participate in the comparison.
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
