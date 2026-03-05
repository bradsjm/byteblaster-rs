//! Endpoint rotation and backoff utilities for reconnection logic.
//!
//! This module provides utilities for rotating through server endpoints
//! and calculating exponential backoff delays.

/// Rotates through a list of endpoints in round-robin fashion.
#[derive(Debug, Clone)]
pub struct EndpointRotator {
    endpoints: Vec<(String, u16)>,
    index: usize,
}

impl EndpointRotator {
    /// Creates a new rotator with the given endpoints.
    pub fn new(endpoints: Vec<(String, u16)>) -> Self {
        Self {
            endpoints,
            index: 0,
        }
    }

    /// Resets the rotation to the first endpoint.
    pub fn reset(&mut self) {
        self.index = 0;
    }
}

impl Iterator for EndpointRotator {
    type Item = (String, u16);

    fn next(&mut self) -> Option<Self::Item> {
        if self.endpoints.is_empty() {
            return None;
        }
        let out = self.endpoints[self.index].clone();
        self.index = (self.index + 1) % self.endpoints.len();
        Some(out)
    }
}

/// Calculates the next backoff delay in seconds.
///
/// Uses exponential backoff with a cap at 60 seconds.
///
/// # Arguments
///
/// * `base` - Base delay in seconds
/// * `failures` - Number of consecutive failures
///
/// # Returns
///
/// The calculated backoff delay in seconds
///
/// # Formula
///
/// `min(base * 2^min(failures, 6), 60)`
pub fn next_backoff_secs(base: u64, failures: u32) -> u64 {
    let capped = failures.min(6);
    let factor = 1u64 << capped;
    (base.max(1).saturating_mul(factor)).min(60)
}

#[cfg(test)]
mod tests {
    use super::{EndpointRotator, next_backoff_secs};

    #[test]
    fn reconnect_backoff_logic() {
        assert_eq!(next_backoff_secs(1, 0), 1);
        assert_eq!(next_backoff_secs(1, 1), 2);
        assert_eq!(next_backoff_secs(1, 2), 4);
        assert_eq!(next_backoff_secs(2, 3), 16);
        assert_eq!(next_backoff_secs(5, 10), 60);
    }

    #[test]
    fn rotator_cycles() {
        let mut r = EndpointRotator::new(vec![
            ("a".to_string(), 1),
            ("b".to_string(), 2),
            ("c".to_string(), 3),
        ]);
        assert_eq!(r.next(), Some(("a".to_string(), 1)));
        assert_eq!(r.next(), Some(("b".to_string(), 2)));
        assert_eq!(r.next(), Some(("c".to_string(), 3)));
        assert_eq!(r.next(), Some(("a".to_string(), 1)));
    }
}
