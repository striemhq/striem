use std::sync::Arc;

use striem_api::serve;
use striem_common::SysMessage;
use striem_config::StrIEMConfig;
use tokio::main;
use tokio::sync::broadcast;

#[main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // The API service is stateless with respect to detection rules: it proxies
    // rule management to the detection microservice over gRPC (address resolved
    // from config via detection_endpoint()).
    let config = StrIEMConfig::new()?;

    let sys = broadcast::channel::<SysMessage>(1).0;
    let sender = sys.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        sender.send(SysMessage::Shutdown).unwrap();
    });

    serve(&Arc::new(arc_swap::ArcSwap::from_pointee(config)), sys).await
}
