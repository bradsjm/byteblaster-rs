use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use byteblaster_core::unstable::{build_logon_message, xor_ff};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};

struct RelayProcess {
    child: Child,
}

impl RelayProcess {
    fn spawn(args: &[String]) -> Self {
        let bin = relay_binary_path();
        let child = Command::new(bin)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn relay process");
        Self { child }
    }
}

fn relay_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_byteblaster-relay") {
        return PathBuf::from(path);
    }

    static BIN_PATH: OnceLock<PathBuf> = OnceLock::new();
    BIN_PATH
        .get_or_init(|| {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let workspace_root = manifest_dir
                .parent()
                .and_then(|p| p.parent())
                .expect("workspace root from manifest dir");
            let bin_path = workspace_root
                .join("target")
                .join("debug")
                .join(if cfg!(windows) {
                    "byteblaster-relay.exe"
                } else {
                    "byteblaster-relay"
                });

            if !bin_path.exists() {
                let status = Command::new("cargo")
                    .arg("build")
                    .arg("-p")
                    .arg("byteblaster-relay")
                    .arg("--bin")
                    .arg("byteblaster-relay")
                    .status()
                    .expect("failed to build relay binary for integration tests");
                assert!(status.success(), "cargo build for relay binary failed");
            }

            bin_path
        })
        .clone()
}

impl Drop for RelayProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct UpstreamHarness {
    addr: SocketAddr,
    send_tx: mpsc::UnboundedSender<Vec<u8>>,
    ready_rx: Option<oneshot::Receiver<()>>,
}

impl UpstreamHarness {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind upstream listener");
        let addr = listener.local_addr().expect("upstream local addr");
        let (send_tx, mut send_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (ready_tx, ready_rx) = oneshot::channel();

        tokio::spawn(async move {
            let accept = tokio::time::timeout(Duration::from_secs(8), listener.accept()).await;
            let Ok(Ok((mut stream, _peer))) = accept else {
                return;
            };
            let _ = ready_tx.send(());

            let mut read_buf = [0_u8; 2048];
            loop {
                tokio::select! {
                    read = stream.read(&mut read_buf) => {
                        match read {
                            Ok(0) | Err(_) => return,
                            Ok(_) => {}
                        }
                    }
                    maybe_payload = send_rx.recv() => {
                        match maybe_payload {
                            Some(payload) => {
                                if stream.write_all(&payload).await.is_err() {
                                    return;
                                }
                            }
                            None => return,
                        }
                    }
                }
            }
        });

        Self {
            addr,
            send_tx,
            ready_rx: Some(ready_rx),
        }
    }

    async fn wait_ready(&mut self) {
        if let Some(ready_rx) = self.ready_rx.take() {
            let _ = tokio::time::timeout(Duration::from_secs(8), ready_rx).await;
        }
    }
}

#[tokio::test]
async fn disconnects_client_without_periodic_reauth() {
    let mut upstream = UpstreamHarness::start().await;
    let relay_addr = free_addr().await;
    let metrics_addr = free_addr().await;

    let relay = RelayProcess::spawn(&relay_args(
        upstream.addr,
        relay_addr,
        metrics_addr,
        10,
        1,
        65_536,
    ));

    wait_for_port(metrics_addr, Duration::from_secs(8)).await;
    upstream.wait_ready().await;

    let mut client = TcpStream::connect(relay_addr)
        .await
        .expect("connect relay client");
    send_auth(&mut client, "downstream@example.com").await;

    tokio::time::sleep(Duration::from_secs(3)).await;

    let mut buf = [0_u8; 1];
    let read = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf)).await;
    let n = read.expect("read timeout").expect("read failed");
    assert_eq!(n, 0, "client should be disconnected after reauth timeout");

    drop(relay);
}

#[tokio::test]
async fn over_capacity_client_gets_server_list_then_disconnects() {
    let mut upstream = UpstreamHarness::start().await;
    let relay_addr = free_addr().await;
    let metrics_addr = free_addr().await;

    let relay = RelayProcess::spawn(&relay_args(
        upstream.addr,
        relay_addr,
        metrics_addr,
        1,
        720,
        65_536,
    ));

    wait_for_port(metrics_addr, Duration::from_secs(8)).await;
    upstream.wait_ready().await;

    let mut first = TcpStream::connect(relay_addr)
        .await
        .expect("connect first client");
    send_auth(&mut first, "first@example.com").await;

    let mut second = TcpStream::connect(relay_addr)
        .await
        .expect("connect second client");
    let mut buf = vec![0_u8; 512];
    let n = tokio::time::timeout(Duration::from_secs(5), second.read(&mut buf))
        .await
        .expect("second client read timeout")
        .expect("second client read failed");
    assert!(n > 0, "second client should receive server list frame");

    let expected_server_list =
        xor_ff(format!("/ServerList/127.0.0.1:{}\0", upstream.addr.port()).as_bytes());
    assert_eq!(&buf[..n], &expected_server_list[..]);

    let n2 = tokio::time::timeout(Duration::from_secs(5), second.read(&mut buf))
        .await
        .expect("second disconnect read timeout")
        .expect("second disconnect read failed");
    assert_eq!(
        n2, 0,
        "second client should be disconnected after server list"
    );

    drop(relay);
}

