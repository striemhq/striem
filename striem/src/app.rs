//! Core application orchestration module.
//!
//! The `App` runs StrIEM's two services in a single process and keeps their
//! configuration in sync:
//! - the **API service** (`striem_api`), which manages configuration and proxies
//!   detection-rule administration to the detection service over gRPC;
//! - the **detection microservice** (`crate::detection::DetectionService`), which
//!   listens for Vector events, runs the Sigma engine, forwards findings
//!   downstream, and hosts the detection-admin gRPC API.
//!
//! Split deployments run these as separate binaries (`striem_api` and
//! `usdetect`); this module is the co-located convenience path.
//!
//! Event flow:
//! Vector Pipeline → DetectionService(VectorServer → DetectionHandler) → findings
//!                                                → downstream VectorClient
//!            API ⇄ DetectionService (rule admin over gRPC)

use std::sync::Arc;

use anyhow::Result;
use arc_swap::ArcSwap;
use log::{error, info};
use serde_json::{Map, Value};
use tokio::sync::broadcast;

use striem_common::SysMessage;
use striem_config::StrIEMConfig;

use striem_api as api;

use crate::detection::DetectionService;

/// Top-level coordinator for the co-located API and detection services.
pub struct App {
    pub config: Arc<ArcSwap<StrIEMConfig>>,
    /// System broadcast channel (shutdown / config reload) shared by subsystems.
    sys: broadcast::Sender<SysMessage>,
}

impl App {
    /// Initialize the application with configuration.
    pub async fn new(config: StrIEMConfig) -> Result<Self> {
        let sys = broadcast::channel::<SysMessage>(1).0;
        let config = Arc::new(ArcSwap::from_pointee(config));

        Ok(App { config, sys })
    }

    pub async fn run(&mut self) -> Result<()> {
        self.config_watch().await;

        let config = self.config.load();

        // The API service manages configuration and proxies rule administration
        // to the detection service over gRPC.
        if config.api.enabled {
            info!("... initializing API server");
            let sys = self.sys.clone();
            let config = self.config.clone();
            tokio::spawn(async move {
                api::serve(&config, sys).await.expect("API server failed");
            });
        }

        // The detection service owns event ingestion, the Sigma engine, and the
        // detection-admin API. It blocks until shutdown.
        info!("... starting detection service on {}", config.input.url());
        let service = DetectionService::new(self.config.clone(), self.sys.clone()).await?;
        service.run().await?;

        Ok(())
    }

    pub fn update_channel(&self) -> broadcast::Sender<SysMessage> {
        self.sys.clone()
    }

    async fn config_watch(&self) {
        let mut rx = self.sys.subscribe();
        let tx = self.sys.clone();
        let locked = self.config.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(SysMessage::Shutdown) => {
                        info!("shutting down config watcher...");
                        return;
                    }
                    Ok(SysMessage::Update(updated)) => {
                        info!("updating configuration...");
                        // Apply updates to local config file and in-memory config
                        let mut current = Self::get_local_config().await;
                        for (k, v) in updated.iter() {
                            current.insert(k.clone(), v.clone());
                        }
                        if Self::set_local_config(&current)
                            .await
                            .inspect_err(|e| {
                                error!("failed to update config: {}", e);
                            })
                            .is_ok()
                            && let Ok(newcfg) = crate::config().await
                        {
                            locked.store(Arc::new(newcfg));
                            info!("config updated");
                            tx.send(SysMessage::Reload)
                                .inspect_err(|e| {
                                    error!("failed to broadcast config reload: {}", e);
                                })
                                .ok();
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("shutting down config watcher...");
                        return;
                    }
                    _ => {
                        continue;
                    }
                }
            }
        });
    }

    async fn get_local_config() -> Map<String, Value> {
        let file = if let Some(dir) = std::env::var_os("STRIEM_APPDATA") {
            std::path::PathBuf::from(dir).join("striem.json")
        } else if let Ok(dir) = std::env::current_dir() {
            dir.join("striem.json")
        } else {
            return Map::new();
        };

        tokio::fs::read_to_string(file)
            .await
            .map(|data| {
                serde_json::from_str(&data)
                    .map(|c: Value| c.as_object().cloned())
                    .unwrap_or_default()
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    async fn set_local_config(updated: &Map<String, Value>) -> Result<()> {
        let mut file = if let Some(dir) = std::env::var_os("STRIEM_APPDATA") {
            std::path::PathBuf::from(dir).join("striem.json")
        } else if let Ok(dir) = std::env::current_dir() {
            dir.join("striem.json")
        } else {
            return Err(anyhow::anyhow!("Failed to determine config file path"));
        };

        let data = serde_json::to_string_pretty(&Value::Object(updated.clone()))?;

        file.set_extension("tmp");
        tokio::fs::write(&file, data).await?;
        tokio::fs::rename(&file, file.with_extension("json")).await?;
        Ok(())
    }
}
