pub mod connection;
pub mod reconnect;
pub mod server_list_manager;
pub mod watchdog;

use crate::config::ClientConfig;
use crate::error::{CoreError, CoreResult};
use crate::protocol::auth::{REAUTH_INTERVAL_SECS, build_logon_message, xor_ff};
use crate::protocol::codec::{FrameDecoder, ProtocolDecoder};
use crate::protocol::model::FrameEvent;
use crate::{protocol::model::AuthMessage, protocol::model::ProtocolWarning};
use futures::{Stream, stream};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};

use self::connection::{connect_with_timeout, endpoint_label};
use self::server_list_manager::ServerListManager;
use self::watchdog::{HealthObserver, Watchdog};

const EVENT_CHANNEL_CAPACITY: usize = 1024;
const TELEMETRY_EMIT_INTERVAL_SECS: u64 = 5;
const MAX_CONNECT_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "telemetry-serde",
    derive(serde::Serialize, serde::Deserialize)
)]
#[non_exhaustive]
pub struct ClientTelemetrySnapshot {
    pub connection_attempts_total: u64,
    pub connection_success_total: u64,
    pub connection_fail_total: u64,
    pub disconnect_total: u64,
    pub watchdog_timeouts_total: u64,
    pub watchdog_exception_events_total: u64,
    pub auth_logon_sent_total: u64,
    pub bytes_in_total: u64,
    pub frame_events_total: u64,
    pub data_blocks_emitted_total: u64,
    pub server_list_updates_total: u64,
    pub checksum_mismatch_total: u64,
    pub decompression_failed_total: u64,
    pub decoder_recovery_events_total: u64,
    pub handler_failures_total: u64,
    pub backpressure_warning_emitted_total: u64,
    pub event_queue_drop_total: u64,
    pub telemetry_events_emitted_total: u64,
}

#[derive(Debug, Default)]
struct RuntimeTelemetry {
    snapshot: ClientTelemetrySnapshot,
    dropped_since_last_report: u64,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ClientEvent {
    Frame(FrameEvent),
    Connected(String),
    Disconnected,
    Telemetry(ClientTelemetrySnapshot),
}

pub type EventHandler = Arc<dyn Fn(&FrameEvent) -> CoreResult<()> + Send + Sync>;

pub trait ByteBlasterClient: Send {
    fn start(&mut self) -> CoreResult<()>;
    fn stop(&mut self) -> Pin<Box<dyn std::future::Future<Output = CoreResult<()>> + Send + '_>>;
    fn events(&mut self)
    -> Pin<Box<dyn Stream<Item = Result<ClientEvent, CoreError>> + Send + '_>>;
}

#[derive(Debug, Clone)]
pub struct ClientBuilder {
    config: ClientConfig,
}

impl ClientBuilder {
    pub fn new(config: ClientConfig) -> Self {
        Self { config }
    }

    pub fn build(self) -> Result<Client, CoreError> {
        self.config.validate()?;
        Ok(Client {
            config: self.config,
            running: false,
            event_rx: None,
            shutdown_tx: None,
            join_handle: None,
            handlers: Vec::new(),
            telemetry: Arc::new(Mutex::new(ClientTelemetrySnapshot::default())),
        })
    }
}

pub struct Client {
    config: ClientConfig,
    running: bool,
    event_rx: Option<mpsc::Receiver<Result<ClientEvent, CoreError>>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    join_handle: Option<tokio::task::JoinHandle<()>>,
    handlers: Vec<EventHandler>,
    telemetry: Arc<Mutex<ClientTelemetrySnapshot>>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("config", &self.config)
            .field("running", &self.running)
            .field("handler_count", &self.handlers.len())
            .finish()
    }
}

