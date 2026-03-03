//! TCP connection utilities for the ByteBlaster client.
//!
//! This module provides connection helpers with timeout support.

use std::io;
use std::time::Duration;
use tokio::net::TcpStream;

/// Connects to a host:port with a specified timeout.
///
/// # Arguments
///
/// * `host` - The hostname or IP address to connect to
/// * `port` - The port number
/// * `timeout` - Maximum time to wait for the connection
///
/// # Returns
///
/// A connected TcpStream on success, or an IO error on failure
///
/// # Errors
///
/// Returns an error if the connection fails or times out.
pub async fn connect_with_timeout(
    host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<TcpStream> {
    let addr = format!("{host}:{port}");
    match tokio::time::timeout(timeout, TcpStream::connect(addr)).await {
        Ok(res) => res,
        Err(_elapsed) => Err(io::Error::new(io::ErrorKind::TimedOut, "connect timeout")),
    }
}

/// Creates a display label for an endpoint.
///
/// # Arguments
///
/// * `host` - The hostname
/// * `port` - The port number
///
/// # Returns
///
/// A formatted string like "host:port"
pub fn endpoint_label(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}
