//! StrIEM - Streaming Intelligence and Event Management
//!
//! The entry point for the StrIEM SIEM daemon. It does these tasks:
//! - It loads the configuration from a file or from environment variables.
//! - It starts the application with the detection rules and the storage.
//! - It does a clean shutdown for SIGINT or SIGTERM.

use std::path;

use anyhow::Result;
use striem_common::SysMessage;
use striem_config::StrIEMConfig;
mod app;
pub(crate) mod detection;
use app::App;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let config = config().await?;

    let mut app = App::new(config).await?;
    let update = app.update_channel();

    // Start the signal handler for a clean shutdown.
    // Send the shutdown to all the subsystems (API, Vector server, storage,
    // detections).
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
        .map(path::PathBuf::from)
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

    // Load the configuration from a file if you give one. If not, use the
    // defaults and the environment variables. Thus you can start the daemon as
    // "striem" or as "striem config.yaml".
    match cfgfiles.len() {
        0 => Ok(StrIEMConfig::new()?),
        _ => Ok(StrIEMConfig::from_multi_file(cfgfiles)?),
    }
}
