//! Standalone detection microservice.
//!
//! Runs the StrIEM detection service on its own: a Vector gRPC listener, the
//! Sigma detection engine, an optional downstream Vector forwarder, and the
//! detection-admin gRPC API. Configuration is loaded from the environment
//! (STRIEM_* variables) and any striem config file, identically to the main
//! daemon; the API service connects to this process over gRPC.

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
