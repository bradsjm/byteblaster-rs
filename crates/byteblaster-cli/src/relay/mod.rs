mod auth;
mod config;
mod runtime;
mod server_list;
mod state;

pub use config::RelayArgs as RelayOptions;

pub async fn run(options: RelayOptions) -> anyhow::Result<()> {
    runtime::run(options).await
}
