//! Shared runtime infrastructure for receiver implementations.
//!
//! This module provides common utilities used by both QBT and Weather Wire receivers,
//! including the runtime lifecycle management, event streaming, and backpressure tracking.

use futures::{Stream, stream};
use std::pin::Pin;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};

/// Type alias for receiver event streams.
///
/// This is a pinned boxed stream that yields results of events or errors.
pub(crate) type ReceiverEventStream<TEvent, TError> =
    Pin<Box<dyn Stream<Item = Result<TEvent, TError>> + Send + 'static>>;

/// Runtime state management for receiver implementations.
///
/// Tracks whether the receiver is running, holds the event channel receiver,
/// shutdown signal sender, and the background task join handle.
#[derive(Debug)]
pub(crate) struct ReceiverRuntime<TEvent, TError> {
    running: bool,
    event_rx: Option<mpsc::Receiver<Result<TEvent, TError>>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    join_handle: Option<tokio::task::JoinHandle<()>>,
}

impl<TEvent, TError> Default for ReceiverRuntime<TEvent, TError> {
    fn default() -> Self {
        Self {
            running: false,
            event_rx: None,
            shutdown_tx: None,
            join_handle: None,
        }
    }
}

impl<TEvent, TError> ReceiverRuntime<TEvent, TError> {
    /// Returns whether the receiver is currently running.
    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    /// Installs the runtime components.
    ///
    /// Takes ownership of the event channel receiver, shutdown signal sender,
    /// and background task join handle, marking the runtime as running.
    pub(crate) fn install(
        &mut self,
        event_rx: mpsc::Receiver<Result<TEvent, TError>>,
        shutdown_tx: watch::Sender<bool>,
        join_handle: tokio::task::JoinHandle<()>,
    ) {
        self.event_rx = Some(event_rx);
        self.shutdown_tx = Some(shutdown_tx);
        self.join_handle = Some(join_handle);
        self.running = true;
    }

    /// Stops the receiver runtime gracefully.
    ///
    /// Sends shutdown signal, awaits the background task completion, and
    /// cleans up runtime state.
    pub(crate) async fn stop(&mut self) {
        if !self.running {
            return;
        }

        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(true);
        }

        if let Some(handle) = self.join_handle.take() {
            let _ = handle.await;
        }

        self.running = false;
        self.shutdown_tx = None;
    }

    /// Takes the event stream, returning an error if already taken.
    ///
    /// Converts the event receiver into a pinned stream for consumption.
    pub(crate) fn take_events(
        &mut self,
        already_taken_error: TError,
    ) -> Result<ReceiverEventStream<TEvent, TError>, TError>
    where
        TEvent: Send + 'static,
        TError: Send + 'static,
    {
        let rx = self.event_rx.take().ok_or(already_taken_error)?;
        Ok(Box::pin(stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        })))
    }
}

/// Tracks backpressure events for event queue overflow.
///
/// Maintains counters for total dropped events and events since the last
/// warning report was emitted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackpressureTracker {
    event_queue_drop_total: u64,
    dropped_since_last_report: u64,
}

impl BackpressureTracker {
    /// Creates a new tracker with specified initial values (test-only).
    #[cfg(test)]
    pub(crate) fn new(event_queue_drop_total: u64, dropped_since_last_report: u64) -> Self {
        Self {
            event_queue_drop_total,
            dropped_since_last_report,
        }
    }

    /// Returns the total number of events dropped due to queue overflow.
    pub(crate) fn event_queue_drop_total(&self) -> u64 {
        self.event_queue_drop_total
    }

    /// Returns the number of events dropped since the last warning report.
    pub(crate) fn dropped_since_last_report(&self) -> u64 {
        self.dropped_since_last_report
    }

    /// Returns true if there are drops pending a warning report.
    pub(crate) fn has_pending_report(&self) -> bool {
        self.dropped_since_last_report > 0
    }

    /// Clears the pending report counter after emitting a warning.
    pub(crate) fn clear_pending_report(&mut self) {
        self.dropped_since_last_report = 0;
    }

    /// Records a send failure, incrementing counters if due to a full queue.
    pub(crate) fn record_send_failure<T>(&mut self, err: TrySendError<T>) {
        if matches!(err, TrySendError::Full(_)) {
            self.event_queue_drop_total = self.event_queue_drop_total.saturating_add(1);
            self.dropped_since_last_report = self.dropped_since_last_report.saturating_add(1);
        }
    }
}

/// Attempts to send an event, emitting backpressure warnings if needed.
///
/// If there are pending dropped events, first attempts to send a warning
/// event before sending the actual event. Handles queue full conditions
/// by recording the failure in the tracker and calling the on_drop callback.
///
/// # Type Parameters
///
/// * `TEvent` - The event type
/// * `TError` - The error type
/// * `FBuildWarning` - Function to build a warning event from tracker state
/// * `FOnWarningSent` - Callback when warning is successfully sent
/// * `FOnDrop` - Callback when events are dropped due to full queue
///
/// # Arguments
///
/// * `event_tx` - The event channel sender
/// * `event` - The event to send
/// * `tracker` - Backpressure tracker for drop counting
/// * `build_warning` - Function to create warning event
/// * `on_warning_sent` - Callback after warning sent
/// * `on_drop` - Callback when events are dropped
pub(crate) fn try_send_with_backpressure_warning<
    TEvent,
    TError,
    FBuildWarning,
    FOnWarningSent,
    FOnDrop,
>(
    event_tx: &mpsc::Sender<Result<TEvent, TError>>,
    event: Result<TEvent, TError>,
    tracker: &mut BackpressureTracker,
    build_warning: FBuildWarning,
    on_warning_sent: FOnWarningSent,
    on_drop: FOnDrop,
) where
    FBuildWarning: FnOnce(&BackpressureTracker) -> TEvent,
    FOnWarningSent: FnOnce(),
    FOnDrop: FnMut(&BackpressureTracker),
{
    let mut on_drop = on_drop;
    if tracker.has_pending_report() {
        let warning = build_warning(tracker);
        match event_tx.try_send(Ok(warning)) {
            Ok(()) => {
                tracker.clear_pending_report();
                on_warning_sent();
            }
            Err(err) => {
                tracker.record_send_failure(err);
                on_drop(tracker);
            }
        }
    }

    if let Err(err) = event_tx.try_send(event) {
        tracker.record_send_failure(err);
        on_drop(tracker);
    }
}
