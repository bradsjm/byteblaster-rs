use crate::wxwire_receiver::codec::{WxWireDecoder, WxWireFrameDecoder};
use crate::wxwire_receiver::config::{WXWIRE_PRIMARY_HOST, WxWireReceiverConfig};
use crate::wxwire_receiver::error::{WxWireReceiverError, WxWireReceiverResult};
use crate::wxwire_receiver::model::{WxWireReceiverFrameEvent, WxWireReceiverWarning};
use crate::wxwire_receiver::transport::{WxWireTransport, XmppWxWireTransport};
use futures::{Stream, stream};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};
use tracing::warn;

#[cfg(not(test))]
const RECONNECT_BACKOFF_INITIAL: Duration = Duration::from_secs(1);
#[cfg(test)]
const RECONNECT_BACKOFF_INITIAL: Duration = Duration::from_millis(10);
#[cfg(not(test))]
const RECONNECT_BACKOFF_MAX: Duration = Duration::from_secs(30);
#[cfg(test)]
const RECONNECT_BACKOFF_MAX: Duration = Duration::from_millis(100);

/// Snapshot of weather wire runtime telemetry counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "telemetry-serde",
    derive(serde::Serialize, serde::Deserialize)
)]
#[non_exhaustive]
pub struct WxWireReceiverTelemetrySnapshot {
    /// Total client starts.
    pub starts_total: u64,
    /// Total stop requests.
    pub stops_total: u64,
    /// Total successful message decodes.
    pub decoded_messages_total: u64,
    /// Total file events emitted.
    pub files_emitted_total: u64,
    /// Total warning events emitted.
    pub warning_events_total: u64,
    /// Total handler failures.
    pub handler_failures_total: u64,
    /// Total dropped events because output queue was full.
    pub event_queue_drop_total: u64,
    /// Total backpressure warnings emitted.
    pub backpressure_warning_emitted_total: u64,
    /// Total idle-timeout reconnect cycles.
    pub idle_reconnects_total: u64,
    /// Total transport reconnect attempts.
    pub reconnect_attempts_total: u64,
    /// Total endpoint connection attempts.
    pub connect_attempts_total: u64,
    /// Total telemetry events emitted.
    pub telemetry_events_emitted_total: u64,
}

#[derive(Debug, Default)]
struct RuntimeTelemetry {
    snapshot: WxWireReceiverTelemetrySnapshot,
    dropped_since_last_report: u64,
}

/// Events emitted by the weather wire client.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WxWireReceiverEvent {
    /// Frame-level event (file or warning).
    Frame(WxWireReceiverFrameEvent),
    /// Connected endpoint label.
    Connected(String),
    /// Disconnected from endpoint.
    Disconnected,
    /// Periodic telemetry snapshot.
    Telemetry(WxWireReceiverTelemetrySnapshot),
}

/// Weather wire event handler callback type.
pub type WxWireReceiverEventHandler =
    Arc<dyn Fn(&WxWireReceiverFrameEvent) -> WxWireReceiverResult<()> + Send + Sync>;

type TransportFuture =
    Pin<Box<dyn Future<Output = WxWireReceiverResult<Box<dyn WxWireTransport>>> + Send>>;
type TransportFactory = Arc<dyn Fn(String, String, Duration) -> TransportFuture + Send + Sync>;

/// Trait for weather wire clients.
pub trait WxWireReceiverClient: Send {
    /// Starts the runtime loop.
    fn start(&mut self) -> WxWireReceiverResult<()>;

    /// Stops the runtime loop.
    fn stop(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = WxWireReceiverResult<()>> + Send + '_>>;

    /// Returns a stream of runtime events.
    fn events(
        &mut self,
    ) -> Pin<Box<dyn Stream<Item = Result<WxWireReceiverEvent, WxWireReceiverError>> + Send + '_>>;
}

/// Unstable ingress surface for raw stanza injection.
pub trait UnstableWxWireReceiverIngress {
    /// Submits one raw XMPP stanza string to the runtime decoder.
    fn submit_raw_stanza(&self, stanza: String) -> WxWireReceiverResult<()>;
}

/// Builder for validated weather wire client construction.
#[derive(Clone)]
pub struct WxWireReceiverBuilder {
    config: WxWireReceiverConfig,
    transport_factory: TransportFactory,
}

impl std::fmt::Debug for WxWireReceiverBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WxWireReceiverBuilder")
            .field("config", &self.config)
            .finish()
    }
}

