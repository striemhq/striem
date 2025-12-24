//! Core application orchestration module.
//!
//! The App struct coordinates all StrIEM subsystems:
//! - Vector gRPC server for receiving events from Vector pipeline
//! - Detection engine for evaluating Sigma rules on streaming data
//! - Parquet storage backend for persisting OCSF-normalized events
//! - Vector client for forwarding detection findings downstream
//! - API server for management interface
//!
//! Event flow:
//! Vector Pipeline → VectorServer → broadcast → [DetectionHandler, ParquetBackend]
//!                                              ↓
//!                                    detection findings → VectorClient → downstream

use std::sync::Arc;

use anyhow::{Result, anyhow};
use arc_swap::ArcSwap;
use backoff::{ExponentialBackoff, future::retry};
use log::{debug, error, info, warn};
use serde_json::{Map, Value};
use tokio::sync::{RwLock, broadcast};

use sigmars::{MemBackend, SigmaCollection};

use striem_common::{SysMessage, event::Event};
use striem_config::{
    self as config, StrIEMConfig, StringOrList, input::Listener, output::Destination,
};

use striem_api as api;
use striem_storage as storage;
use striem_vector::{Client as VectorClient, Server as VectorServer};

use crate::detection::DetectionHandler;

/// Main application struct coordinating all StrIEM subsystems.
/// Uses Arc<RwLock<>> for detections to allow concurrent rule evaluation
/// while supporting dynamic rule updates via API.
pub struct App {
    /// Sigma detection rules with thread-safe access for concurrent evaluation and API updates
    pub detections: Arc<RwLock<SigmaCollection>>,
    pub config: Arc<ArcSwap<StrIEMConfig>>,
    /// gRPC server accepting events from Vector pipeline
    server: VectorServer,
    /// Internal broadcast channel for detection findings (separate from upstream Vector events)
    events: broadcast::Sender<Arc<Vec<Event>>>,
    /// etc
    sys: broadcast::Sender<SysMessage>,
}

