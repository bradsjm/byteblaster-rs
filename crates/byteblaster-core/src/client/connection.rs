use std::io;
use std::time::Duration;
use tokio::net::TcpStream;

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

pub fn endpoint_label(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}