impl WxWireReceiverBuilder {
    /// Creates a new builder.
    pub fn new(config: WxWireReceiverConfig) -> Self {
        Self {
            config,
            transport_factory: Arc::new(default_transport_factory),
        }
    }

    /// Overrides transport construction logic.
    ///
    /// This is intended for tests and unstable integrations.
    pub fn with_transport_factory(mut self, factory: TransportFactory) -> Self {
        self.transport_factory = factory;
        self
    }

    /// Validates config and builds a client instance.
    pub fn build(self) -> Result<WxWireReceiver, WxWireReceiverError> {
        self.config.validate()?;
        Ok(WxWireReceiver {
            config: self.config,
            running: false,
            event_rx: None,
            shutdown_tx: None,
            join_handle: None,
            ingress_tx: None,
            handlers: Vec::new(),
            telemetry: Arc::new(Mutex::new(WxWireReceiverTelemetrySnapshot::default())),
            transport_factory: self.transport_factory,
        })
    }
}

/// Weather wire client runtime.
pub struct WxWireReceiver {
    config: WxWireReceiverConfig,
    running: bool,
    event_rx: Option<mpsc::Receiver<Result<WxWireReceiverEvent, WxWireReceiverError>>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    join_handle: Option<tokio::task::JoinHandle<()>>,
    ingress_tx: Option<mpsc::Sender<String>>,
    handlers: Vec<WxWireReceiverEventHandler>,
    telemetry: Arc<Mutex<WxWireReceiverTelemetrySnapshot>>,
    transport_factory: TransportFactory,
}

impl std::fmt::Debug for WxWireReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WxWireReceiver")
            .field("config", &self.config)
            .field("running", &self.running)
            .field("handler_count", &self.handlers.len())
            .finish()
    }
}

impl WxWireReceiver {
    /// Returns a builder for the weather wire client.
    pub fn builder(config: WxWireReceiverConfig) -> WxWireReceiverBuilder {
        WxWireReceiverBuilder::new(config)
    }

    /// Returns runtime config.
    pub fn config(&self) -> &WxWireReceiverConfig {
        &self.config
    }

    /// Adds an event handler callback.
    pub fn subscribe(&mut self, handler: WxWireReceiverEventHandler) {
        self.handlers.push(handler);
    }

    /// Returns a snapshot of current telemetry counters.
    pub fn telemetry_snapshot(&self) -> WxWireReceiverTelemetrySnapshot {
        self.telemetry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn submit_raw_stanza_internal(&self, stanza: String) -> WxWireReceiverResult<()> {
        let tx = self.ingress_tx.as_ref().ok_or_else(|| {
            WxWireReceiverError::Lifecycle("weather wire client not running".to_string())
        })?;

        tx.try_send(stanza).map_err(|err| match err {
            TrySendError::Full(_) => {
                WxWireReceiverError::Lifecycle("weather wire ingress queue full".to_string())
            }
            TrySendError::Closed(_) => {
                WxWireReceiverError::Lifecycle("weather wire ingress queue closed".to_string())
            }
        })
    }
}

impl UnstableWxWireReceiverIngress for WxWireReceiver {
    fn submit_raw_stanza(&self, stanza: String) -> WxWireReceiverResult<()> {
        self.submit_raw_stanza_internal(stanza)
    }
}

impl WxWireReceiverClient for WxWireReceiver {
    fn start(&mut self) -> WxWireReceiverResult<()> {
        if self.running {
            return Err(WxWireReceiverError::Lifecycle(
                "weather wire client already running".to_string(),
            ));
        }

        let (event_tx, event_rx) = mpsc::channel(self.config.event_channel_capacity);
        let (ingress_tx, ingress_rx) = mpsc::channel(self.config.inbound_channel_capacity);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let config = self.config.clone();
        let handlers = self.handlers.clone();
        let telemetry = Arc::clone(&self.telemetry);
        let factory = Arc::clone(&self.transport_factory);

        self.join_handle = Some(tokio::spawn(async move {
            run_weather_wire_loop(
                config,
                event_tx,
                ingress_rx,
                shutdown_rx,
                handlers,
                telemetry,
                factory,
            )
            .await;
        }));

        self.event_rx = Some(event_rx);
        self.ingress_tx = Some(ingress_tx);
        self.shutdown_tx = Some(shutdown_tx);
        self.running = true;
        Ok(())
    }

