use crate::error::{CoreError, CoreResult};
use crate::weather_wire::codec::{WxWireDecoder, WxWireFrameDecoder};
use crate::weather_wire::config::{
    WXWIRE_MAX_BACKOFF_SECS, WXWIRE_MIN_BACKOFF_SECS, WXWIRE_PRIMARY_HOST, WxWireConfig,
};
use crate::weather_wire::error::WeatherWireResult;
use crate::weather_wire::model::{WeatherWireFrameEvent, WeatherWireWarning};
use crate::weather_wire::transport::{WxWireTransport, XmppWxWireTransport};
use futures::{Stream, stream};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};

/// Snapshot of weather wire runtime telemetry counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "telemetry-serde",
    derive(serde::Serialize, serde::Deserialize)
)]
#[non_exhaustive]
pub struct WxWireTelemetrySnapshot {
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
    snapshot: WxWireTelemetrySnapshot,
    dropped_since_last_report: u64,
}

/// Events emitted by the weather wire client.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WxWireClientEvent {
    /// Frame-level event (file or warning).
    Frame(WeatherWireFrameEvent),
    /// Connected endpoint label.
    Connected(String),
    /// Disconnected from endpoint.
    Disconnected,
    /// Periodic telemetry snapshot.
    Telemetry(WxWireTelemetrySnapshot),
}

/// Weather wire event handler callback type.
pub type WxWireEventHandler = Arc<dyn Fn(&WeatherWireFrameEvent) -> CoreResult<()> + Send + Sync>;

type TransportFuture =
    Pin<Box<dyn Future<Output = WeatherWireResult<Box<dyn WxWireTransport>>> + Send>>;
type TransportFactory = Arc<dyn Fn(String, String, Duration) -> TransportFuture + Send + Sync>;

/// Trait for weather wire clients.
pub trait WeatherWireClient: Send {
    /// Starts the runtime loop.
    fn start(&mut self) -> CoreResult<()>;

    /// Stops the runtime loop.
    fn stop(&mut self) -> Pin<Box<dyn std::future::Future<Output = CoreResult<()>> + Send + '_>>;

    /// Returns a stream of runtime events.
    fn events(
        &mut self,
    ) -> Pin<Box<dyn Stream<Item = Result<WxWireClientEvent, CoreError>> + Send + '_>>;
}

/// Unstable ingress surface for raw stanza injection.
pub trait UnstableWxWireIngress {
    /// Submits one raw XMPP stanza string to the runtime decoder.
    fn submit_raw_stanza(&self, stanza: String) -> CoreResult<()>;
}

/// Builder for validated weather wire client construction.
#[derive(Clone)]
pub struct WxWireClientBuilder {
    config: WxWireConfig,
    transport_factory: TransportFactory,
}

impl std::fmt::Debug for WxWireClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WxWireClientBuilder")
            .field("config", &self.config)
            .finish()
    }
}

impl WxWireClientBuilder {
    /// Creates a new builder.
    pub fn new(config: WxWireConfig) -> Self {
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
    pub fn build(self) -> Result<WxWireClientImpl, CoreError> {
        self.config.validate().map_err(CoreError::from)?;
        Ok(WxWireClientImpl {
            config: self.config,
            running: false,
            event_rx: None,
            shutdown_tx: None,
            join_handle: None,
            ingress_tx: None,
            handlers: Vec::new(),
            telemetry: Arc::new(Mutex::new(WxWireTelemetrySnapshot::default())),
            transport_factory: self.transport_factory,
        })
    }
}

/// Weather wire client runtime.
pub struct WxWireClientImpl {
    config: WxWireConfig,
    running: bool,
    event_rx: Option<mpsc::Receiver<Result<WxWireClientEvent, CoreError>>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    join_handle: Option<tokio::task::JoinHandle<()>>,
    ingress_tx: Option<mpsc::Sender<String>>,
    handlers: Vec<WxWireEventHandler>,
    telemetry: Arc<Mutex<WxWireTelemetrySnapshot>>,
    transport_factory: TransportFactory,
}

impl std::fmt::Debug for WxWireClientImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WxWireClientImpl")
            .field("config", &self.config)
            .field("running", &self.running)
            .field("handler_count", &self.handlers.len())
            .finish()
    }
}

