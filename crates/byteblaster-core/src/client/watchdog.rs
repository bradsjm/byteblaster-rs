use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::time::{Duration, Instant};

pub trait HealthObserver: Send + Sync {
    fn on_data_received(&self);
    fn on_exception(&self);
    fn should_close(&self) -> bool;
}

#[derive(Debug)]
pub struct Watchdog {
    timeout: Duration,
    max_exceptions: u32,
    exception_count: AtomicU32,
    last_data: Mutex<Instant>,
}

impl Watchdog {
    pub fn new(timeout_secs: u64, max_exceptions: u32) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs.max(1)),
            max_exceptions,
            exception_count: AtomicU32::new(0),
            last_data: Mutex::new(Instant::now()),
        }
    }

    pub fn should_close_at(&self, now: Instant) -> bool {
        let exceptions = self.exception_count.load(Ordering::Relaxed);
        if exceptions > self.max_exceptions {
            return true;
        }

        let last = self
            .last_data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        now.duration_since(*last) > self.timeout
    }
}

impl HealthObserver for Watchdog {
    fn on_data_received(&self) {
        self.exception_count.store(0, Ordering::Relaxed);
        let mut last = self
            .last_data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *last = Instant::now();
    }

    fn on_exception(&self) {
        self.exception_count.fetch_add(1, Ordering::Relaxed);
    }

    fn should_close(&self) -> bool {
        self.should_close_at(Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::{HealthObserver, Watchdog};
    use tokio::time::{Duration, Instant};

    #[test]
    fn watchdog_timeout_trigger() {
        let w = Watchdog::new(2, 10);
        let now = Instant::now();
        assert!(!w.should_close_at(now + Duration::from_secs(1)));
        assert!(w.should_close_at(now + Duration::from_secs(3)));
    }

    #[test]
    fn watchdog_resets_on_data() {
        let w = Watchdog::new(2, 10);
        w.on_data_received();
        let now = Instant::now();
        assert!(!w.should_close_at(now + Duration::from_secs(1)));
        assert!(w.should_close_at(now + Duration::from_secs(3)));
    }

    #[test]
    fn watchdog_exception_limit() {
        let w = Watchdog::new(100, 2);
        w.on_exception();
        w.on_exception();
        assert!(!w.should_close_at(Instant::now()));
        w.on_exception();
        assert!(w.should_close_at(Instant::now()));
    }
}
