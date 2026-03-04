use crate::live;

pub use live::server::ServerOptions;

pub async fn run(options: ServerOptions) -> anyhow::Result<()> {
    live::server::run(options).await
}