impl WxWireClientImpl {
    /// Returns a builder for the weather wire client.
    pub fn builder(config: WxWireConfig) -> WxWireClientBuilder {
        WxWireClientBuilder::new(config)
    }

    /// Returns runtime config.
    pub fn config(&self) -> &WxWireConfig {
        &self.config
    }

    /// Adds an event handler callback.
    pub fn subscribe(&mut self, handler: WxWireEventHandler) {
        self.handlers.push(handler);
    }

    /// Returns a snapshot of current telemetry counters.
    pub fn telemetry_snapshot(&self) -> WxWireTelemetrySnapshot {
        self.telemetry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn submit_raw_stanza_internal(&self, stanza: String) -> CoreResult<()> {
        let tx = self
            .ingress_tx
            .as_ref()
            .ok_or_else(|| CoreError::Lifecycle("weather wire client not running".to_string()))?;

        tx.try_send(stanza).map_err(|err| match err {
            TrySendError::Full(_) => {
                CoreError::Lifecycle("weather wire ingress queue full".to_string())
            }
            TrySendError::Closed(_) => {
                CoreError::Lifecycle("weather wire ingress queue closed".to_string())
            }
        })
    }
}

impl UnstableWxWireIngress for WxWireClientImpl {
    fn submit_raw_stanza(&self, stanza: String) -> CoreResult<()> {
        self.submit_raw_stanza_internal(stanza)
    }
}

impl WeatherWireClient for WxWireClientImpl {
    fn start(&mut self) -> CoreResult<()> {
        if self.running {
            return Err(CoreError::Lifecycle(
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
            self.ingress_tx = None;
            self.shutdown_tx = None;
            Ok(())
        })
    }

    fn events(
        &mut self,
    ) -> Pin<Box<dyn Stream<Item = Result<WxWireClientEvent, CoreError>> + Send + '_>> {
        match self.event_rx.take() {
            Some(rx) => Box::pin(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|item| (item, rx))
            })),
            None => Box::pin(stream::empty()),
        }
    }
}

