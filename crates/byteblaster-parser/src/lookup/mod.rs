//! Product Identifier List (PIL) lookup functionality.
//!
//! Provides a lookup table of meteorological product type descriptions indexed by PIL prefix.

mod pil_generated;

pub use pil_generated::{
    PIL_ENTRY_COUNT, PIL_GENERATED_AT_UTC, PIL_SOURCE_COMMIT, PIL_SOURCE_PATH, PIL_SOURCE_REPO,
};

/// Looks up a product type description by PIL prefix.
///
/// # Arguments
///
/// * `nnn` - PIL prefix (first 3 characters of AFOS PIL)
///
/// # Returns
///
/// Product type description as a static string slice, or `None` if the PIL prefix is unknown
///
/// # Example
///
/// ```
/// use byteblaster_parser::pil_description;
///
/// assert_eq!(pil_description("AFD"), Some("Area Forecast Discussion"));
/// assert_eq!(pil_description("SVR"), Some("Severe Thunderstorm Warning"));
/// assert_eq!(pil_description("TOR"), Some("Tornado Warning"));
/// assert_eq!(pil_description("ZZZ"), None);
/// ```
pub fn pil_description(nnn: &str) -> Option<&'static str> {
    let key = nnn.trim().to_ascii_uppercase();
    if key.len() != 3 {
        return None;
    }
    pil_generated::PIL_DESCRIPTIONS
        .binary_search_by_key(&key.as_str(), |(candidate, _)| candidate)
        .ok()
        .map(|index| pil_generated::PIL_DESCRIPTIONS[index].1)
}

#[cfg(test)]
mod tests {
    use super::pil_description;

    #[test]
    fn known_entries_are_found_case_insensitively() {
        assert!(pil_description("AFD").is_some());
        assert_eq!(pil_description("afd"), pil_description("AFD"));
    }

    #[test]
    fn unknown_or_invalid_entries_are_none() {
        assert_eq!(pil_description("ZZZ"), None);
        assert_eq!(pil_description("TO"), None);
        assert_eq!(pil_description("TOOO"), None);
    }
}
