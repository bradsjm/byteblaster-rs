//! QbtReceiver runtime for ByteBlaster protocol connections.
//!
//! This module provides a full-featured client implementation with:
//! - Connection management with timeout and retry
//! - Automatic reconnection with endpoint rotation
//! - Authentication heartbeat
//! - Watchdog health monitoring
//! - Event streaming with backpressure handling
//! - Server list persistence and management

pub mod connection;
pub mod reconnect;
pub mod server_list_manager;
pub mod watchdog;

use crate::qbt_receiver::config::QbtReceiverConfig;
use crate::qbt_receiver::error::{QbtReceiverError, QbtReceiverResult};
use crate::qbt_receiver::protocol::auth::{REAUTH_INTERVAL_SECS, build_logon_message, xor_ff};
use crate::qbt_receiver::protocol::codec::{QbtFrameDecoder, QbtProtocolDecoder};
use crate::qbt_receiver::protocol::model::QbtFrameEvent;
use crate::qbt_receiver::protocol::model::{QbtAuthMessage, QbtProtocolWarning};
use crate::runtime_support::{
    BackpressureTracker, ReceiverEventStream, ReceiverRuntime, try_send_with_backpressure_warning,
};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, watch};

use self::connection::{connect_with_timeout, endpoint_label};
use self::server_list_manager::ServerListManager;
use self::watchdog::{HealthObserver, Watchdog};

/// Capacity of the event channel between client and consumers.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

/// Interval between telemetry snapshot emissions (in seconds).
const TELEMETRY_EMIT_INTERVAL_SECS: u64 = 5;

/// Maximum connection timeout (in seconds).
const MAX_CONNECT_TIMEOUT_SECS: u64 = 5;

/// Snapshot of client telemetry counters.
///
/// This structure tracks various metrics about the client's operation,
/// useful for monitoring and debugging.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "telemetry-serde",
    derive(serde::Serialize, serde::Deserialize)
)]
#[non_exhaustive]
pub struct QbtReceiverTelemetrySnapshot {
    /// Total connection attempts made.
    pub connection_attempts_total: u64,
    /// Total successful connections.
    pub connection_success_total: u64,
    /// Total failed connection attempts.
    pub connection_fail_total: u64,
    /// Total disconnections (expected and unexpected).
    pub disconnect_total: u64,
    /// Total watchdog timeouts.
    pub watchdog_timeouts_total: u64,
    /// Total watchdog exception events.
    pub watchdog_exception_events_total: u64,
    /// Total authentication logon messages sent.
    pub auth_logon_sent_total: u64,
    /// Total bytes received.
    pub bytes_in_total: u64,
    /// Total frame events decoded.
    pub frame_events_total: u64,
    /// Total data blocks emitted to handlers.
    pub data_blocks_emitted_total: u64,
    /// Total server list updates received.
    pub server_list_updates_total: u64,
    /// Total checksum mismatches detected.
    pub checksum_mismatch_total: u64,
    /// Total decompression failures.
    pub decompression_failed_total: u64,
    /// Total decoder recovery events.
    pub decoder_recovery_events_total: u64,
    /// Total handler failures.
    pub handler_failures_total: u64,
    /// Total backpressure warnings emitted.
    pub backpressure_warning_emitted_total: u64,
    /// Total events dropped due to channel full.
    pub event_queue_drop_total: u64,
    /// Total telemetry events emitted.
    pub telemetry_events_emitted_total: u64,
}

/// Internal runtime telemetry with tracking for backpressure reporting.
#[derive(Debug, Default)]
struct RuntimeTelemetry {
    /// Current snapshot of counters.
    snapshot: QbtReceiverTelemetrySnapshot,
    /// Shared backpressure/drop accounting.
    backpressure: BackpressureTracker,
}