async fn run_weather_wire_loop(
    config: WxWireConfig,
    event_tx: mpsc::Sender<Result<WxWireClientEvent, CoreError>>,
    mut ingress_rx: mpsc::Receiver<String>,
    mut shutdown_rx: watch::Receiver<bool>,
    handlers: Vec<WxWireEventHandler>,
    telemetry_sink: Arc<Mutex<WxWireTelemetrySnapshot>>,
    transport_factory: TransportFactory,
) {
    let mut telemetry = RuntimeTelemetry::default();
    telemetry.snapshot.starts_total = telemetry.snapshot.starts_total.saturating_add(1);

    let mut decoder = WxWireDecoder;
    let mut telemetry_tick =
        tokio::time::interval(Duration::from_secs(config.telemetry_emit_interval_secs));
    telemetry_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    telemetry_tick.tick().await;
    let mut consecutive_failures = 0u32;

    'outer: loop {
        if *shutdown_rx.borrow() {
            telemetry.snapshot.stops_total = telemetry.snapshot.stops_total.saturating_add(1);
            try_send_event(
                &event_tx,
                Ok(WxWireClientEvent::Disconnected),
                &mut telemetry,
            );
            update_telemetry_sink(&telemetry_sink, &telemetry);
            return;
        }

        telemetry.snapshot.reconnect_attempts_total = telemetry
            .snapshot
            .reconnect_attempts_total
            .saturating_add(1);

        let connect_timeout = Duration::from_secs(config.connect_timeout_secs);
        let connect = connect_single_endpoint(
            &transport_factory,
            config.username.clone(),
            config.password.clone(),
            connect_timeout,
            &mut telemetry,
        )
        .await;

        let mut transport = match connect {
            Ok(transport) => transport,
            Err(err) => {
                let warning = WeatherWireFrameEvent::Warning(WeatherWireWarning::TransportError {
                    message: err.to_string(),
                });
                dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                update_telemetry_sink(&telemetry_sink, &telemetry);
                consecutive_failures = consecutive_failures.saturating_add(1);
                let delay = next_backoff_secs(consecutive_failures);
                tokio::time::sleep(Duration::from_secs(delay)).await;
                continue;
            }
        };
        consecutive_failures = 0;

        try_send_event(
            &event_tx,
            Ok(WxWireClientEvent::Connected(transport.label())),
            &mut telemetry,
        );
        update_telemetry_sink(&telemetry_sink, &telemetry);

        let mut last_message_time = Instant::now();

        loop {
            if *shutdown_rx.borrow() {
                telemetry.snapshot.stops_total = telemetry.snapshot.stops_total.saturating_add(1);
                let _ = transport.disconnect().await;
                try_send_event(
                    &event_tx,
                    Ok(WxWireClientEvent::Disconnected),
                    &mut telemetry,
                );
                update_telemetry_sink(&telemetry_sink, &telemetry);
                return;
            }

            enum NextAction {
                Stay,
                Reconnect,
                Shutdown,
            }

            let action = {
                let next_message = transport.next_message();
                tokio::pin!(next_message);

                let mut action = NextAction::Stay;
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
                            Ok(WxWireClientEvent::Telemetry(telemetry.snapshot.clone())),
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
                                    let warning = WeatherWireFrameEvent::Warning(WeatherWireWarning::DecoderRecovered {
                                        error: err.to_string(),
                                    });
                                    dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                                    decoder.reset();
                                    update_telemetry_sink(&telemetry_sink, &telemetry);
                                }
                            }
                        }
                    }
                    transport_event = tokio::time::timeout(Duration::from_secs(1), &mut next_message) => {
                        match transport_event {
                            Ok(Ok(message)) => {
                                last_message_time = Instant::now();
                                match decoder.feed_message(&message) {
                                    Ok(frame_events) => {
                                        telemetry.snapshot.decoded_messages_total = telemetry
                                            .snapshot
                                            .decoded_messages_total
                                            .saturating_add(1);
                                        dispatch_frame_events(&event_tx, &handlers, frame_events, &mut telemetry);
                                        update_telemetry_sink(&telemetry_sink, &telemetry);
                                    }
                                    Err(err) => {
                                        let warning = WeatherWireFrameEvent::Warning(WeatherWireWarning::DecoderRecovered {
                                            error: err.to_string(),
                                        });
                                        dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                                        decoder.reset();
                                        update_telemetry_sink(&telemetry_sink, &telemetry);
                                    }
                                }
                            }
                            Ok(Err(err)) => {
                                let warning = WeatherWireFrameEvent::Warning(WeatherWireWarning::TransportError {
                                    message: err.to_string(),
                                });
                                dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                                action = NextAction::Reconnect;
                            }
                            Err(_) => {
                                if last_message_time.elapsed() >= Duration::from_secs(config.idle_timeout_secs) {
                                    telemetry.snapshot.idle_reconnects_total = telemetry
                                        .snapshot
                                        .idle_reconnects_total
                                        .saturating_add(1);
                                    let warning = WeatherWireFrameEvent::Warning(WeatherWireWarning::IdleTimeoutReconnect);
                                    dispatch_frame_events(&event_tx, &handlers, vec![warning], &mut telemetry);
                                    action = NextAction::Reconnect;
                                }
                            }
                        }
                    }
                }
                action
            };

            match action {
                NextAction::Stay => {}
                NextAction::Shutdown => {
                    let _ = transport.disconnect().await;
                    try_send_event(
                        &event_tx,
                        Ok(WxWireClientEvent::Disconnected),
                        &mut telemetry,
                    );
                    update_telemetry_sink(&telemetry_sink, &telemetry);
                    return;
                }
                NextAction::Reconnect => {
                    let _ = transport.disconnect().await;
                    try_send_event(
                        &event_tx,
                        Ok(WxWireClientEvent::Disconnected),
                        &mut telemetry,
                    );
                    update_telemetry_sink(&telemetry_sink, &telemetry);
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    let delay = next_backoff_secs(consecutive_failures);
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    continue 'outer;
                }
            }
        }
    }
}