    fn stop(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = WxWireReceiverResult<()>> + Send + '_>> {
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
            self.ingress_tx = None;
            self.shutdown_tx = None;
            Ok(())
        })
    }

    fn events(
        &mut self,
    ) -> Pin<Box<dyn Stream<Item = Result<WxWireReceiverEvent, WxWireReceiverError>> + Send + '_>>
    {
        match self.event_rx.take() {
            Some(rx) => Box::pin(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|item| (item, rx))
            })),
            None => Box::pin(stream::empty()),
        }
    }
}

async fn run_weather_wire_loop(
    config: WxWireReceiverConfig,
    event_tx: mpsc::Sender<Result<WxWireReceiverEvent, WxWireReceiverError>>,
    mut ingress_rx: mpsc::Receiver<String>,
    mut shutdown_rx: watch::Receiver<bool>,
    handlers: Vec<WxWireReceiverEventHandler>,
    telemetry_sink: Arc<Mutex<WxWireReceiverTelemetrySnapshot>>,
    transport_factory: TransportFactory,
) {
    let mut telemetry = RuntimeTelemetry::default();
    telemetry.snapshot.starts_total = telemetry.snapshot.starts_total.saturating_add(1);

    let mut decoder = WxWireDecoder;
    let mut telemetry_tick =
        tokio::time::interval(Duration::from_secs(config.telemetry_emit_interval_secs));
    telemetry_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    telemetry_tick.tick().await;
    if *shutdown_rx.borrow() {
        telemetry.snapshot.stops_total = telemetry.snapshot.stops_total.saturating_add(1);
        try_send_event(
            &event_tx,
            Ok(WxWireReceiverEvent::Disconnected),
            &mut telemetry,
        );
        update_telemetry_sink(&telemetry_sink, &telemetry);
        return;
    }

    let connect_timeout = Duration::from_secs(config.connect_timeout_secs);
    let mut transport: Option<Box<dyn WxWireTransport>> = None;
    let mut last_message_time = Instant::now();
    let mut reconnect_backoff = RECONNECT_BACKOFF_INITIAL;

    loop {
        if *shutdown_rx.borrow() {
            telemetry.snapshot.stops_total = telemetry.snapshot.stops_total.saturating_add(1);
            if let Some(mut connected) = transport.take() {
                let _ = connected.disconnect().await;
            }
            try_send_event(
                &event_tx,
                Ok(WxWireReceiverEvent::Disconnected),
                &mut telemetry,
            );
            update_telemetry_sink(&telemetry_sink, &telemetry);
            return;
        }

        if transport.is_none() {
            match connect_single_endpoint(
                &transport_factory,
                config.username.clone(),
                config.password.clone(),
                connect_timeout,
                &mut telemetry,
            )
            .await
            {
                Ok(connected) => {
                    reconnect_backoff = RECONNECT_BACKOFF_INITIAL;
                    last_message_time = Instant::now();
                    let label = connected.label();
                    try_send_event(
                        &event_tx,
                        Ok(WxWireReceiverEvent::Connected(label)),
                        &mut telemetry,
                    );
                    update_telemetry_sink(&telemetry_sink, &telemetry);
                    transport = Some(connected);
                }
                Err(err) => {
                    warn!(error = %err, "wxwire connection attempt failed");
                    let warning =
                        WxWireReceiverFrameEvent::Warning(WxWireReceiverWarning::TransportError {
                            message: err.to_string(),
                        });
                    dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                    update_telemetry_sink(&telemetry_sink, &telemetry);
                    telemetry.snapshot.reconnect_attempts_total = telemetry
                        .snapshot
                        .reconnect_attempts_total
                        .saturating_add(1);
                    if wait_reconnect_backoff(&mut shutdown_rx, reconnect_backoff).await {
                        continue;
                    }
                    reconnect_backoff =
                        (reconnect_backoff.saturating_mul(2)).min(RECONNECT_BACKOFF_MAX);
                    continue;
                }
            }
        }

        let mut connected_transport = transport.take().expect("checked is_some above");

        enum NextAction {
            Stay,
            Shutdown,
            Reconnect,
        }
        let mut action = NextAction::Stay;

        {
            let next_stanza = connected_transport.next_stanza();
            tokio::pin!(next_stanza);

            tokio::select! {
                _ = shutdown_rx.changed() => {
                    telemetry.snapshot.stops_total = telemetry.snapshot.stops_total.saturating_add(1);
                    action = NextAction::Shutdown;
                }
                _ = telemetry_tick.tick() => {
                    telemetry.snapshot.telemetry_events_emitted_total = telemetry
                        .snapshot
                        .telemetry_events_emitted_total
                        .saturating_add(1);
                    try_send_event(
                        &event_tx,
                        Ok(WxWireReceiverEvent::Telemetry(telemetry.snapshot.clone())),
                        &mut telemetry,
                    );
                    update_telemetry_sink(&telemetry_sink, &telemetry);
                }
                maybe_raw = ingress_rx.recv() => {
                    if let Some(raw) = maybe_raw {
                        last_message_time = Instant::now();
                        match decoder.feed(&raw) {
                            Ok(frame_events) => {
                                telemetry.snapshot.decoded_messages_total = telemetry
                                    .snapshot
                                    .decoded_messages_total
                                    .saturating_add(1);
                                dispatch_frame_events(&event_tx, &handlers, frame_events, &mut telemetry);
                                update_telemetry_sink(&telemetry_sink, &telemetry);
                            }
                            Err(err) => {
                                let warning = WxWireReceiverFrameEvent::Warning(WxWireReceiverWarning::DecoderRecovered {
                                    error: err.to_string(),
                                });
                                dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                                decoder.reset();
                                update_telemetry_sink(&telemetry_sink, &telemetry);
                            }
                        }
                    }
                }
                transport_event = tokio::time::timeout(Duration::from_secs(1), &mut next_stanza) => {
                    match transport_event {
                        Ok(Ok(stanza)) => {
                            last_message_time = Instant::now();
                            match decoder.feed(&stanza) {
                                Ok(frame_events) => {
                                    telemetry.snapshot.decoded_messages_total = telemetry
                                        .snapshot
                                        .decoded_messages_total
                                        .saturating_add(1);
                                    dispatch_frame_events(&event_tx, &handlers, frame_events, &mut telemetry);
                                    update_telemetry_sink(&telemetry_sink, &telemetry);
                                }
                                Err(err) => {
                                    let warning = WxWireReceiverFrameEvent::Warning(WxWireReceiverWarning::DecoderRecovered {
                                        error: err.to_string(),
                                    });
                                    dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                                    decoder.reset();
                                    update_telemetry_sink(&telemetry_sink, &telemetry);
                                }
                            }
                        }
                        Ok(Err(err)) => {
                            warn!(error = %err, "wxwire transport error, reconnecting");
                            let warning = WxWireReceiverFrameEvent::Warning(WxWireReceiverWarning::TransportError {
                                message: err.to_string(),
                            });
                            dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                            update_telemetry_sink(&telemetry_sink, &telemetry);
                            action = NextAction::Reconnect;
                        }
                        Err(_) => {
                            if last_message_time.elapsed() >= Duration::from_secs(config.idle_timeout_secs) {
                                let message = format!(
                                    "no accepted room message for {}s",
                                    config.idle_timeout_secs
                                );
                                warn!(%message, "wxwire idle timeout");
                                let warning = WxWireReceiverFrameEvent::Warning(WxWireReceiverWarning::TransportError {
                                    message,
                                });
                                dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                                update_telemetry_sink(&telemetry_sink, &telemetry);
                                last_message_time = Instant::now();
                            }
                        }
                    }
                }
            }
        }

        match action {
            NextAction::Stay => {
                transport = Some(connected_transport);
            }
            NextAction::Shutdown => {
                let _ = connected_transport.disconnect().await;
                try_send_event(
                    &event_tx,
                    Ok(WxWireReceiverEvent::Disconnected),
                    &mut telemetry,
                );
                update_telemetry_sink(&telemetry_sink, &telemetry);
                return;
            }
            NextAction::Reconnect => {
                telemetry.snapshot.reconnect_attempts_total = telemetry
                    .snapshot
                    .reconnect_attempts_total
                    .saturating_add(1);
                let _ = connected_transport.disconnect().await;
                try_send_event(
                    &event_tx,
                    Ok(WxWireReceiverEvent::Disconnected),
                    &mut telemetry,
                );
                update_telemetry_sink(&telemetry_sink, &telemetry);
                if wait_reconnect_backoff(&mut shutdown_rx, reconnect_backoff).await {
                    continue;
                }
                reconnect_backoff =
                    (reconnect_backoff.saturating_mul(2)).min(RECONNECT_BACKOFF_MAX);
            }
        }
    }
}