impl Client {
    pub fn builder(config: ClientConfig) -> ClientBuilder {
        ClientBuilder::new(config)
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub fn subscribe(&mut self, handler: EventHandler) {
        self.handlers.push(handler);
    }

    pub fn telemetry_snapshot(&self) -> ClientTelemetrySnapshot {
        self.telemetry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl ByteBlasterClient for Client {
    fn start(&mut self) -> CoreResult<()> {
        if self.running {
            return Err(CoreError::Lifecycle("client already running".to_string()));
        }

        let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let config = self.config.clone();
        let handlers = self.handlers.clone();
        let telemetry = Arc::clone(&self.telemetry);

        self.join_handle = Some(tokio::spawn(async move {
            run_connection_loop(config, event_tx, shutdown_rx, handlers, telemetry).await;
        }));

        self.event_rx = Some(event_rx);
        self.shutdown_tx = Some(shutdown_tx);
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Pin<Box<dyn std::future::Future<Output = CoreResult<()>> + Send + '_>> {
        Box::pin(async move {
            if !self.running {
                return Ok(());
            }

            if let Some(tx) = &self.shutdown_tx {
                let _ = tx.send(true);
            }

            if let Some(handle) = self.join_handle.take() {
                let _ = handle.await;
            }

            self.running = false;
            self.shutdown_tx = None;
            Ok(())
        })
    }

    fn events(
        &mut self,
    ) -> Pin<Box<dyn Stream<Item = Result<ClientEvent, CoreError>> + Send + '_>> {
        match self.event_rx.take() {
            Some(rx) => Box::pin(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|item| (item, rx))
            })),
            None => Box::pin(stream::empty()),
        }
    }
}

async fn run_connection_loop(
    config: ClientConfig,
    event_tx: mpsc::Sender<Result<ClientEvent, CoreError>>,
    mut shutdown_rx: watch::Receiver<bool>,
    handlers: Vec<EventHandler>,
    telemetry_sink: Arc<Mutex<ClientTelemetrySnapshot>>,
) {
    let mut telemetry = RuntimeTelemetry::default();
    let mut server_list =
        ServerListManager::new(config.server_list_path.clone(), config.servers.clone());
    if let Err(err) = server_list.load() {
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
                Err(CoreError::Lifecycle("no servers configured".to_string())),
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
                    Ok(ClientEvent::Connected(endpoint_label(&host, port))),
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

                if !*shutdown_rx.borrow() {
                    server_list.mark_bad_endpoint(&(host.clone(), port));
                }

                telemetry.snapshot.disconnect_total =
                    telemetry.snapshot.disconnect_total.saturating_add(1);
                try_send_event(&event_tx, Ok(ClientEvent::Disconnected), &mut telemetry);
                update_telemetry_sink(&telemetry_sink, &telemetry);
            }
            Err(err) => {
                telemetry.snapshot.connection_fail_total =
                    telemetry.snapshot.connection_fail_total.saturating_add(1);
                server_list.mark_bad_endpoint(&(host.clone(), port));
                try_send_event(&event_tx, Err(CoreError::Io(err)), &mut telemetry);
                update_telemetry_sink(&telemetry_sink, &telemetry);
            }
        }

        tokio::task::yield_now().await;
    }

    update_telemetry_sink(&telemetry_sink, &telemetry);
}

struct ConnectedSessionContext<'a> {
    config: &'a ClientConfig,
    event_tx: &'a mpsc::Sender<Result<ClientEvent, CoreError>>,
    shutdown_rx: &'a mut watch::Receiver<bool>,
    handlers: &'a [EventHandler],
    server_list: &'a mut ServerListManager,
    telemetry: &'a mut RuntimeTelemetry,
    telemetry_sink: &'a Arc<Mutex<ClientTelemetrySnapshot>>,
}

async fn run_connected_session(
    mut stream: tokio::net::TcpStream,
    ctx: &mut ConnectedSessionContext<'_>,
) -> CoreResult<()> {
    let mut decoder = ProtocolDecoder::new(ctx.config.decode.clone());
    let watchdog = Watchdog::new(ctx.config.watchdog_timeout_secs, ctx.config.max_exceptions);
    let mut auth_interval = tokio::time::interval(Duration::from_secs(REAUTH_INTERVAL_SECS));
    auth_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut telemetry_interval =
        tokio::time::interval(Duration::from_secs(TELEMETRY_EMIT_INTERVAL_SECS));
    telemetry_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let auth = AuthMessage {
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
                    Ok(ClientEvent::Telemetry(ctx.telemetry.snapshot.clone())),
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
                                    if let FrameEvent::ServerListUpdate(list) = event {
                                        if let Err(err) = ctx.server_list.apply_server_list(list.clone()) {
                                            try_send_event(ctx.event_tx, Err(err), ctx.telemetry);
                                        }
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
                                try_send_event(ctx.event_tx, Err(CoreError::Protocol(err)), ctx.telemetry);
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
                        return Err(CoreError::Io(err));
                    }
                    Err(_elapsed) => {
                        if watchdog.should_close() {
                            ctx.telemetry.snapshot.watchdog_timeouts_total = ctx.telemetry
                                .snapshot
                                .watchdog_timeouts_total
                                .saturating_add(1);
                            update_telemetry_sink(ctx.telemetry_sink, ctx.telemetry);
                            return Err(CoreError::Lifecycle("watchdog timeout".to_string()));
                        }
                    }
                }
            }
        }
    }
}