async fn connect_single_endpoint(
    factory: &TransportFactory,
    username: String,
    password: String,
    connect_timeout: Duration,
    telemetry: &mut RuntimeTelemetry,
) -> WeatherWireResult<Box<dyn WxWireTransport>> {
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

fn next_backoff_secs(consecutive_failures: u32) -> u64 {
    let exp = consecutive_failures.saturating_sub(1).min(16);
    let scaled = WXWIRE_MIN_BACKOFF_SECS.saturating_mul(1u64 << exp);
    scaled.clamp(WXWIRE_MIN_BACKOFF_SECS, WXWIRE_MAX_BACKOFF_SECS)
}

fn dispatch_frame_events(
    event_tx: &mpsc::Sender<Result<WxWireClientEvent, CoreError>>,
    handlers: &[WxWireEventHandler],
    frame_events: Vec<WeatherWireFrameEvent>,
    telemetry: &mut RuntimeTelemetry,
) {
    for frame_event in frame_events {
        if matches!(frame_event, WeatherWireFrameEvent::File(_)) {
            telemetry.snapshot.files_emitted_total =
                telemetry.snapshot.files_emitted_total.saturating_add(1);
        }
        if matches!(frame_event, WeatherWireFrameEvent::Warning(_)) {
            telemetry.snapshot.warning_events_total =
                telemetry.snapshot.warning_events_total.saturating_add(1);
        }

        for handler in handlers {
            if let Err(err) = handler(&frame_event) {
                telemetry.snapshot.handler_failures_total =
                    telemetry.snapshot.handler_failures_total.saturating_add(1);
                let warning = WeatherWireFrameEvent::Warning(WeatherWireWarning::HandlerError {
                    message: err.to_string(),
                });
                telemetry.snapshot.warning_events_total =
                    telemetry.snapshot.warning_events_total.saturating_add(1);
                try_send_event(event_tx, Ok(WxWireClientEvent::Frame(warning)), telemetry);
            }
        }

        try_send_event(
            event_tx,
            Ok(WxWireClientEvent::Frame(frame_event)),
            telemetry,
        );
    }
}

fn try_send_event(
    event_tx: &mpsc::Sender<Result<WxWireClientEvent, CoreError>>,
    event: Result<WxWireClientEvent, CoreError>,
    telemetry: &mut RuntimeTelemetry,
) {
    try_emit_backpressure_warning(event_tx, telemetry);
    if let Err(err) = event_tx.try_send(event) {
        record_dropped_event(err, telemetry);
    }
}

fn try_emit_backpressure_warning(
    event_tx: &mpsc::Sender<Result<WxWireClientEvent, CoreError>>,
    telemetry: &mut RuntimeTelemetry,
) {
    if telemetry.dropped_since_last_report == 0 {
        return;
    }

    let warning = WeatherWireFrameEvent::Warning(WeatherWireWarning::BackpressureDrop {
        dropped_since_last_report: telemetry.dropped_since_last_report,
        total_dropped_events: telemetry.snapshot.event_queue_drop_total,
    });

    match event_tx.try_send(Ok(WxWireClientEvent::Frame(warning))) {
        Ok(()) => {
            telemetry.snapshot.warning_events_total =
                telemetry.snapshot.warning_events_total.saturating_add(1);
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
    err: TrySendError<Result<WxWireClientEvent, CoreError>>,
    telemetry: &mut RuntimeTelemetry,
) {
    if matches!(err, TrySendError::Full(_)) {
        telemetry.snapshot.event_queue_drop_total =
            telemetry.snapshot.event_queue_drop_total.saturating_add(1);
        telemetry.dropped_since_last_report = telemetry.dropped_since_last_report.saturating_add(1);
    }
}

fn update_telemetry_sink(sink: &Arc<Mutex<WxWireTelemetrySnapshot>>, telemetry: &RuntimeTelemetry) {
    let mut guard = sink.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = telemetry.snapshot.clone();
}

#[cfg(test)]
mod tests {
    use super::{
        TransportFactory, UnstableWxWireIngress, WeatherWireClient, WxWireClientEvent,
        WxWireClientImpl, WxWireConfig, WxWireEventHandler,
    };
    use crate::error::CoreError;
    use crate::weather_wire::model::{WeatherWireFrameEvent, WeatherWireWarning};
    use crate::weather_wire::transport::WxWireTransport;
    use futures::StreamExt;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio_xmpp::parsers::message::Message;

    #[derive(Debug)]
    struct MockTransport {
        label: String,
        rx: mpsc::Receiver<Message>,
    }

    impl WxWireTransport for MockTransport {
        fn label(&self) -> String {
            self.label.clone()
        }

        fn next_message<'a>(
            &'a mut self,
        ) -> Pin<
            Box<
                dyn std::future::Future<
                        Output = crate::weather_wire::error::WeatherWireResult<Message>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async move {
                self.rx.recv().await.ok_or_else(|| {
                    crate::weather_wire::error::WeatherWireError::Transport(
                        "mock stream ended".to_string(),
                    )
                })
            })
        }

        fn disconnect<'a>(
            &'a mut self,
        ) -> Pin<
            Box<
                dyn std::future::Future<Output = crate::weather_wire::error::WeatherWireResult<()>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(()) })
        }
    }

    fn valid_config() -> WxWireConfig {
        WxWireConfig {
            username: "user".to_string(),
            password: "pass".to_string(),
            idle_timeout_secs: 1,
            telemetry_emit_interval_secs: 1,
            connect_timeout_secs: 1,
            ..WxWireConfig::default()
        }
    }

    fn mock_factory() -> TransportFactory {
        Arc::new(move |_username, _password, _timeout| {
            let (tx, rx) = mpsc::channel(8);
            let stanza = "<message xmlns='jabber:client' type='groupchat'><body>S</body><x xmlns='nwws-oi' id='id1' issue='2026-03-05T00:00:00Z' ttaaii='NOUS41' cccc='KOKX' awipsid='AFDOKX'>line</x></message>";
            let elem: tokio_xmpp::minidom::Element = stanza.parse().expect("valid xml");
            let msg = Message::try_from(elem).expect("valid message");
            let _ = tx.try_send(msg);
            let label = "primary".to_string();
            Box::pin(async move {
                Ok(Box::new(MockTransport { label, rx }) as Box<dyn WxWireTransport>)
            })
        })
    }

    #[tokio::test]
    async fn client_emits_file_frame_for_valid_message() {
        let mut client = WxWireClientImpl::builder(valid_config())
            .with_transport_factory(mock_factory())
            .build()
            .expect("client should build");
        client.start().expect("client should start");

        let mut events = client.events();
        let mut saw_file = false;
        for _ in 0..12 {
            if let Ok(Some(Ok(WxWireClientEvent::Frame(WeatherWireFrameEvent::File(file))))) =
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
        let bad: WxWireEventHandler =
            Arc::new(|_evt: &WeatherWireFrameEvent| Err(CoreError::Lifecycle("boom".to_string())));

        let mut client = WxWireClientImpl::builder(valid_config())
            .with_transport_factory(mock_factory())
            .build()
            .expect("client should build");
        client.subscribe(bad);
        client.start().expect("client should start");

        let mut events = client.events();
        let mut saw_handler_warning = false;
        for _ in 0..12 {
            if let Ok(Some(Ok(WxWireClientEvent::Frame(WeatherWireFrameEvent::Warning(
                WeatherWireWarning::HandlerError { .. },
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
        let mut client = WxWireClientImpl::builder(valid_config())
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
            if let Ok(Some(Ok(WxWireClientEvent::Frame(WeatherWireFrameEvent::File(_))))) =
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

    #[test]
    fn backoff_is_bounded_to_policy_window() {
        assert_eq!(super::next_backoff_secs(0), 5);
        assert_eq!(super::next_backoff_secs(1), 5);
        assert_eq!(super::next_backoff_secs(2), 10);
        assert_eq!(super::next_backoff_secs(3), 20);
        assert_eq!(super::next_backoff_secs(20), 300);
    }
}
