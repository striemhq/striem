use std::sync::Arc;

use striem_api::serve;
use striem_common::SysMessage;
use striem_config::StrIEMConfig;
use tokio::main;
use tokio::sync::broadcast;

#[main]
async fn main() -> anyhow::Result<()> {
    // Starts logs, traces (OTLP when configured), and the metrics registry.
    striem_telemetry::init("api");

    // The API service holds no detection rules. It sends the rule management to
    // the detection microservice over gRPC. It gets the address from the
    // configuration with detection_endpoint().
    let config = StrIEMConfig::new()?;

    let sys = broadcast::channel::<SysMessage>(1).0;
    let sender = sys.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        sender.send(SysMessage::Shutdown).unwrap();
    });

    serve(&Arc::new(arc_swap::ArcSwap::from_pointee(config)), sys).await
}
