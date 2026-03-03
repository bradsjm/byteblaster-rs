#[derive(Debug, Clone)]
pub struct EndpointRotator {
    endpoints: Vec<(String, u16)>,
    index: usize,
}

impl EndpointRotator {
    pub fn new(endpoints: Vec<(String, u16)>) -> Self {
        Self {
            endpoints,
            index: 0,
        }
    }

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
