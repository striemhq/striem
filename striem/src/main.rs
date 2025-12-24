//! StrIEM - Streaming Intelligence and Event Management
//!
//! Entry point for the StrIEM SIEM daemon. Responsible for:
//! - Loading configuration from file or environment variables
//! - Initializing the application with detection rules and storage
//! - Handling graceful shutdown via SIGINT/SIGTERM

use std::path;

use anyhow::Result;
use striem_common::SysMessage;
use striem_config::StrIEMConfig;
mod app;
mod detection;
use app::App;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let config = config().await?;

    let mut app = App::new(config).await?;
    let update = app.update_channel();

    // Spawn signal handler for graceful shutdown
    // Broadcast to all subsystems (API, Vector server, storage, detections)
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("StrIEM shutting down...");
        update.send(SysMessage::Shutdown).unwrap();
    });

    println!(".:: Starting StrIEM ::.");
    app.run().await?;
    println!(".:: StrIEM Stopped. Goodbye ::.");

    Ok(())
}

pub(crate) async fn config() -> Result<StrIEMConfig> {
    let mut cfgfiles = std::env::args()
        .skip(1)
        .map(|arg| path::PathBuf::from(arg))
        .collect::<Vec<_>>();

    if let Some(dir) = std::env::var_os("STRIEM_APPDATA") {
        let cfg = path::PathBuf::from(dir).join("striem.json");
        if cfg.exists() {
            cfgfiles.push(cfg)
        }
    } else {
        let cfg = std::env::current_dir()?.join("striem.json");
        if cfg.exists() {
            cfgfiles.push(cfg)
        }
    };

    // Load configuration from file if provided, otherwise use defaults/environment variables
    // This allows both "striem" and "striem config.yaml" invocations
    match cfgfiles.len() {
        0 => Ok(StrIEMConfig::new()?),
        _ => Ok(StrIEMConfig::from_multi_file(cfgfiles)?),
    }
}