async fn wait_reconnect_backoff(
    shutdown_rx: &mut watch::Receiver<bool>,
    duration: Duration,
) -> bool {
    tokio::select! {
        _ = shutdown_rx.changed() => true,
        _ = tokio::time::sleep(duration) => false,
    }
}

async fn connect_single_endpoint(
    factory: &TransportFactory,
    username: String,
    password: String,
    connect_timeout: Duration,
    telemetry: &mut RuntimeTelemetry,
) -> WxWireReceiverResult<Box<dyn WxWireTransport>> {
    telemetry.snapshot.connect_attempts_total =
        telemetry.snapshot.connect_attempts_total.saturating_add(1);
    factory(username, password, connect_timeout).await
}

fn default_transport_factory(
    username: String,
    password: String,
    connect_timeout: Duration,
) -> TransportFuture {
    Box::pin(async move {
        let transport = XmppWxWireTransport::connect(
            WXWIRE_PRIMARY_HOST,
            username.as_str(),
            password.as_str(),
            connect_timeout,
        )
        .await?;
        Ok(Box::new(transport) as Box<dyn WxWireTransport>)
    })
}

fn dispatch_frame_events(
    event_tx: &mpsc::Sender<Result<WxWireReceiverEvent, WxWireReceiverError>>,
    handlers: &[WxWireReceiverEventHandler],
    frame_events: Vec<WxWireReceiverFrameEvent>,
    telemetry: &mut RuntimeTelemetry,
) {
    for frame_event in frame_events {
        if matches!(frame_event, WxWireReceiverFrameEvent::File(_)) {
            telemetry.snapshot.files_emitted_total =
                telemetry.snapshot.files_emitted_total.saturating_add(1);
        }
        if matches!(frame_event, WxWireReceiverFrameEvent::Warning(_)) {
            telemetry.snapshot.warning_events_total =
                telemetry.snapshot.warning_events_total.saturating_add(1);
        }

        for handler in handlers {
            if let Err(err) = handler(&frame_event) {
                telemetry.snapshot.handler_failures_total =
                    telemetry.snapshot.handler_failures_total.saturating_add(1);
                let warning =
                    WxWireReceiverFrameEvent::Warning(WxWireReceiverWarning::HandlerError {
                        message: err.to_string(),
                    });
                telemetry.snapshot.warning_events_total =
                    telemetry.snapshot.warning_events_total.saturating_add(1);
                try_send_event(event_tx, Ok(WxWireReceiverEvent::Frame(warning)), telemetry);
            }
        }

        try_send_event(
            event_tx,
            Ok(WxWireReceiverEvent::Frame(frame_event)),
            telemetry,
        );
    }
}