fn dispatch_events(
    event_tx: &mpsc::Sender<Result<ClientEvent, CoreError>>,
    handlers: &[EventHandler],
    events: Vec<FrameEvent>,
    telemetry: &mut RuntimeTelemetry,
) {
    for event in events {
        for handler in handlers {
            if let Err(err) = handler(&event) {
                telemetry.snapshot.handler_failures_total =
                    telemetry.snapshot.handler_failures_total.saturating_add(1);
                let warning = FrameEvent::Warning(ProtocolWarning::HandlerError {
                    message: err.to_string(),
                });
                try_send_event(event_tx, Ok(ClientEvent::Frame(warning)), telemetry);
            }
        }
        try_send_event(event_tx, Ok(ClientEvent::Frame(event)), telemetry);
    }
}

fn count_decoder_recoveries(events: &[FrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                FrameEvent::Warning(ProtocolWarning::DecoderRecovered { .. })
            )
        })
        .count()
}

fn count_data_blocks(events: &[FrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, FrameEvent::DataBlock(_)))
        .count()
}

fn count_server_list_updates(events: &[FrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, FrameEvent::ServerListUpdate(_)))
        .count()
}

fn count_checksum_mismatches(events: &[FrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                FrameEvent::Warning(ProtocolWarning::ChecksumMismatch { .. })
            )
        })
        .count()
}

fn count_decompression_failures(events: &[FrameEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                FrameEvent::Warning(ProtocolWarning::DecompressionFailed { .. })
            )
        })
        .count()
}

fn try_send_event(
    event_tx: &mpsc::Sender<Result<ClientEvent, CoreError>>,
    event: Result<ClientEvent, CoreError>,
    telemetry: &mut RuntimeTelemetry,
) {
    try_emit_backpressure_warning(event_tx, telemetry);
    if let Err(err) = event_tx.try_send(event) {
        record_dropped_event(err, telemetry);
    }
}

fn try_emit_backpressure_warning(
    event_tx: &mpsc::Sender<Result<ClientEvent, CoreError>>,
    telemetry: &mut RuntimeTelemetry,
) {
    if telemetry.dropped_since_last_report == 0 {
        return;
    }

    let warning = FrameEvent::Warning(ProtocolWarning::BackpressureDrop {
        dropped_since_last_report: telemetry.dropped_since_last_report,
        total_dropped_events: telemetry.snapshot.event_queue_drop_total,
        decoder_recovery_events: telemetry.snapshot.decoder_recovery_events_total,
    });

    match event_tx.try_send(Ok(ClientEvent::Frame(warning))) {
        Ok(()) => {
            telemetry.snapshot.backpressure_warning_emitted_total = telemetry
                .snapshot
                .backpressure_warning_emitted_total
                .saturating_add(1);
            telemetry.dropped_since_last_report = 0;
        }
        Err(err) => record_dropped_event(err, telemetry),
    }
}

fn record_dropped_event(
    err: TrySendError<Result<ClientEvent, CoreError>>,
    telemetry: &mut RuntimeTelemetry,
) {
    if matches!(err, TrySendError::Full(_)) {
        telemetry.snapshot.event_queue_drop_total =
            telemetry.snapshot.event_queue_drop_total.saturating_add(1);
        telemetry.dropped_since_last_report = telemetry.dropped_since_last_report.saturating_add(1);
    }
}