/// Events emitted by the ByteBlaster client.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum QbtReceiverEvent {
    /// A protocol frame event (data block, server list, or warning).
    Frame(QbtFrameEvent),
    /// Connected to a server endpoint.
    Connected(String),
    /// Disconnected from the current endpoint.
    Disconnected,
    /// Periodic telemetry snapshot.
    Telemetry(QbtReceiverTelemetrySnapshot),
}

/// Type alias for event handler callbacks.
///
/// Handlers receive frame events and can return errors which will be
/// converted to warnings and emitted to other handlers.
pub type QbtReceiverEventHandler =
    Arc<dyn Fn(&QbtFrameEvent) -> QbtReceiverResult<()> + Send + Sync>;

/// Trait for ByteBlaster client implementations.
///
/// This trait defines the interface for starting, stopping, and
/// receiving events from a client connection.
pub trait QbtReceiverClient: Send {
    /// Starts the client connection loop.
    ///
    /// # Errors
    ///
    /// Returns an error if the client is already running.
    fn start(&mut self) -> QbtReceiverResult<()>;

    /// Stops the client and cleans up resources.
    ///
    /// # Errors
    ///
    /// Returns an error if cleanup fails.
    fn stop(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = QbtReceiverResult<()>> + Send + '_>>;

    /// Returns a stream of client events.
    ///
    /// This can only be called once; subsequent calls return an error.
    fn events(
        &mut self,
    ) -> Result<ReceiverEventStream<QbtReceiverEvent, QbtReceiverError>, QbtReceiverError>;
}

/// Builder for constructing a [`QbtReceiver`] with validation.
#[derive(Debug, Clone)]
pub struct QbtReceiverBuilder {
    config: QbtReceiverConfig,
}

impl QbtReceiverBuilder {
    /// Creates a new client builder with the given configuration.
    pub fn new(config: QbtReceiverConfig) -> Self {
        Self { config }
    }

    /// Builds a [`QbtReceiver`] after validating the configuration.
    ///
    /// # Errors
    ///
    /// Returns a [`QbtReceiverConfigError`](crate::qbt_receiver::error::QbtReceiverConfigError) if validation fails.
    pub fn build(self) -> Result<QbtReceiver, QbtReceiverError> {
        self.config.validate()?;
        Ok(QbtReceiver {
            config: self.config,
            runtime: ReceiverRuntime::default(),
            handlers: Vec::new(),
            telemetry: Arc::new(Mutex::new(QbtReceiverTelemetrySnapshot::default())),
        })
    }
}

/// ByteBlaster client implementation.
///
/// This is the main client type that manages the connection lifecycle,
/// event streaming, and telemetry. Use [`QbtReceiver::builder`] to construct
/// an instance with validated configuration.
pub struct QbtReceiver {
    /// QbtReceiver configuration.
    config: QbtReceiverConfig,
    /// Shared runtime lifecycle and event channel state.
    runtime: ReceiverRuntime<QbtReceiverEvent, QbtReceiverError>,
    /// Registered event handlers.
    handlers: Vec<QbtReceiverEventHandler>,
    /// Shared telemetry snapshot.
    telemetry: Arc<Mutex<QbtReceiverTelemetrySnapshot>>,
}

impl std::fmt::Debug for QbtReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QbtReceiver")
            .field("config", &self.config)
            .field("running", &self.runtime.is_running())
            .field("handler_count", &self.handlers.len())
            .finish()
    }
}

impl QbtReceiver {
    /// Creates a client builder with the given configuration.
    pub fn builder(config: QbtReceiverConfig) -> QbtReceiverBuilder {
        QbtReceiverBuilder::new(config)
    }

    pub fn config(&self) -> &QbtReceiverConfig {
        &self.config
    }

    pub fn subscribe(&mut self, handler: QbtReceiverEventHandler) {
        self.handlers.push(handler);
    }

