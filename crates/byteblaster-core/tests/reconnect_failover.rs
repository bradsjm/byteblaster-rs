use byteblaster_core::unstable::{EndpointRotator, HealthObserver, Watchdog, next_backoff_secs};
use byteblaster_core::{
    ByteBlasterClient, Client, ClientConfig, ClientEvent, CoreError, DecodeConfig,
    calculate_checksum,
};
use futures::StreamExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::time::{Duration, Instant};

const SYNC: &[u8; 6] = b"\0\0\0\0\0\0";

fn xor_encode(input: &[u8]) -> Vec<u8> {
    input.iter().map(|b| b ^ 0xFF).collect()
}

fn build_header(filename: &str, checksum: u32) -> [u8; 80] {
    let mut raw = format!("/PF{filename} /PN 1 /PT 1 /CS {checksum} /FD01/01/2024 01:00:00 AM\r\n");
    while raw.len() < 80 {
        raw.push(' ');
    }

    let mut out = [0u8; 80];
    out.copy_from_slice(&raw.as_bytes()[..80]);
    out
}

fn encoded_valid_data_frame() -> Vec<u8> {
    let body = [b'R'; 1024];
    let checksum = calculate_checksum(&body) as u32;
    let header = build_header("reconnect.bin", checksum);

    let mut decoded = Vec::new();
    decoded.extend_from_slice(SYNC);
    decoded.extend_from_slice(&header);
    decoded.extend_from_slice(&body);
    xor_encode(&decoded)
}

#[test]
fn reconnect_failover_rotates_endpoints_with_backoff() {
    let mut rotator = EndpointRotator::new(vec![
        ("primary.example".to_string(), 2211),
        ("secondary.example".to_string(), 2211),
        ("tertiary.example".to_string(), 1000),
    ]);

    assert_eq!(rotator.next(), Some(("primary.example".to_string(), 2211)));
    assert_eq!(
        rotator.next(),
        Some(("secondary.example".to_string(), 2211))
    );
    assert_eq!(rotator.next(), Some(("tertiary.example".to_string(), 1000)));
    assert_eq!(rotator.next(), Some(("primary.example".to_string(), 2211)));

    assert_eq!(next_backoff_secs(1, 0), 1);
    assert_eq!(next_backoff_secs(1, 1), 2);
    assert_eq!(next_backoff_secs(1, 2), 4);
    assert_eq!(next_backoff_secs(1, 6), 60);
    assert_eq!(next_backoff_secs(5, 10), 60);
}

#[test]
fn connection_idle_timeout() {
    let watchdog = Watchdog::new(2, 10);

    let now = Instant::now();
    assert!(!watchdog.should_close_at(now + Duration::from_secs(1)));
    assert!(watchdog.should_close_at(now + Duration::from_secs(3)));

    watchdog.on_data_received();
    let after_data = Instant::now();
    assert!(!watchdog.should_close_at(after_data + Duration::from_secs(1)));
    assert!(watchdog.should_close_at(after_data + Duration::from_secs(3)));
}

#[tokio::test]
async fn watchdog_timeout_reconnects_without_termination() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener
        .local_addr()
        .expect("listener should have local addr");

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let accepted_connections = Arc::new(AtomicUsize::new(0));
    let accepted_connections_task = Arc::clone(&accepted_connections);

    let server_task = tokio::spawn(async move {
        let payload = encoded_valid_data_frame();
        loop {
            tokio::select! {
                changed = shutdown_rx.changed() => {
                    if changed.is_ok() && *shutdown_rx.borrow() {
                        break;
                    }
                }
                accepted = listener.accept() => {
                    let Ok((mut socket, _)) = accepted else {
                        break;
                    };
                    accepted_connections_task.fetch_add(1, Ordering::Relaxed);

                    let mut auth_buf = [0u8; 128];
                    let _ = tokio::time::timeout(Duration::from_millis(200), socket.read(&mut auth_buf)).await;

                    if socket.write_all(&payload).await.is_err() {
                        continue;
                    }

                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    });

    let mut client = Client::builder(ClientConfig {
        email: "test@example.com".to_string(),
        servers: vec![("127.0.0.1".to_string(), address.port())],
        server_list_path: None,
        reconnect_delay_secs: 1,
        connection_timeout_secs: 1,
        watchdog_timeout_secs: 1,
        max_exceptions: 10,
        decode: DecodeConfig::default(),
    })
    .build()
    .expect("client should build");

    client.start().expect("client should start");
    let mut events = client.events();

    let deadline = Instant::now() + Duration::from_secs(8);
    let mut connected_events = 0u32;
    let mut watchdog_timeout_errors = 0u32;

    while Instant::now() < deadline {
        if connected_events >= 2 && watchdog_timeout_errors >= 1 {
            break;
        }

        match tokio::time::timeout(Duration::from_millis(300), events.next()).await {
            Ok(Some(Ok(ClientEvent::Connected(_)))) => {
                connected_events = connected_events.saturating_add(1);
            }
            Ok(Some(Err(CoreError::Lifecycle(message)))) if message == "watchdog timeout" => {
                watchdog_timeout_errors = watchdog_timeout_errors.saturating_add(1);
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => {}
        }
    }

    shutdown_tx
        .send(true)
        .expect("server shutdown signal should send");
    server_task.await.expect("server task should join");
    drop(events);
    client.stop().await.expect("client should stop");

    assert!(
        connected_events >= 2,
        "expected reconnect after watchdog timeout"
    );
    assert!(
        watchdog_timeout_errors >= 1,
        "expected watchdog timeout to be surfaced"
    );
    assert!(
        accepted_connections.load(Ordering::Relaxed) >= 1,
        "expected server to observe at least one accepted connection"
    );
}
