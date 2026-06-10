#[tokio::main]
async fn main() -> anyhow::Result<()> {
    sled_cli::run_cli().await
}