    pub fn telemetry_snapshot(&self) -> QbtReceiverTelemetrySnapshot {
        self.telemetry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl QbtReceiverClient for QbtReceiver {
    fn start(&mut self) -> QbtReceiverResult<()> {
        if self.runtime.is_running() {
            return Err(QbtReceiverError::Lifecycle(
                "client already running".to_string(),
            ));
        }

        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let config = self.config.clone();
        let handlers = self.handlers.clone();
        let telemetry = Arc::clone(&self.telemetry);

        let join_handle = tokio::spawn(async move {
            run_connection_loop(config, event_tx, shutdown_rx, handlers, telemetry).await;
        });

        self.runtime.install(event_rx, shutdown_tx, join_handle);
        Ok(())
    }

    fn stop(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = QbtReceiverResult<()>> + Send + '_>> {
        Box::pin(async move {
            self.runtime.stop().await;
            Ok(())
        })
    }

    fn events(
        &mut self,
    ) -> Result<ReceiverEventStream<QbtReceiverEvent, QbtReceiverError>, QbtReceiverError> {
        self.runtime.take_events(QbtReceiverError::Lifecycle(
            "event stream already taken".to_string(),
        ))
    }
}

async fn run_connection_loop(
    config: QbtReceiverConfig,
    event_tx: mpsc::Sender<Result<QbtReceiverEvent, QbtReceiverError>>,
    mut shutdown_rx: watch::Receiver<bool>,
    handlers: Vec<QbtReceiverEventHandler>,
    telemetry_sink: Arc<Mutex<QbtReceiverTelemetrySnapshot>>,
) {
    let mut telemetry = RuntimeTelemetry::default();
    let mut server_list =
        ServerListManager::new(config.server_list_path.clone(), config.servers.clone());
    if config.follow_server_list_updates
        && let Err(err) = server_list.load()
    {
        try_send_event(&event_tx, Err(err), &mut telemetry);
    }

    while !*shutdown_rx.borrow() {
        telemetry.snapshot.connection_attempts_total = telemetry
            .snapshot
            .connection_attempts_total
            .saturating_add(1);
        update_telemetry_sink(&telemetry_sink, &telemetry);

        let Some((host, port)) = server_list.next_endpoint() else {
            try_send_event(
                &event_tx,
                Err(QbtReceiverError::Lifecycle(
                    "no servers configured".to_string(),
                )),
                &mut telemetry,
            );
            update_telemetry_sink(&telemetry_sink, &telemetry);
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(config.reconnect_delay_secs.max(1))) => {}
                _ = shutdown_rx.changed() => {}
            }
            continue;
        };

        let connect = connect_with_timeout(
            &host,
            port,
            Duration::from_secs(
                config
                    .connection_timeout_secs
                    .clamp(1, MAX_CONNECT_TIMEOUT_SECS),
            ),
        )
        .await;

        match connect {
            Ok(stream) => {
                telemetry.snapshot.connection_success_total = telemetry
                    .snapshot
                    .connection_success_total
                    .saturating_add(1);
                try_send_event(
                    &event_tx,
                    Ok(QbtReceiverEvent::Connected(endpoint_label(&host, port))),
                    &mut telemetry,
                );
                update_telemetry_sink(&telemetry_sink, &telemetry);

                let mut session_ctx = ConnectedSessionContext {
                    config: &config,
                    event_tx: &event_tx,
                    shutdown_rx: &mut shutdown_rx,
                    handlers: &handlers,
                    server_list: &mut server_list,
                    telemetry: &mut telemetry,
                    telemetry_sink: &telemetry_sink,
                };

                let run = run_connected_session(stream, &mut session_ctx).await;

                if let Err(err) = run {
                    try_send_event(&event_tx, Err(err), &mut telemetry);
                }

                if !*shutdown_rx.borrow() && config.follow_server_list_updates {
                    server_list.mark_bad_endpoint(&(host.clone(), port));
                }

                telemetry.snapshot.disconnect_total =
                    telemetry.snapshot.disconnect_total.saturating_add(1);
                try_send_event(
                    &event_tx,
                    Ok(QbtReceiverEvent::Disconnected),
                    &mut telemetry,
                );
                update_telemetry_sink(&telemetry_sink, &telemetry);
            }
            Err(err) => {
                telemetry.snapshot.connection_fail_total =
                    telemetry.snapshot.connection_fail_total.saturating_add(1);
                if config.follow_server_list_updates {
                    server_list.mark_bad_endpoint(&(host.clone(), port));
                }
                try_send_event(&event_tx, Err(QbtReceiverError::Io(err)), &mut telemetry);
                update_telemetry_sink(&telemetry_sink, &telemetry);
            }
        }

        tokio::task::yield_now().await;
    }