fn try_send_event(
    event_tx: &mpsc::Sender<Result<WxWireReceiverEvent, WxWireReceiverError>>,
    event: Result<WxWireReceiverEvent, WxWireReceiverError>,
    telemetry: &mut RuntimeTelemetry,
) {
    try_emit_backpressure_warning(event_tx, telemetry);
    if let Err(err) = event_tx.try_send(event) {
        record_dropped_event(err, telemetry);
    }
}

fn try_emit_backpressure_warning(
    event_tx: &mpsc::Sender<Result<WxWireReceiverEvent, WxWireReceiverError>>,
    telemetry: &mut RuntimeTelemetry,
) {
    if telemetry.dropped_since_last_report == 0 {
        return;
    }

    let warning = WxWireReceiverFrameEvent::Warning(WxWireReceiverWarning::BackpressureDrop {
        dropped_since_last_report: telemetry.dropped_since_last_report,
        total_dropped_events: telemetry.snapshot.event_queue_drop_total,
    });

    match event_tx.try_send(Ok(WxWireReceiverEvent::Frame(warning))) {
        Ok(()) => {
            telemetry.snapshot.warning_events_total =
                telemetry.snapshot.warning_events_total.saturating_add(1);
            telemetry.snapshot.backpressure_warning_emitted_total = telemetry
                .snapshot
                .backpressure_warning_emitted_total
                .saturating_add(1);
            telemetry.dropped_since_last_report = 0;
        }
        Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => {}
    }
}

