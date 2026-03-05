use crate::weather_wire::error::WeatherWireError;

/// Primary NWWS-OI endpoint hostname.
pub const WXWIRE_PRIMARY_HOST: &str = "nwws-oi.weather.gov";
/// NWWS-OI XMPP port.
pub const WXWIRE_PORT: u16 = 5222;
/// Fixed MUC room name.
pub const WXWIRE_ROOM: &str = "nwws@conference.nwws-oi.weather.gov";
/// Minimum reconnect backoff delay in seconds.
pub const WXWIRE_MIN_BACKOFF_SECS: u64 = 5;
/// Maximum reconnect backoff delay in seconds.
pub const WXWIRE_MAX_BACKOFF_SECS: u64 = 300;

/// Runtime configuration for Weather Wire.
#[derive(Debug, Clone)]
pub struct WxWireConfig {
    /// NWWS-OI username.
    pub username: String,
    /// NWWS-OI password.
    pub password: String,
    /// Idle timeout window before reconnect in seconds.
    pub idle_timeout_secs: u64,
    /// Capacity of the event channel.
    pub event_channel_capacity: usize,
    /// Capacity of the inbound stanza channel.
    pub inbound_channel_capacity: usize,
    /// Interval for telemetry emission in seconds.
    pub telemetry_emit_interval_secs: u64,
    /// Timeout for establishing the XMPP connection and session.
    pub connect_timeout_secs: u64,
}

impl Default for WxWireConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            idle_timeout_secs: 90,
            event_channel_capacity: 1024,
            inbound_channel_capacity: 512,
            telemetry_emit_interval_secs: 5,
            connect_timeout_secs: 10,
        }
    }
}

impl WxWireConfig {
    /// Validates configuration fields.
    pub fn validate(&self) -> Result<(), WeatherWireError> {
        if self.username.trim().is_empty() {
            return Err(WeatherWireError::InvalidConfig(
                "username must not be empty".to_string(),
            ));
        }

        if self.password.trim().is_empty() {
            return Err(WeatherWireError::InvalidConfig(
                "password must not be empty".to_string(),
            ));
        }

        if self.idle_timeout_secs == 0 {
            return Err(WeatherWireError::InvalidConfig(
                "idle_timeout_secs must be >= 1".to_string(),
            ));
        }

        if self.event_channel_capacity == 0 {
            return Err(WeatherWireError::InvalidConfig(
                "event_channel_capacity must be >= 1".to_string(),
            ));
        }

        if self.inbound_channel_capacity == 0 {
            return Err(WeatherWireError::InvalidConfig(
                "inbound_channel_capacity must be >= 1".to_string(),
            ));
        }

        if self.telemetry_emit_interval_secs == 0 {
            return Err(WeatherWireError::InvalidConfig(
                "telemetry_emit_interval_secs must be >= 1".to_string(),
            ));
        }
        if self.connect_timeout_secs == 0 {
            return Err(WeatherWireError::InvalidConfig(
                "connect_timeout_secs must be >= 1".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::WxWireConfig;

    #[test]
    fn validate_rejects_missing_credentials() {
        let cfg = WxWireConfig::default();
        assert!(cfg.validate().is_err());

        let cfg = WxWireConfig {
            username: "user".to_string(),
            ..WxWireConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_accepts_valid_config() {
        let cfg = WxWireConfig {
            username: "user".to_string(),
            password: "pass".to_string(),
            ..WxWireConfig::default()
        };

        assert!(cfg.validate().is_ok());
    }
}
