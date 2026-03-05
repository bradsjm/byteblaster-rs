use thiserror::Error;

/// Result alias for weather wire components.
pub type WeatherWireResult<T> = Result<T, WeatherWireError>;

/// Errors emitted by weather wire decoding and runtime components.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WeatherWireError {
    /// Invalid weather wire configuration.
    #[error("invalid weather wire config: {0}")]
    InvalidConfig(String),
    /// Stanza could not be parsed.
    #[error("invalid weather wire stanza: {0}")]
    InvalidStanza(String),
    /// Runtime lifecycle operation failed.
    #[error("weather wire lifecycle error: {0}")]
    Lifecycle(String),
    /// XMPP transport layer failure.
    #[error("weather wire transport error: {0}")]
    Transport(String),
}