    update_telemetry_sink(&telemetry_sink, &telemetry);
}

struct ConnectedSessionContext<'a> {
    config: &'a QbtReceiverConfig,
    event_tx: &'a mpsc::Sender<Result<QbtReceiverEvent, QbtReceiverError>>,
    shutdown_rx: &'a mut watch::Receiver<bool>,
    handlers: &'a [QbtReceiverEventHandler],
    server_list: &'a mut ServerListManager,
    telemetry: &'a mut RuntimeTelemetry,
    telemetry_sink: &'a Arc<Mutex<QbtReceiverTelemetrySnapshot>>,
}

async fn run_connected_session(
    mut stream: tokio::net::TcpStream,
    ctx: &mut ConnectedSessionContext<'_>,
) -> QbtReceiverResult<()> {
    let mut decoder = QbtProtocolDecoder::new(ctx.config.decode.clone());
    let watchdog = Watchdog::new(ctx.config.watchdog_timeout_secs, ctx.config.max_exceptions);
    let mut auth_interval = tokio::time::interval(Duration::from_secs(REAUTH_INTERVAL_SECS));
    auth_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    auth_interval.tick().await;
    let mut telemetry_interval =
        tokio::time::interval(Duration::from_secs(TELEMETRY_EMIT_INTERVAL_SECS));
    telemetry_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let auth = QbtAuthMessage {
        email: ctx.config.email.clone(),
    };
    let initial = xor_ff(build_logon_message(&auth.email).as_bytes());
    stream.write_all(&initial).await?;
    ctx.telemetry.snapshot.auth_logon_sent_total = ctx
        .telemetry
        .snapshot
        .auth_logon_sent_total
        .saturating_add(1);
    update_telemetry_sink(ctx.telemetry_sink, ctx.telemetry);

    let mut buf = vec![0u8; 8192];

    loop {
        if *ctx.shutdown_rx.borrow() {
            return Ok(());
        }

        tokio::select! {
            _ = ctx.shutdown_rx.changed() => {
                return Ok(());
            }
            _ = auth_interval.tick() => {
                let logon = xor_ff(build_logon_message(&auth.email).as_bytes());
                stream.write_all(&logon).await?;
                ctx.telemetry.snapshot.auth_logon_sent_total = ctx.telemetry.snapshot.auth_logon_sent_total.saturating_add(1);
                update_telemetry_sink(ctx.telemetry_sink, ctx.telemetry);
            }
            _ = telemetry_interval.tick() => {
                ctx.telemetry.snapshot.telemetry_events_emitted_total = ctx.telemetry
                    .snapshot
                    .telemetry_events_emitted_total
                    .saturating_add(1);
                try_send_event(
                    ctx.event_tx,
                    Ok(QbtReceiverEvent::Telemetry(ctx.telemetry.snapshot.clone())),
                    ctx.telemetry,
                );
                update_telemetry_sink(ctx.telemetry_sink, ctx.telemetry);
            }
            read = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buf)) => {
                match read {
                    Ok(Ok(0)) => return Ok(()),
                    Ok(Ok(n)) => {
                        watchdog.on_data_received();
                        ctx.telemetry.snapshot.bytes_in_total = ctx.telemetry.snapshot.bytes_in_total.saturating_add(n as u64);
                        match decoder.feed(&buf[..n]) {
                            Ok(events) => {
                                ctx.telemetry.snapshot.decoder_recovery_events_total = ctx.telemetry
                                    .snapshot
                                    .decoder_recovery_events_total
                                    .saturating_add(count_decoder_recoveries(&events) as u64);
                                ctx.telemetry.snapshot.frame_events_total = ctx.telemetry
                                    .snapshot
                                    .frame_events_total
                                    .saturating_add(events.len() as u64);
                                ctx.telemetry.snapshot.data_blocks_emitted_total = ctx.telemetry
                                    .snapshot
                                    .data_blocks_emitted_total
                                    .saturating_add(count_data_blocks(&events) as u64);
                                ctx.telemetry.snapshot.server_list_updates_total = ctx.telemetry
                                    .snapshot
                                    .server_list_updates_total
                                    .saturating_add(count_server_list_updates(&events) as u64);
                                ctx.telemetry.snapshot.checksum_mismatch_total = ctx.telemetry
                                    .snapshot
                                    .checksum_mismatch_total
                                    .saturating_add(count_checksum_mismatches(&events) as u64);
                                ctx.telemetry.snapshot.decompression_failed_total = ctx.telemetry
                                    .snapshot
                                    .decompression_failed_total
                                    .saturating_add(count_decompression_failures(&events) as u64);
                                for event in &events {
                                    if ctx.config.follow_server_list_updates
                                        && let QbtFrameEvent::ServerListUpdate(list) = event
                                        && let Err(err) = ctx.server_list.apply_server_list(list.clone()) {
                                        try_send_event(ctx.event_tx, Err(err), ctx.telemetry);
                                    }
                                }
                                dispatch_events(ctx.event_tx, ctx.handlers, events, ctx.telemetry);
                                update_telemetry_sink(ctx.telemetry_sink, ctx.telemetry);
                            }
                            Err(err) => {
                                watchdog.on_exception();
                                ctx.telemetry.snapshot.watchdog_exception_events_total = ctx.telemetry
                                    .snapshot
                                    .watchdog_exception_events_total
                                    .saturating_add(1);
                                decoder.reset();
                                try_send_event(ctx.event_tx, Err(QbtReceiverError::Protocol(err)), ctx.telemetry);
                                update_telemetry_sink(ctx.telemetry_sink, ctx.telemetry);
                            }
                        }
                    }
                    Ok(Err(err)) => {
                        watchdog.on_exception();
                        ctx.telemetry.snapshot.watchdog_exception_events_total = ctx.telemetry
                            .snapshot
                            .watchdog_exception_events_total
                            .saturating_add(1);
                        update_telemetry_sink(ctx.telemetry_sink, ctx.telemetry);
                        return Err(QbtReceiverError::Io(err));
                    }
                    Err(_elapsed) => {
                        if watchdog.should_close() {
                            ctx.telemetry.snapshot.watchdog_timeouts_total = ctx.telemetry
                                .snapshot
                                .watchdog_timeouts_total
                                .saturating_add(1);
                            update_telemetry_sink(ctx.telemetry_sink, ctx.telemetry);
                            return Err(QbtReceiverError::Lifecycle("watchdog timeout".to_string()));
                        }
                    }
                }
            }
        }
    }
}