fn record_dropped_event(
    err: TrySendError<Result<WxWireReceiverEvent, WxWireReceiverError>>,
    telemetry: &mut RuntimeTelemetry,
) {
    if matches!(err, TrySendError::Full(_)) {
        telemetry.snapshot.event_queue_drop_total =
            telemetry.snapshot.event_queue_drop_total.saturating_add(1);
        telemetry.dropped_since_last_report = telemetry.dropped_since_last_report.saturating_add(1);
    }
}

fn update_telemetry_sink(
    sink: &Arc<Mutex<WxWireReceiverTelemetrySnapshot>>,
    telemetry: &RuntimeTelemetry,
) {
    let mut guard = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = telemetry.snapshot.clone();
}

#[cfg(test)]
mod tests {
    use super::{
        TransportFactory, UnstableWxWireReceiverIngress, WxWireReceiver, WxWireReceiverClient,
        WxWireReceiverConfig, WxWireReceiverEvent, WxWireReceiverEventHandler,
    };
    use crate::wxwire_receiver::error::WxWireReceiverError;
    use crate::wxwire_receiver::model::{WxWireReceiverFrameEvent, WxWireReceiverWarning};
    use crate::wxwire_receiver::transport::WxWireTransport;
    use futures::StreamExt;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::sync::mpsc;
    #[derive(Debug)]
    struct MockTransport {
        label: String,
        rx: mpsc::Receiver<String>,
    }

    impl WxWireTransport for MockTransport {
        fn label(&self) -> String {
            self.label.clone()
        }

