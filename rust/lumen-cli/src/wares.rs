//! wares — Lumen Package Manager
//! CLI for publishing and managing Lumen wares with Sigstore-style keyless signing.

#[tokio::main]
async fn main() {
    lumen_cli::wares_cli::run().await;
}