impl App {
    /// Initialize the application with configuration.
    ///
    /// # Design Notes
    /// - Detection rules are loaded synchronously at startup to fail fast on invalid rules
    /// - Broadcast channels use Arc<Vec<Event>> to minimize cloning overhead for multiple subscribers
    /// - Channel capacity of 64 provides backpressure without excessive buffering
    pub async fn new(config: StrIEMConfig) -> Result<Self> {
        let broadcast = broadcast::channel::<SysMessage>(1).0;
        // Internal channel capacity tuned for detection findings (typically lower volume than raw events)
        let events = broadcast::channel::<Arc<Vec<Event>>>(64).0;
        let server = VectorServer::new();

        let mut detections = SigmaCollection::default();
        let config = Arc::new(ArcSwap::from_pointee(config));

        if let Some(StringOrList::String(path)) = &config.load().detections {
            debug!("... loading Sigma detection rules from {}", path);
        } else {
            debug!("... loading detection rules");
        }
        // Support both single directory and multiple directories for detection rules
        // This enables organizing rules by severity, product, or team ownership
        let count = match &config.load().detections {
            Some(config::StringOrList::String(path)) => detections
                .load_from_dir(path)
                .map_err(|e| anyhow!(e.to_string())),
            Some(config::StringOrList::List(paths)) => paths
                .iter()
                .map(|path| {
                    detections
                        .load_from_dir(path)
                        .map_err(|e| anyhow!(e.to_string()))
                })
                .collect::<Result<Vec<_>>>()
                .map(|r| r.iter().sum()),
            None => {
                warn!("No detection rules loaded");
                Ok(0)
            }
        }?;

        // MemBackend is required by sigmars for rule compilation and indexing
        // Rules are pre-compiled at startup to avoid runtime compilation overhead
        let mut backend = MemBackend::new().await;
        detections.init(&mut backend).await;

        let detections = Arc::new(RwLock::new(detections));

        info!("... loaded {} Sigma detections", count);
        Ok(App {
            detections,
            config,
            server,
            sys: broadcast,
            events,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        self.config_watch().await;

        let config = self.config.load();
        if let Some(_) = self.config.load().storage {
            info!("... initializing Parquet storage handler");
            self.run_parquet().await?;
        }

        // Only spawn detection handler if rules are configured
        // Allows running as a pure data pipeline without detection overhead
        if config.detections.is_some() && self.detections.read().await.len() > 0 {
            info!("... initializing detection handler");
            let src = self.server.subscribe().await?;
            let dest = self.events.clone();
            let mut detection_handler =
                DetectionHandler::new(src, dest, self.detections.clone(), self.sys.subscribe());

            tokio::spawn(async move {
                detection_handler.run().await;
            });
        }

        if config.api.enabled {
            info!("... initializing API server and Vector configuration");
            let broadcast = self.sys.clone();
            let detections = self.detections.clone();
            let config = self.config.clone();
            tokio::spawn(async move {
                api::serve(&config, detections, broadcast)
                    .await
                    .expect("API server failed");
            });
        }

        if let Some(Destination::Vector(ref vector)) = config.output {
            info!("... initializing Vector output to {}", vector.cfg.url());
            self.run_vector(vector).await?;
        }

        let shutdown = self.sys.subscribe();
        if let Listener::Vector(ref vector) = config.input {
            info!("... listening for Vector events on {}", vector.url());
            self.server.serve(&vector.address(), shutdown).await?;
        }

        Ok(())
    }

    pub fn update_channel(&self) -> broadcast::Sender<SysMessage> {
        self.sys.clone()
    }

    /// Initialize Parquet storage backend with dual subscription model.
    ///
    /// # Channel Architecture
    /// - `rx`: Upstream events from Vector (raw logs normalized to OCSF)
    /// - `rx_internal`: Detection findings from DetectionHandler (OCSF detection_finding class)
    ///
    /// Both streams are written to Parquet, but routed to different files based on class_uid.
    /// This allows querying raw data and detections independently via DuckDB.
    async fn run_parquet(&self) -> Result<()> {
        let writer =
            storage::ParquetBackend::new(&self.config).expect("Failed to create Parquet backend");

        let server_rx = self.server.subscribe().await?;
        let event_rx = self.events.subscribe();
        let shutdown = self.sys.subscribe();
        tokio::spawn(async move {
            writer.run(server_rx, event_rx, shutdown).await;
        });
        Ok(())
    }
    /// Initialize Vector client for forwarding detection findings downstream.
    ///
    /// # Retry Strategy
    /// Uses exponential backoff to handle transient network failures or Vector restarts.
    /// Only subscribes to internal channel (detection findings), not raw upstream events.
    /// This creates a detection-only output stream for downstream analysis or alerting.
    async fn run_vector(
        &self,
        vector: &striem_config::output::VectorDestinationConfig,
    ) -> Result<()> {
        let url = vector.cfg.url();
        let rx = self.events.subscribe();
        let shutdown = self.sys.subscribe();
        tokio::spawn(async move {
            // Retry indefinitely with exponential backoff until connection succeeds
            // This is critical for resilience during Vector restarts or network issues
            let mut sink = retry(ExponentialBackoff::default(), || async {
                VectorClient::new(&url, rx.resubscribe(), shutdown.resubscribe())
                    .await
                    .map_err(|e| {
                        warn!("Failed to connect to Vector at {}: {}", url, e);
                        e.into()
                    })
            })
            .await
            .expect("Failed to connect to Vector client");

            info!("... connected to downstream Vector at {}", url);

            sink.run().await.expect("Vector client failed");
        });
        Ok(())
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
                        {
                            if let Ok(newcfg) = crate::config().await {
                                locked.store(Arc::new(newcfg));
                                info!("config updated");
                                tx.send(SysMessage::Reload)
                                    .inspect_err(|e| {
                                        error!("failed to broadcast config reload: {}", e);
                                    })
                                    .ok();
                            }
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
        } else {
            if let Ok(dir) = std::env::current_dir() {
                dir.join("striem.json")
            } else {
                return Map::new();
            }
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
        } else {
            if let Ok(dir) = std::env::current_dir() {
                dir.join("striem.json")
            } else {
                return Err(anyhow!("Failed to determine config file path"));
            }
        };

        let data = serde_json::to_string_pretty(&Value::Object(updated.clone()))?;

        file.set_extension("tmp");
        tokio::fs::write(&file, data).await?;
        tokio::fs::rename(&file, file.with_extension("json")).await?;
        Ok(())
    }
}
