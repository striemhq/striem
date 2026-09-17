//! Standalone detection microservice.
//!
//! This binary runs the StrIEM detection service on its own. The service has a
//! Vector gRPC listener, the Sigma detection engine, an optional downstream
//! Vector forwarder, and the detection-admin gRPC API. It loads the
//! configuration from the environment (STRIEM_* variables) and from any striem
//! config file. This is the same as the main daemon. The API service connects
//! to this process over gRPC.

use std::sync::Arc;

use anyhow::Result;
use log::info;
use striem_common::SysMessage;
use striem_config::StrIEMConfig;
use tokio::sync::broadcast;

use striem::detection::DetectionService;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let config = Arc::new(arc_swap::ArcSwap::from_pointee(StrIEMConfig::new()?));

    let sys = broadcast::channel::<SysMessage>(1).0;

    let signal = sys.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("Shutdown signal received, stopping StrIEM Detect...");
        signal.send(SysMessage::Shutdown).ok();
    });

    let service = DetectionService::new(config, sys).await?;

    info!("Starting StrIEM Detect...");
    service.run().await
}
