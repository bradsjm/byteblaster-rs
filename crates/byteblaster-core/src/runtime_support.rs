use futures::{Stream, stream};
use std::pin::Pin;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};

pub(crate) type ReceiverEventStream<TEvent, TError> =
    Pin<Box<dyn Stream<Item = Result<TEvent, TError>> + Send + 'static>>;

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
    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackpressureTracker {
    event_queue_drop_total: u64,
    dropped_since_last_report: u64,
}

impl BackpressureTracker {
    #[cfg(test)]
    pub(crate) fn new(event_queue_drop_total: u64, dropped_since_last_report: u64) -> Self {
        Self {
            event_queue_drop_total,
            dropped_since_last_report,
        }
    }

    pub(crate) fn event_queue_drop_total(&self) -> u64 {
        self.event_queue_drop_total
    }

    pub(crate) fn dropped_since_last_report(&self) -> u64 {
        self.dropped_since_last_report
    }

    pub(crate) fn has_pending_report(&self) -> bool {
        self.dropped_since_last_report > 0
    }

    pub(crate) fn clear_pending_report(&mut self) {
        self.dropped_since_last_report = 0;
    }

    pub(crate) fn record_send_failure<T>(&mut self, err: TrySendError<T>) {
        if matches!(err, TrySendError::Full(_)) {
            self.event_queue_drop_total = self.event_queue_drop_total.saturating_add(1);
            self.dropped_since_last_report = self.dropped_since_last_report.saturating_add(1);
        }
    }
}
