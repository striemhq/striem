use std::net::SocketAddr;
use tokio::sync::broadcast;
use striem_vector::{Client, Server};
use striem_common::event::Event;
use anyhow::Result;
use log::info;
use std::env;
use std::sync::Arc;

use striem::detection::DetectionHandler;

use sigmars::SigmaCollection;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let addr = env::var("STRIEM_BIND_ADDRESS")
    .unwrap_or("127.0.0.1:50051".to_string())
    .parse::<SocketAddr>()
    .map_err(|e| anyhow::anyhow!("Invalid STRIEM_BIND_ADDRESS value: {}", e))?;

    let dest = env::var("STRIEM_VECTOR_URI")?;

    let rules = SigmaCollection::new_from_dir(&env::var("STRIEM_DETECTIONS")?)
        .map_err(|e| anyhow::anyhow!("Failed to load Sigma rules: {}", e))?;

    let (tx, rx) = broadcast::channel::<Arc<Vec<Event>>>(100);
    let (shutdown, shutdown_rx) = broadcast::channel::<striem_common::SysMessage>(1);

    let mut server = Server::new();

    let events = server.subscribe().await?;

    let mut detect = DetectionHandler::new(events,
                                  tx,
                                  Arc::new(tokio::sync::RwLock::new(rules)),
                                  shutdown_rx);

    let mut client = Client::new(&dest, rx, shutdown.subscribe()).await?;

    info!("Starting StrIEM Detect on port {}...", addr.port());

    tokio::select! {
        _ = detect.run() => { unreachable!("Detection handler exited unexpectedly") },
        _ = server.serve(&addr, shutdown.subscribe()) => {
            shutdown.send(striem_common::SysMessage::Shutdown)?;
            Err(anyhow::anyhow!("Vector server exited unexpectedly"))?;
        },
        _ = client.run() => {
            shutdown.send(striem_common::SysMessage::Shutdown)?;
            Err(anyhow::anyhow!("Vector client exited unexpectedly"))?;
        },
        _ = tokio::signal::ctrl_c() => {
            info!("Shutdown signal received, stopping StrIEM Detect...");
            shutdown.send(striem_common::SysMessage::Shutdown)?;
        }
    }
    Ok(())
}
