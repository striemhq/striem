//! StrIEM - Streaming Intelligence and Event Management
//!
//! Entry point for the StrIEM SIEM daemon. Responsible for:
//! - Loading configuration from file or environment variables
//! - Initializing the application with detection rules and storage
//! - Handling graceful shutdown via SIGINT/SIGTERM

use anyhow::Result;
use striem_config::StrIEMConfig;
mod app;
mod detection;
use app::App;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let argv: Vec<String> = std::env::args().collect();

    // Load configuration from file if provided, otherwise use defaults/environment variables
    // This allows both "striem" and "striem config.yaml" invocations
    let config = match argv.len() {
        1 => StrIEMConfig::new()?,
        _ => StrIEMConfig::from_file(&argv[1])?,
    };
    let mut app = App::new(config).await?;
    let shutdown = app.shutdown();

    // Spawn signal handler for graceful shutdown
    // Broadcast to all subsystems (API, Vector server, storage, detections)
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("StrIEM shutting down...");
        shutdown.send(()).unwrap();
    });

    println!(".:: Starting StrIEM ::.");
    app.run().await?;
    println!(".:: StrIEM Stopped. Goodbye ::.");

    Ok(())
}
