use std::sync::Arc;

use striem_api::serve;
use striem_common::SysMessage;
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

    let sys = broadcast::channel::<SysMessage>(1).0;
    let sender = sys.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        sender.send(SysMessage::Shutdown).unwrap();
    });
    serve(
        &Arc::new(arc_swap::ArcSwap::from_pointee(config)),
        Arc::new(RwLock::new(detections)),
        sys,
    )
    .await
}
