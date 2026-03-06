use thiserror::Error;

/// Result alias for weather wire components.
pub type WxWireReceiverResult<T> = Result<T, WxWireReceiverError>;

/// Errors emitted by weather wire decoding and runtime components.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WxWireReceiverError {
    #[error(transparent)]
    Config(#[from] WxWireConfigError),
    #[error(transparent)]
    Decode(#[from] WxWireDecodeError),
    #[error(transparent)]
    Lifecycle(#[from] WxWireLifecycleError),
    #[error("weather wire transport error: {0}")]
    Transport(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum WxWireConfigError {
    #[error("username must not be empty")]
    EmptyUsername,
    #[error("password must not be empty")]
    EmptyPassword,
    #[error("idle_timeout_secs must be >= 1")]
    ZeroIdleTimeout,
    #[error("event_channel_capacity must be >= 1")]
    ZeroEventChannelCapacity,
    #[error("inbound_channel_capacity must be >= 1")]
    ZeroInboundChannelCapacity,
    #[error("telemetry_emit_interval_secs must be >= 1")]
    ZeroTelemetryEmitInterval,
    #[error("connect_timeout_secs must be >= 1")]
    ZeroConnectTimeout,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum WxWireDecodeError {
    #[error("invalid xml stanza: {0}")]
    InvalidXml(String),
    #[error("not an xmpp <message/> stanza")]
    NotMessageStanza,
    #[error("missing nwws-oi payload")]
    MissingPayload,
    #[error("weather wire payload is empty")]
    EmptyPayload,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum WxWireLifecycleError {
    #[error("weather wire client already running")]
    AlreadyRunning,
    #[error("weather wire client not running")]
    NotRunning,
    #[error("weather wire ingress queue full")]
    IngressQueueFull,
    #[error("weather wire ingress queue closed")]
    IngressQueueClosed,
    #[error("weather wire event stream already taken")]
    EventStreamTaken,
    #[error("{0}")]
    Internal(String),
}
