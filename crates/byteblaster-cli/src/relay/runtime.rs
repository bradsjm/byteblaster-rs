use crate::relay::config::{RelayArgs, RelayConfig};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use byteblaster_core::qbt_receiver::{
    QbtRelayHealthSnapshot, QbtRelayMetricsSnapshot, QbtRelayState, run_qbt_relay,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::{error, info};

pub async fn run(args: RelayArgs) -> crate::error::CliResult<()> {
    let config = RelayConfig::from_args(args)?;
    config
        .relay
        .validate()
        .map_err(|err| crate::error::CliError::invalid_argument(err.to_string()))?;

    let state = Arc::new(QbtRelayState::from_upstream_servers(
        &config.relay.upstream_servers,
        config.relay.quality_window_secs,
    ));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let relay_state = Arc::clone(&state);
    let relay_config = config.relay.clone();
    let relay_shutdown = shutdown_rx.clone();
    let relay_task =
        tokio::spawn(async move { run_qbt_relay(relay_config, relay_state, relay_shutdown).await });

    let metrics_state = Arc::clone(&state);
    let metrics_bind_addr = config.metrics_bind_addr;
    let metrics_shutdown = shutdown_rx.clone();
    let metrics_task = tokio::spawn(async move {
        run_metrics_server(metrics_state, metrics_bind_addr, metrics_shutdown).await
    });

    tokio::signal::ctrl_c().await?;
    info!("shutdown signal received");
    let _ = shutdown_tx.send(true);

    match relay_task.await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            error!(error = %err, "relay runtime failed");
            return Err(crate::error::CliError::runtime(format!(
                "relay runtime failed: {err}"
            )));
        }
        Err(join_err) => {
            error!(error = %join_err, "relay task join failed");
            return Err(crate::error::CliError::runtime(format!(
                "relay task join failed: {join_err}"
            )));
        }
    }
    match metrics_task.await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            error!(error = %err, "metrics server failed");
            return Err(crate::error::CliError::runtime(format!(
                "metrics server failed: {err}"
            )));
        }
        Err(join_err) => {
            error!(error = %join_err, "metrics task join failed");
            return Err(crate::error::CliError::runtime(format!(
                "metrics task join failed: {join_err}"
            )));
        }
    }

    info!("relay stopped");
    Ok(())
}

async fn run_metrics_server(
    state: Arc<QbtRelayState>,
    metrics_bind_addr: std::net::SocketAddr,
    mut shutdown_rx: watch::Receiver<bool>,
) -> crate::error::CliResult<()> {
    let router = Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let listener = TcpListener::bind(metrics_bind_addr).await.map_err(|err| {
        crate::error::CliError::runtime(format!(
            "failed to bind metrics listener at {metrics_bind_addr}: {err}"
        ))
    })?;
    info!(metrics_addr = %metrics_bind_addr, "metrics server ready");

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.changed().await;
        })
        .await
        .map_err(|err| crate::error::CliError::runtime(format!("metrics server failed: {err}")))
}

async fn metrics_handler(State(state): State<Arc<QbtRelayState>>) -> Json<QbtRelayMetricsSnapshot> {
    Json(state.metrics_snapshot())
}

async fn health_handler(State(state): State<Arc<QbtRelayState>>) -> Json<QbtRelayHealthSnapshot> {
    Json(state.health_snapshot())
}
