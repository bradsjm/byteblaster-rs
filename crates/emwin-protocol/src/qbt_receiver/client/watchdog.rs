//! Watchdog health monitoring for EMWIN connections.
//!
//! This module provides a watchdog that monitors connection health
//! based on data reception and exception counts, triggering connection
//! closure when thresholds are exceeded.
//!
//! ## Health Metrics
//!
//! The watchdog monitors two primary health indicators:
//! - **Data reception timeout**: If no data is received within the configured
//!   timeout duration, the connection is considered unhealthy
//! - **Exception count**: If too many consecutive exceptions/errors occur,
//!   the connection is considered unhealthy
//!
//! ## Usage Pattern
//!
//! The watchdog is integrated into the client runtime's read loop:
//! 1. Create a `Watchdog` with timeout and max exceptions from config
//! 2. Call `on_data_received` each time data is successfully read
//! 3. Call `on_exception` each time an error occurs during processing
//! 4. Periodically check `should_close` or `should_close_at` to determine
//!    if the connection should be terminated due to health issues
//!
//! When the watchdog signals that the connection should close, the client
//! runtime triggers a reconnection cycle, allowing the system to recover
//! from transient failures.
//!
//! ## Trait Implementation
//!
//! The [`HealthObserver`] trait allows the watchdog to be used generically
//! with any type that needs to report health events. The [`Watchdog`]
//! struct implements this trait and can be used directly or wrapped in
//! other types that need health monitoring.
//!
//! ## Configuration
//!
//! Watchdog behavior is controlled by two parameters:
//! - `timeout_secs`: Maximum time without data reception (minimum 1 second)
//! - `max_exceptions`: Maximum number of consecutive exceptions allowed
//!
//! These values come from `QbtReceiverConfig.watchdog_timeout_secs` and
//! `QbtReceiverConfig.max_exceptions` fields.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::time::{Duration, Instant};

/// Trait for health observation.
///
/// Implementors can track connection health by receiving notifications
/// about data reception and exceptions.
pub trait HealthObserver: Send + Sync {
    /// Called when data is successfully received.
    fn on_data_received(&self);
    /// Called when an exception/error occurs.
    fn on_exception(&self);
    /// Returns true if the connection should be closed due to health issues.
    fn should_close(&self) -> bool;
}

/// Connection health watchdog.
///
/// Monitors connection health based on:
/// - Time since last data reception (timeout)
/// - Number of consecutive exceptions
///
/// If either threshold is exceeded, the watchdog signals that the
/// connection should be closed.
#[derive(Debug)]
pub struct Watchdog {
    /// Timeout duration for data reception.
    timeout: Duration,
    /// Maximum allowed consecutive exceptions.
    max_exceptions: u32,
    /// Current exception count.
    exception_count: AtomicU32,
    /// Timestamp of last data reception.
    last_data: Mutex<Instant>,
}

impl Watchdog {
    /// Creates a new watchdog with the given parameters.
    ///
    /// # Arguments
    ///
    /// * `timeout_secs` - Timeout in seconds (minimum 1)
    /// * `max_exceptions` - Maximum allowed consecutive exceptions
    pub fn new(timeout_secs: u64, max_exceptions: u32) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs.max(1)),
            max_exceptions,
            exception_count: AtomicU32::new(0),
            last_data: Mutex::new(Instant::now()),
        }
    }

    /// Checks if the connection should be closed at the given time.
    ///
    /// # Arguments
    ///
    /// * `now` - The current instant to check against
    ///
    /// # Returns
    ///
    /// `true` if the connection should be closed
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