fn dispatch_events(
    event_tx: &mpsc::Sender<Result<QbtReceiverEvent, QbtReceiverError>>,
    handlers: &[QbtReceiverEventHandler],
    events: Vec<QbtFrameEvent>,
    telemetry: &mut RuntimeTelemetry,
) {
    for event in events {
        for handler in handlers {
            if let Err(err) = handler(&event) {
                telemetry.snapshot.handler_failures_total =
                    telemetry.snapshot.handler_failures_total.saturating_add(1);
                let warning = QbtFrameEvent::Warning(QbtProtocolWarning::HandlerError {
                    message: err.to_string(),
                });
                try_send_event(event_tx, Ok(QbtReceiverEvent::Frame(warning)), telemetry);
            }
        }
        try_send_event(event_tx, Ok(QbtReceiverEvent::Frame(event)), telemetry);
    }
}

fn count_decoder_recoveries(events: &[QbtFrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                QbtFrameEvent::Warning(QbtProtocolWarning::DecoderRecovered { .. })
            )
        })
        .count()
}

fn count_data_blocks(events: &[QbtFrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, QbtFrameEvent::DataBlock(_)))
        .count()
}

fn count_server_list_updates(events: &[QbtFrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, QbtFrameEvent::ServerListUpdate(_)))
        .count()
}

