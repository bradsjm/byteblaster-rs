use crate::support::{build_frame, build_header};
use emwin_protocol::qbt_receiver::{
    QbtDecodeConfig, QbtReceiver, QbtReceiverClient, QbtReceiverConfig, QbtReceiverError,
    QbtReceiverEvent, calculate_qbt_checksum,
};
use futures::StreamExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::time::{Duration, Instant};

fn encoded_valid_data_frame() -> Vec<u8> {
    let body = [b'R'; 1024];
    let checksum = calculate_qbt_checksum(&body) as u32;
    let header = build_header("reconnect.bin", 1, 1, checksum, None);
    build_frame(header, &body)
}

#[tokio::test]
async fn watchdog_timeout_reconnects_without_termination() {
    let listener_a = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address_a = listener_a
        .local_addr()
        .expect("listener should have local addr");

    let listener_b = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address_b = listener_b
        .local_addr()
        .expect("listener should have local addr");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let accepted_connections = Arc::new(AtomicUsize::new(0));

    let spawn_server = |listener: TcpListener,
                        mut local_shutdown_rx: watch::Receiver<bool>,
                        accepted_connections_task: Arc<AtomicUsize>| {
        tokio::spawn(async move {
            let payload = encoded_valid_data_frame();
            loop {
                tokio::select! {
                    changed = local_shutdown_rx.changed() => {
                        if changed.is_ok() && *local_shutdown_rx.borrow() {
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
        })
    };

    let server_task_a = spawn_server(
        listener_a,
        shutdown_rx.clone(),
        Arc::clone(&accepted_connections),
    );
    let server_task_b = spawn_server(
        listener_b,
        shutdown_rx.clone(),
        Arc::clone(&accepted_connections),
    );

    let mut client = QbtReceiver::builder(QbtReceiverConfig {
        email: "test@example.com".to_string(),
        servers: vec![
            ("127.0.0.1".to_string(), address_a.port()),
            ("127.0.0.1".to_string(), address_b.port()),
        ],
        server_list_path: None,
        follow_server_list_updates: true,
        reconnect_delay_secs: 1,
        connection_timeout_secs: 1,
        watchdog_timeout_secs: 1,
        max_exceptions: 10,
        decode: QbtDecodeConfig::default(),
    })
    .build()
    .expect("client should build");

    client.start().expect("client should start");
    let mut events = client.events().expect("events should be available");

    let deadline = Instant::now() + Duration::from_secs(8);
    let mut connected_events = 0u32;
    let mut watchdog_timeout_errors = 0u32;

    while Instant::now() < deadline {
        if connected_events >= 2 && watchdog_timeout_errors >= 1 {
            break;
        }

        match tokio::time::timeout(Duration::from_millis(300), events.next()).await {
            Ok(Some(Ok(QbtReceiverEvent::Connected(_)))) => {
                connected_events = connected_events.saturating_add(1);
            }
            Ok(Some(Err(QbtReceiverError::Lifecycle(message))))
                if message == "watchdog timeout" =>
            {
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
    server_task_a.await.expect("server task a should join");
    server_task_b.await.expect("server task b should join");
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
