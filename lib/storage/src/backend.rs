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
use log::{debug, error, info};
use parquet::arrow::parquet_to_arrow_schema;
use serde_json::Value;
use std::{collections::HashMap, path::Path, sync::Arc};
use striem_common::event::Event;

/// Backend managing multiple Parquet writers, one per OCSF class.
/// Writers are selected at runtime based on event's class_uid field.
pub struct ParquetBackend {
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
    pub fn new(schema: &String, out: &String) -> Result<Self> {
        let dir = Path::new(&schema);
        let mut heap = HashMap::new();
        for (s, path) in visit_dirs(dir).map_err(|e| anyhow!(e.to_string()))? {
            // Convert Parquet schema to Arrow schema and enrich with metadata
            // Metadata is preserved in Parquet files for debugging and lineage tracking
            let arrow_schema = Arc::new(parquet_to_arrow_schema(&s, None)?.with_metadata(
                HashMap::from([
                    (
                        "created_by".to_string(),
                        format!(
                            "StrIEM version {} (build {})",
                            env!("CARGO_PKG_VERSION"),
                            env!("CARGO_GIT_SHA")
                        ),
                    ),
                    ("description".to_string(), s.name().to_string()),
                    (
                        "schema_file".to_string(),
                        path.trim_start_matches(&format!("/{}", schema)).to_string(),
                    ),
                ]),
            ));

            // Derive category from class_uid using OCSF's numeric scheme:
            // class_uid 3002 -> category 3 (IAM), class 2 (Authentication)
            let class: ocsf::Class = s.name().parse().map_err(|e: String| anyhow!(e))?;
            let category = ocsf::Category::try_from((class as u32 % 10000) / 1000)?;
            let outpath = format!("{}/{}/{}", out, category.to_string(), class.to_string());

            let writer = Writer::new(outpath, arrow_schema)?;

            heap.insert(class, writer);
        }

        Ok(Self { heap })
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
        mut shutdown: tokio::sync::broadcast::Receiver<()>,
    ) {
        // Start rotation timers for all writers before processing events
        for w in self.heap.values_mut() {
            w.run().await.expect("Failed to start writer");
        }

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
                    _ = shutdown.recv() => {
                        info!("shutting down Parquet writer...");
                        return;
                    }
                };
            }
        });
    }
}