fn count_checksum_mismatches(events: &[QbtFrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                QbtFrameEvent::Warning(QbtProtocolWarning::ChecksumMismatch { .. })
            )
        })
        .count()
}

fn count_decompression_failures(events: &[QbtFrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                QbtFrameEvent::Warning(QbtProtocolWarning::DecompressionFailed { .. })
            )
        })
        .count()
}

fn try_send_event(
    event_tx: &mpsc::Sender<Result<QbtReceiverEvent, QbtReceiverError>>,
    event: Result<QbtReceiverEvent, QbtReceiverError>,
    telemetry: &mut RuntimeTelemetry,
) {
    let decoder_recovery_events = telemetry.snapshot.decoder_recovery_events_total;
    try_send_with_backpressure_warning(
        event_tx,
        event,
        &mut telemetry.backpressure,
        |tracker| {
            QbtReceiverEvent::Frame(QbtFrameEvent::Warning(
                QbtProtocolWarning::BackpressureDrop {
                    dropped_since_last_report: tracker.dropped_since_last_report(),
                    total_dropped_events: tracker.event_queue_drop_total(),
                    decoder_recovery_events,
                },
            ))
        },
        || {
            telemetry.snapshot.backpressure_warning_emitted_total = telemetry
                .snapshot
                .backpressure_warning_emitted_total
                .saturating_add(1);
        },
        |tracker| {
            telemetry.snapshot.event_queue_drop_total = tracker.event_queue_drop_total();
        },
    );
}

fn update_telemetry_sink(
    telemetry_sink: &Arc<Mutex<QbtReceiverTelemetrySnapshot>>,
    telemetry: &RuntimeTelemetry,
) {
    let mut guard = telemetry_sink
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = telemetry.snapshot.clone();
}

