mod runtime;

pub use runtime::RelayArgs as RelayOptions;

pub async fn run(options: RelayOptions) -> anyhow::Result<()> {
    runtime::run(options).await
}