fn update_telemetry_sink(
    telemetry_sink: &Arc<Mutex<ClientTelemetrySnapshot>>,
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
        ClientEvent, ClientTelemetrySnapshot, RuntimeTelemetry, dispatch_events, try_send_event,
    };
    use crate::client::EventHandler;
    use crate::error::CoreError;
    use crate::protocol::model::{FrameEvent, ProtocolWarning, ServerList};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn handler_error_isolated() {
        let called_ok = Arc::new(AtomicUsize::new(0));
        let called_ok_clone = Arc::clone(&called_ok);

        let bad: EventHandler = Arc::new(|_evt: &FrameEvent| -> Result<(), CoreError> {
            Err(CoreError::Lifecycle("boom".to_string()))
        });
        let good: EventHandler = Arc::new(move |_evt: &FrameEvent| -> Result<(), CoreError> {
            called_ok_clone.fetch_add(1, Ordering::Relaxed);
            Ok(())
        });

        let handlers = vec![bad, good];
        let (tx, mut rx) = mpsc::channel(16);
        let events = vec![FrameEvent::ServerListUpdate(ServerList::default())];
        let mut telemetry = RuntimeTelemetry::default();

        dispatch_events(&tx, &handlers, events, &mut telemetry);

        let mut saw_warning = false;
        let mut saw_frame = false;
        while let Ok(item) =
            tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await
        {
            match item {
                Some(Ok(ClientEvent::Frame(FrameEvent::Warning(
                    ProtocolWarning::HandlerError { .. },
                )))) => {
                    saw_warning = true;
                }
                Some(Ok(ClientEvent::Frame(FrameEvent::ServerListUpdate(_)))) => {
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
            snapshot: ClientTelemetrySnapshot {
                decoder_recovery_events_total: 3,
                event_queue_drop_total: 0,
                ..ClientTelemetrySnapshot::default()
            },
            dropped_since_last_report: 0,
        };

        tx.try_send(Ok(ClientEvent::Disconnected))
            .expect("seed event should fit");

        try_send_event(
            &tx,
            Ok(ClientEvent::Frame(FrameEvent::ServerListUpdate(
                ServerList::default(),
            ))),
            &mut telemetry,
        );

        assert_eq!(telemetry.snapshot.event_queue_drop_total, 1);
        assert_eq!(telemetry.dropped_since_last_report, 1);

        let _ = rx.recv().await;

        try_send_event(
            &tx,
            Ok(ClientEvent::Frame(FrameEvent::ServerListUpdate(
                ServerList::default(),
            ))),
            &mut telemetry,
        );

        let warning_item = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv())
            .await
            .expect("warning should be emitted before timeout")
            .expect("channel should still be open");

        match warning_item {
            Ok(ClientEvent::Frame(FrameEvent::Warning(ProtocolWarning::BackpressureDrop {
                dropped_since_last_report,
                total_dropped_events,
                decoder_recovery_events,
            }))) => {
                assert_eq!(dropped_since_last_report, 1);
                assert_eq!(total_dropped_events, 1);
                assert_eq!(decoder_recovery_events, 3);
            }
            other => panic!("expected backpressure warning, got {other:?}"),
        }

        assert_eq!(telemetry.snapshot.event_queue_drop_total, 2);
        assert_eq!(telemetry.dropped_since_last_report, 1);
    }

    #[tokio::test]
    async fn backpressure_drop_warning_reports_and_resets_window() {
        let (tx, mut rx) = mpsc::channel(4);
        let mut telemetry = RuntimeTelemetry {
            snapshot: ClientTelemetrySnapshot {
                decoder_recovery_events_total: 5,
                event_queue_drop_total: 7,
                ..ClientTelemetrySnapshot::default()
            },
            dropped_since_last_report: 2,
        };

        try_send_event(
            &tx,
            Ok(ClientEvent::Frame(FrameEvent::ServerListUpdate(
                ServerList::default(),
            ))),
            &mut telemetry,
        );

        assert_eq!(telemetry.snapshot.event_queue_drop_total, 7);
        assert_eq!(telemetry.dropped_since_last_report, 0);

        let first = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv())
            .await
            .expect("first item should arrive")
            .expect("first item should exist");
        let second = tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv())
            .await
            .expect("second item should arrive")
            .expect("second item should exist");

        match first {
            Ok(ClientEvent::Frame(FrameEvent::Warning(ProtocolWarning::BackpressureDrop {
                dropped_since_last_report,
                total_dropped_events,
                decoder_recovery_events,
            }))) => {
                assert_eq!(dropped_since_last_report, 2);
                assert_eq!(total_dropped_events, 7);
                assert_eq!(decoder_recovery_events, 5);
            }
            other => panic!("expected first item to be warning, got {other:?}"),
        }

        assert!(matches!(
            second,
            Ok(ClientEvent::Frame(FrameEvent::ServerListUpdate(_)))
        ));
    }
}
