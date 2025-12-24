//! Parquet storage backend for OCSF events.
//!
//! Routes events to appropriate Parquet writers based on OCSF class_uid.
//! Each OCSF class gets its own writer and directory structure:
//! `{storage_path}/{category}/{class}/`
//!
//! This organization enables efficient DuckDB queries by class/category
//! and keeps related events together for better compression.

use super::writer::Writer;
use super::{ocsf, util::visit_dirs};
use anyhow::{Result, anyhow};
use arc_swap::ArcSwap;
use log::{debug, error, info};
use parquet::arrow::parquet_to_arrow_schema;
use serde_json::Value;
use std::path::PathBuf;
use std::{collections::HashMap, sync::Arc};
use striem_common::SysMessage;
use striem_common::event::Event;
use striem_config::StrIEMConfig;

/// Backend managing multiple Parquet writers, one per OCSF class.
/// Writers are selected at runtime based on event's class_uid field.
pub struct ParquetBackend {
    config: Arc<ArcSwap<StrIEMConfig>>,
    path: Arc<ArcSwap<PathBuf>>,
    pub heap: HashMap<ocsf::Class, Writer>,
}

impl std::fmt::Debug for ParquetBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParquetBackend {{ heap: {:?} }}", self.heap.keys())
    }
}

impl ParquetBackend {
    /// Initialize backend by loading OCSF schemas and creating writers.
    ///
    /// # Schema Discovery
    /// Recursively scans schema directory for `.parquet` schema files.
    /// Schema file name (minus extension) must match OCSF class name.
    ///
    /// # Directory Structure
    /// Output path: `{out}/{category}/{class}/`
    /// Example: `./storage/iam/authentication/` for class_uid 3002
    ///
    /// This structure is optimized for DuckDB's glob patterns:
    /// `SELECT * FROM './storage/iam/**/*.parquet'`
    pub fn new(config: &Arc<ArcSwap<StrIEMConfig>>) -> Result<Self> {
        let (path, schemapath) = config
            .load()
            .storage
            .as_ref()
            .map(|c| (c.path.clone(), c.schema.clone()))
            .ok_or_else(|| anyhow!("storage path not set"))?;

        let path = Arc::new(ArcSwap::from_pointee(path));

        let mut heap = HashMap::new();

        for (schema, filepath) in visit_dirs(&schemapath)? {
            // Convert Parquet schema to Arrow schema and enrich with metadata
            // Metadata is preserved in Parquet files for debugging and lineage tracking
            let arrow_schema = Arc::new(
                parquet_to_arrow_schema(&schema, None)?.with_metadata(HashMap::from([
                    (
                        "created_by".to_string(),
                        format!(
                            "StrIEM version {} (build {})",
                            env!("CARGO_PKG_VERSION"),
                            env!("CARGO_GIT_SHA")
                        ),
                    ),
                    ("description".to_string(), schema.name().to_string()),
                    (
                        "schema_file".to_string(),
                        filepath
                            .strip_prefix(&schemapath)?
                            .to_string_lossy()
                            .to_string(),
                    ),
                ])),
            );

            // Derive category from class_uid using OCSF's numeric scheme:
            // class_uid 3002 -> category 3 (IAM), class 2 (Authentication)
            let class: ocsf::Class = schema.name().parse().map_err(|e: String| anyhow!(e))?;
            let category = ocsf::Category::try_from((class as u32 % 10000) / 1000)?;

            let subpath = PathBuf::from(category.to_string()).join(class.to_string());
            let writer = Writer::new(path.clone(), subpath, arrow_schema)?;

            heap.insert(class, writer);
        }

        Ok(Self {
            heap,
            path,
            config: config.clone(),
        })
    }

    /// Route and write a JSON event to the appropriate Parquet writer.
    ///
    /// # Routing Logic
    /// Extracts `class_uid` field from event to determine OCSF class.
    /// Fails if class_uid is missing or unknown (no matching schema loaded).
    ///
    /// # Error Handling
    /// Returns error rather than silently dropping events to surface
    /// schema mismatches early in development.
    pub async fn write(&self, value: &Value) -> Result<()> {
        let writer = value
            .get("class_uid")
            .and_then(|v| v.as_u64())
            .and_then(|v| ocsf::Class::try_from(v as u32).ok())
            .and_then(|k| self.heap.get(&k))
            .ok_or(anyhow::anyhow!("invalid OCSF"))?;

        writer.write(value).await?;

        Ok(())
    }

    async fn process(&self, events: Arc<Vec<Event>>) {
        for event in &*events {
            if let Err(e) = self.write(&event.data).await {
                error!("Failed to write event: {}", e);
            }
        }
    }

    /// Run the backend with dual event stream subscription.
    ///
    /// # Channel Architecture
    /// - `upstream_rx`: Raw events from Vector (all OCSF classes)
    /// - `internal_rx`: Detection findings from Sigma engine (class_uid 2004)
    ///
    /// Both streams are written to storage but routed to different Parquet files.
    /// Detection findings inherit metadata from original events but get new UIDs.
    ///
    /// # Lifecycle
    /// Spawns rotation tasks for all writers, then processes events until
    /// shutdown or both channels close. Writer Drop impls handle final flushes.
    pub async fn run(
        mut self,
        mut upstream_rx: tokio::sync::broadcast::Receiver<Arc<Vec<Event>>>,
        mut internal_rx: tokio::sync::broadcast::Receiver<Arc<Vec<Event>>>,
        mut sys: tokio::sync::broadcast::Receiver<SysMessage>,
    ) {
        // Start rotation timers for all writers before processing events
        for w in self.heap.values_mut() {
            w.run().await.expect("Failed to start writer");
        }
        let config = self.config.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // File finalization is handled by Writer's Drop implementation
                    result = upstream_rx.recv() => {
                        if let Ok(events) = result {
                            self.process(events).await;
                        } else {
                            debug!("Upstream channel closed, shutting down ParquetBackend");
                            break;
                        }
                    },
                    result = internal_rx.recv() => {
                        if let Ok(events) = result {
                            self.process(events).await;
                        } else {
                            debug!("Internal channel closed, shutting down ParquetBackend");
                            break;
                        }
                    },
                    msg = sys.recv() => {
                        match msg {
                            Ok(SysMessage::Shutdown) => {
                            info!("shutting down Parquet writer...");
                            return;
                            }
                            Ok(SysMessage::Reload) => {
                                info!("reloading Parquet writer config...");
                                if let Ok(path) = config.load().storage.as_ref()
                                .map(|c| c.path.clone())
                                .ok_or_else(|| anyhow!("storage path not set")) {
                                    self.path.store(Arc::new(path));
                                    // Schema reload not implemented yet
                                    info!("Parquet writer config reloaded");
                                } else {
                                    error!("failed to reload Parquet writer config");
                                    return;
                                }
                            }
                            Err(_) => {
                                info!("Shutdown channel closed, exiting ParquetBackend...");
                                return;
                            }
                            _ => continue,
                        }
                    }
                };
            }
        });
    }
}
