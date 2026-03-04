mod runtime;

pub use runtime::RelayArgs as RelayOptions;

pub fn init_logging() {
    runtime::init_logging();
}

pub async fn run(options: RelayOptions) -> anyhow::Result<()> {
    runtime::run(options).await
}