#[tokio::test]
async fn slow_client_is_disconnected_when_buffer_budget_is_exceeded() {
    let mut upstream = UpstreamHarness::start().await;
    let upstream_addr = upstream.addr;
    let send_tx = upstream.send_tx.clone();

    let relay_addr = free_addr().await;
    let metrics_addr = free_addr().await;

    let relay = RelayProcess::spawn(&relay_args(
        upstream_addr,
        relay_addr,
        metrics_addr,
        10,
        720,
        32,
    ));

    wait_for_port(metrics_addr, Duration::from_secs(8)).await;
    upstream.wait_ready().await;

    let mut client = TcpStream::connect(relay_addr)
        .await
        .expect("connect slow client");
    send_auth(&mut client, "slow@example.com").await;

    for _ in 0..200 {
        send_tx
            .send(vec![0xAA; 128])
            .expect("send upstream payload to relay");
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut buf = [0_u8; 1];
    let n = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf))
        .await
        .expect("slow client disconnect timeout")
        .expect("slow client read failed");
    assert_eq!(n, 0, "slow client should be disconnected on queue overflow");

    drop(relay);
}

#[tokio::test]
async fn health_endpoint_reports_status_and_active_clients() {
    let mut upstream = UpstreamHarness::start().await;
    let relay_addr = free_addr().await;
    let metrics_addr = free_addr().await;

    let relay = RelayProcess::spawn(&relay_args(
        upstream.addr,
        relay_addr,
        metrics_addr,
        10,
        720,
        65_536,
    ));

    wait_for_port(metrics_addr, Duration::from_secs(8)).await;
    upstream.wait_ready().await;

    let initial = get_json(metrics_addr, "/health").await;
    assert_eq!(initial["status"], "ok");
    assert_eq!(initial["forwarding_paused"], false);
    assert_eq!(initial["downstream_active_clients"], 0);

    let mut client = TcpStream::connect(relay_addr)
        .await
        .expect("connect client for health test");
    send_auth(&mut client, "health@example.com").await;

    wait_for_active_clients(metrics_addr, 1, Duration::from_secs(5)).await;
    let while_connected = get_json(metrics_addr, "/health").await;
    assert_eq!(while_connected["status"], "ok");
    assert_eq!(while_connected["downstream_active_clients"], 1);

    drop(client);
    wait_for_active_clients(metrics_addr, 0, Duration::from_secs(5)).await;
    let after_disconnect = get_json(metrics_addr, "/health").await;
    assert_eq!(after_disconnect["status"], "ok");
    assert_eq!(after_disconnect["downstream_active_clients"], 0);

    drop(relay);
}

#[tokio::test]
async fn health_endpoint_reports_forwarding_paused_when_quality_drops() {
    let mut upstream = UpstreamHarness::start().await;
    let upstream_addr = upstream.addr;
    let send_tx = upstream.send_tx.clone();

    let relay_addr = free_addr().await;
    let metrics_addr = free_addr().await;

    let relay = RelayProcess::spawn(&relay_args(
        upstream_addr,
        relay_addr,
        metrics_addr,
        10,
        720,
        16,
    ));

    wait_for_port(metrics_addr, Duration::from_secs(8)).await;
    upstream.wait_ready().await;

    let mut client = TcpStream::connect(relay_addr)
        .await
        .expect("connect client for paused health test");
    send_auth(&mut client, "paused@example.com").await;

    for _ in 0..200 {
        send_tx
            .send(vec![0xAB; 128])
            .expect("send upstream payload to relay");
    }

    wait_for_forwarding_paused(metrics_addr, true, Duration::from_secs(5)).await;
    let paused = get_json(metrics_addr, "/health").await;
    assert_eq!(paused["status"], "ok");
    assert_eq!(paused["forwarding_paused"], true);

    drop(client);
    drop(relay);
}

fn relay_args(
    upstream_addr: SocketAddr,
    relay_addr: SocketAddr,
    metrics_addr: SocketAddr,
    max_clients: usize,
    auth_timeout_secs: u64,
    client_buffer_bytes: usize,
) -> Vec<String> {
    vec![
        "--email".into(),
        "relay@example.com".into(),
        "--server".into(),
        format!("127.0.0.1:{}", upstream_addr.port()),
        "--bind".into(),
        relay_addr.to_string(),
        "--metrics-bind".into(),
        metrics_addr.to_string(),
        "--max-clients".into(),
        max_clients.to_string(),
        "--auth-timeout-secs".into(),
        auth_timeout_secs.to_string(),
        "--client-buffer-bytes".into(),
        client_buffer_bytes.to_string(),
    ]
}

async fn free_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral addr");
    listener.local_addr().expect("ephemeral local addr")
}

async fn wait_for_port(addr: SocketAddr, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if TcpStream::connect(addr).await.is_ok() {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {addr}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn send_auth(stream: &mut TcpStream, email: &str) {
    let logon = build_logon_message(email);
    let wire = xor_ff(logon.as_bytes());
    stream.write_all(&wire).await.expect("send downstream auth");
}

async fn get_json(addr: SocketAddr, path: &str) -> Value {
    let mut stream = TcpStream::connect(addr)
        .await
        .expect("connect http endpoint");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write http request");

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .expect("http read timeout")
        .expect("http read failed");

    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("http response missing header/body separator");
    let body = &response[split + 4..];
    serde_json::from_slice(body).expect("parse json body")
}

async fn wait_for_active_clients(addr: SocketAddr, expected: u64, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let json = get_json(addr, "/health").await;
        if json["downstream_active_clients"] == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for active clients {expected}, got {}",
            json["downstream_active_clients"]
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_forwarding_paused(addr: SocketAddr, expected: bool, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let json = get_json(addr, "/health").await;
        if json["forwarding_paused"] == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for forwarding_paused={expected}, got {}",
            json["forwarding_paused"]
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