#[cfg(test)]
mod tests {
    use super::{
        QbtReceiverEvent, QbtReceiverTelemetrySnapshot, RuntimeTelemetry, dispatch_events,
        try_send_event,
    };
    use crate::qbt_receiver::client::QbtReceiverEventHandler;
    use crate::qbt_receiver::error::QbtReceiverError;
    use crate::qbt_receiver::protocol::model::{QbtFrameEvent, QbtProtocolWarning, QbtServerList};
    use crate::runtime_support::BackpressureTracker;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn handler_error_isolated() {
        let called_ok = Arc::new(AtomicUsize::new(0));
        let called_ok_clone = Arc::clone(&called_ok);

        let bad: QbtReceiverEventHandler =
            Arc::new(|_evt: &QbtFrameEvent| -> Result<(), QbtReceiverError> {
                Err(QbtReceiverError::Lifecycle("boom".to_string()))
            });
        let good: QbtReceiverEventHandler = Arc::new(
            move |_evt: &QbtFrameEvent| -> Result<(), QbtReceiverError> {
                called_ok_clone.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        );

        let handlers = vec![bad, good];
        let (tx, mut rx) = mpsc::channel(16);
        let events = vec![QbtFrameEvent::ServerListUpdate(QbtServerList::default())];
        let mut telemetry = RuntimeTelemetry::default();

        dispatch_events(&tx, &handlers, events, &mut telemetry);

        let mut saw_warning = false;
        let mut saw_frame = false;
        while let Ok(item) =
            tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await
        {
            match item {
                Some(Ok(QbtReceiverEvent::Frame(QbtFrameEvent::Warning(
                    QbtProtocolWarning::HandlerError { .. },
                )))) => {
                    saw_warning = true;
                }
                Some(Ok(QbtReceiverEvent::Frame(QbtFrameEvent::ServerListUpdate(_)))) => {
                    saw_frame = true;
                }
                Some(_) => {}
                None => break,
            }
            if saw_warning && saw_frame {
                break;
            }
        }

        assert!(saw_warning);
        assert!(saw_frame);
        assert_eq!(called_ok.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn backpressure_drop_emits_warning_with_counters() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut telemetry = RuntimeTelemetry {
            snapshot: QbtReceiverTelemetrySnapshot {
                decoder_recovery_events_total: 3,
                event_queue_drop_total: 0,
                ..QbtReceiverTelemetrySnapshot::default()
            },
            backpressure: BackpressureTracker::default(),
        };

        tx.try_send(Ok(QbtReceiverEvent::Disconnected))
            .expect("seed event should fit");

        try_send_event(
            &tx,
            Ok(QbtReceiverEvent::Frame(QbtFrameEvent::ServerListUpdate(
                QbtServerList::default(),
            ))),
            &mut telemetry,
        );

        assert_eq!(telemetry.snapshot.event_queue_drop_total, 1);
        assert_eq!(telemetry.backpressure.dropped_since_last_report(), 1);

        let _ = rx.recv().await;

        try_send_event(
            &tx,
            Ok(QbtReceiverEvent::Frame(QbtFrameEvent::ServerListUpdate(
                QbtServerList::default(),
            ))),
            &mut telemetry,
        );

        let warning_item = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv())
            .await
            .expect("warning should be emitted before timeout")
            .expect("channel should still be open");

        match warning_item {
            Ok(QbtReceiverEvent::Frame(QbtFrameEvent::Warning(
                QbtProtocolWarning::BackpressureDrop {
                    dropped_since_last_report,
                    total_dropped_events,
                    decoder_recovery_events,
                },
            ))) => {
                assert_eq!(dropped_since_last_report, 1);
                assert_eq!(total_dropped_events, 1);
                assert_eq!(decoder_recovery_events, 3);
            }
            other => panic!("expected backpressure warning, got {other:?}"),
        }

        assert_eq!(telemetry.snapshot.event_queue_drop_total, 2);
        assert_eq!(telemetry.backpressure.dropped_since_last_report(), 1);
    }

    #[tokio::test]
    async fn backpressure_drop_warning_reports_and_resets_window() {
        let (tx, mut rx) = mpsc::channel(4);
        let mut telemetry = RuntimeTelemetry {
            snapshot: QbtReceiverTelemetrySnapshot {
                decoder_recovery_events_total: 5,
                event_queue_drop_total: 7,
                ..QbtReceiverTelemetrySnapshot::default()
            },
            backpressure: BackpressureTracker::new(7, 2),
        };

        try_send_event(
            &tx,
            Ok(QbtReceiverEvent::Frame(QbtFrameEvent::ServerListUpdate(
                QbtServerList::default(),
            ))),
            &mut telemetry,
        );

        assert_eq!(telemetry.snapshot.event_queue_drop_total, 7);
        assert_eq!(telemetry.backpressure.dropped_since_last_report(), 0);

        let first = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv())
            .await
            .expect("first item should arrive")
            .expect("first item should exist");
        let second = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv())
            .await
            .expect("second item should arrive")
            .expect("second item should exist");

        match first {
            Ok(QbtReceiverEvent::Frame(QbtFrameEvent::Warning(
                QbtProtocolWarning::BackpressureDrop {
                    dropped_since_last_report,
                    total_dropped_events,
                    decoder_recovery_events,
                },
            ))) => {
                assert_eq!(dropped_since_last_report, 2);
                assert_eq!(total_dropped_events, 7);
                assert_eq!(decoder_recovery_events, 5);
            }
            other => panic!("expected first item to be warning, got {other:?}"),
        }

        assert!(matches!(
            second,
            Ok(QbtReceiverEvent::Frame(QbtFrameEvent::ServerListUpdate(_)))
        ));
    }
}