        fn next_stanza<'a>(
            &'a mut self,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = crate::wxwire_receiver::error::WxWireReceiverResult<String>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.rx.recv().await.ok_or_else(|| {
                    crate::wxwire_receiver::error::WxWireReceiverError::Transport(
                        "mock stream ended".to_string(),
                    )
                })
            })
        }

        fn disconnect<'a>(
            &'a mut self,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = crate::wxwire_receiver::error::WxWireReceiverResult<()>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(()) })
        }
    }

    #[derive(Debug)]
    struct FlakyTransport {
        label: String,
        rx: mpsc::Receiver<String>,
        fail_once: bool,
    }

    impl WxWireTransport for FlakyTransport {
        fn label(&self) -> String {
            self.label.clone()
        }

        fn next_stanza<'a>(
            &'a mut self,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = crate::wxwire_receiver::error::WxWireReceiverResult<String>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                if self.fail_once {
                    self.fail_once = false;
                    return Err(
                        crate::wxwire_receiver::error::WxWireReceiverError::Transport(
                            "simulated socket failure".to_string(),
                        ),
                    );
                }
                self.rx.recv().await.ok_or_else(|| {
                    crate::wxwire_receiver::error::WxWireReceiverError::Transport(
                        "mock stream ended".to_string(),
                    )
                })
            })
        }

        fn disconnect<'a>(
            &'a mut self,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = crate::wxwire_receiver::error::WxWireReceiverResult<()>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(()) })
        }
    }

    fn valid_config() -> WxWireReceiverConfig {
        WxWireReceiverConfig {
            username: "user".to_string(),
            password: "pass".to_string(),
            idle_timeout_secs: 1,
            telemetry_emit_interval_secs: 1,
            connect_timeout_secs: 1,
            ..WxWireReceiverConfig::default()
        }
    }

    fn mock_factory() -> TransportFactory {
        Arc::new(move |_username, _password, _timeout| {
            let (tx, rx) = mpsc::channel(8);
            let stanza = "<message xmlns='jabber:client' type='groupchat'><body>S</body><x xmlns='nwws-oi' id='id1' issue='2026-03-05T00:00:00Z' ttaaii='NOUS41' cccc='KOKX' awipsid='AFDOKX'>line</x></message>";
            let _ = tx.try_send(stanza.to_string());
            let label = "primary".to_string();
            Box::pin(async move {
                Ok(Box::new(MockTransport { label, rx }) as Box<dyn WxWireTransport>)
            })
        })
    }

    #[tokio::test]
    async fn client_emits_file_frame_for_valid_message() {
        let mut client = WxWireReceiver::builder(valid_config())
            .with_transport_factory(mock_factory())
            .build()
            .expect("client should build");
        client.start().expect("client should start");

        let mut events = client.events();
        let mut saw_file = false;
        for _ in 0..12 {
            if let Ok(Some(Ok(WxWireReceiverEvent::Frame(WxWireReceiverFrameEvent::File(file))))) =
                tokio::time::timeout(Duration::from_millis(250), events.next()).await
            {
                saw_file = file.filename == "AFDOKX.txt";
                break;
            }
        }

        drop(events);
        client.stop().await.expect("stop should succeed");
        assert!(saw_file);
    }

    #[tokio::test]
    async fn handler_error_isolated() {
        let bad: WxWireReceiverEventHandler = Arc::new(|_evt: &WxWireReceiverFrameEvent| {
            Err(WxWireReceiverError::Lifecycle("boom".to_string()))
        });

        let mut client = WxWireReceiver::builder(valid_config())
            .with_transport_factory(mock_factory())
            .build()
            .expect("client should build");
        client.subscribe(bad);
        client.start().expect("client should start");

        let mut events = client.events();
        let mut saw_handler_warning = false;
        for _ in 0..12 {
            if let Ok(Some(Ok(WxWireReceiverEvent::Frame(WxWireReceiverFrameEvent::Warning(
                WxWireReceiverWarning::HandlerError { .. },
            ))))) = tokio::time::timeout(Duration::from_millis(250), events.next()).await
            {
                saw_handler_warning = true;
                break;
            }
        }

        drop(events);
        client.stop().await.expect("stop should succeed");
        assert!(saw_handler_warning);
    }

    #[tokio::test]
    async fn unstable_raw_ingress_works() {
        let mut client = WxWireReceiver::builder(valid_config())
            .with_transport_factory(mock_factory())
            .build()
            .expect("client should build");
        client.start().expect("client should start");

        client
            .submit_raw_stanza("<message xmlns='jabber:client' type='groupchat'><body>S2</body><x xmlns='nwws-oi' id='id2' issue='2026-03-05T00:00:00Z' ttaaii='NOUS41' cccc='KOKX' awipsid='AFDOKX'>line</x></message>".to_string())
            .expect("submit should succeed");

        let mut events = client.events();
        let mut saw_file = false;
        for _ in 0..20 {
            if let Ok(Some(Ok(WxWireReceiverEvent::Frame(WxWireReceiverFrameEvent::File(_))))) =
                tokio::time::timeout(Duration::from_millis(250), events.next()).await
            {
                saw_file = true;
                break;
            }
        }

        drop(events);
        client.stop().await.expect("stop should succeed");
        assert!(saw_file);
    }

    #[tokio::test]
    async fn initial_connect_failure_retries_and_recovers() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_factory = Arc::clone(&attempts);
        let factory: TransportFactory = Arc::new(move |_username, _password, _timeout| {
            let current = attempts_for_factory.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                if current == 0 {
                    return Err(WxWireReceiverError::Transport(
                        "initial connect failure".to_string(),
                    ));
                }
                let (tx, rx) = mpsc::channel(8);
                let stanza = "<message xmlns='jabber:client' type='groupchat'><body>S</body><x xmlns='nwws-oi' id='id1' issue='2026-03-05T00:00:00Z' ttaaii='NOUS41' cccc='KOKX' awipsid='AFDOKX'>line</x></message>";
                let _ = tx.try_send(stanza.to_string());
                Ok(Box::new(MockTransport {
                    label: "recovered".to_string(),
                    rx,
                }) as Box<dyn WxWireTransport>)
            })
        });

        let mut client = WxWireReceiver::builder(valid_config())
            .with_transport_factory(factory)
            .build()
            .expect("client should build");
        client.start().expect("client should start");

        let mut events = client.events();
        let mut saw_connected = false;
        for _ in 0..40 {
            if let Ok(Some(Ok(WxWireReceiverEvent::Connected(label)))) =
                tokio::time::timeout(Duration::from_millis(100), events.next()).await
                && label == "recovered"
            {
                saw_connected = true;
                break;
            }
        }

        drop(events);
        client.stop().await.expect("stop should succeed");
        assert!(saw_connected);
        assert!(attempts.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn transport_error_emits_disconnected_and_reconnects() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_factory = Arc::clone(&attempts);
        let factory: TransportFactory = Arc::new(move |_username, _password, _timeout| {
            let current = attempts_for_factory.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let (tx, rx) = mpsc::channel(8);
                let stanza = "<message xmlns='jabber:client' type='groupchat'><body>S</body><x xmlns='nwws-oi' id='id1' issue='2026-03-05T00:00:00Z' ttaaii='NOUS41' cccc='KOKX' awipsid='AFDOKX'>line</x></message>";
                let _ = tx.try_send(stanza.to_string());
                if current == 0 {
                    Ok(Box::new(FlakyTransport {
                        label: "flaky".to_string(),
                        rx,
                        fail_once: true,
                    }) as Box<dyn WxWireTransport>)
                } else {
                    Ok(Box::new(MockTransport {
                        label: "reconnected".to_string(),
                        rx,
                    }) as Box<dyn WxWireTransport>)
                }
            })
        });

        let mut client = WxWireReceiver::builder(valid_config())
            .with_transport_factory(factory)
            .build()
            .expect("client should build");
        client.start().expect("client should start");

        let mut events = client.events();
        let mut saw_disconnected = false;
        let mut saw_reconnected = false;
        for _ in 0..60 {
            if let Ok(Some(Ok(event))) =
                tokio::time::timeout(Duration::from_millis(100), events.next()).await
            {
                match event {
                    WxWireReceiverEvent::Disconnected => {
                        saw_disconnected = true;
                    }
                    WxWireReceiverEvent::Connected(label) if label == "reconnected" => {
                        saw_reconnected = true;
                        break;
                    }
                    _ => {}
                }
            }
        }

        drop(events);
        client.stop().await.expect("stop should succeed");
        assert!(saw_disconnected);
        assert!(saw_reconnected);
        assert!(attempts.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn failed_backpressure_warning_send_does_not_increment_drop_total() {
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send(Ok(WxWireReceiverEvent::Disconnected))
            .expect("channel should accept initial event");

        let mut telemetry = super::RuntimeTelemetry::default();
        telemetry.snapshot.event_queue_drop_total = 7;
        telemetry.dropped_since_last_report = 3;

        super::try_emit_backpressure_warning(&tx, &mut telemetry);

        assert_eq!(telemetry.snapshot.event_queue_drop_total, 7);
        assert_eq!(telemetry.dropped_since_last_report, 3);

        let queued = rx.try_recv().expect("original event should remain");
        assert!(matches!(queued, Ok(WxWireReceiverEvent::Disconnected)));
    }
}
