use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumPolicy {
    StrictDrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V2CompressionPolicy {
    RequireZlibHeader,
    TryAlways,
}

#[derive(Debug, Clone)]
pub struct DecodeConfig {
    pub checksum_policy: ChecksumPolicy,
    pub compression_policy: V2CompressionPolicy,
    pub max_v2_body_size: usize,
}

impl Default for DecodeConfig {
    fn default() -> Self {
        Self {
            checksum_policy: ChecksumPolicy::StrictDrop,
            compression_policy: V2CompressionPolicy::RequireZlibHeader,
            max_v2_body_size: 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub email: String,
    pub servers: Vec<(String, u16)>,
    pub server_list_path: Option<PathBuf>,
    pub reconnect_delay_secs: u64,
    pub connection_timeout_secs: u64,
    pub watchdog_timeout_secs: u64,
    pub max_exceptions: u32,
    pub decode: DecodeConfig,
}

impl ClientConfig {
    pub fn validate(&self) -> Result<(), crate::error::ConfigError> {
        if self.email.trim().is_empty() {
            return Err(crate::error::ConfigError::EmptyEmail);
        }
        if self.servers.is_empty() {
            return Err(crate::error::ConfigError::NoServers);
        }
        Ok(())
    }
}
