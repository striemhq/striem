use std::sync::Arc;

use striem_api::serve;
use striem_config::{StrIEMConfig, StringOrList};
use tokio::main;
use tokio::sync::{RwLock, broadcast};

#[main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = StrIEMConfig::new()?;
    let rules = if let Some(StringOrList::String(dir)) = &config.detections {
        dir.clone()
    } else {
        "./rules".to_string()
    };
    let detections = sigmars::SigmaCollection::new_from_dir(&rules)
        .map_err(|e| anyhow::anyhow!("Failed to load Sigma rules: {}", e))?;

    let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(1);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        shutdown_tx.send(()).unwrap();
    });
    serve(&config, Arc::new(RwLock::new(detections)), shutdown_rx).await
}
