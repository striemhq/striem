//! Parquet file writer with time-based rotation.
//!
//! # Rotation Strategy
//! Files are rotated every 5 minutes (configurable) to bound file sizes
//! and enable incremental queries. Empty files are written to temp locations
//! and only moved to final location if non-empty.
//!
//! # Concurrency
//! Uses ArcSwap for lock-free rotation, allowing writes to continue
//! while old file is being finalized and moved.

use anyhow::Result;
use arc_swap::ArcSwap;
use arrow::{array::RecordBatch, datatypes::SchemaRef};
use log::{debug, trace};
use parquet::arrow::{AsyncArrowWriter, arrow_writer::ArrowWriterOptions};
use parquet::{
    basic::Compression,
    file::{
        metadata::KeyValue,
        properties::{WriterProperties, WriterVersion},
    },
};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::{fs::File, sync::Mutex};
type WriterInstanceMutex = Mutex<Option<WriterImpl>>;
type WriterInstance = Arc<ArcSwap<WriterInstanceMutex>>;

/// Internal writer state with temporary file and final path.
/// Separated to enable atomic rotation via ArcSwap.
struct WriterImpl {
    path: String,
    tempfile: NamedTempFile,
    writer: AsyncArrowWriter<File>,
}

/// Manages Parquet file lifecycle: creation, buffering, rotation, finalization.
/// Uses temporary files to avoid partial writes in final location.
pub struct Writer {
    schema: SchemaRef,
    inner: WriterInstance,
    // TODO: Make rotation interval configurable per-class for different retention needs
    rotation_interval: tokio::time::Duration,
}

impl Writer {
    /// Create a new writer with 5-minute rotation interval.
    ///
    /// # File Lifecycle
    /// 1. Events buffered in memory (Arrow RecordBatch)
    /// 2. Flushed to temporary file periodically
    /// 3. On rotation, temp file moved to final location with UUIDv7 name
    /// 4. Empty files (no row groups) are discarded to save storage
    pub fn new(path: String, schema: SchemaRef) -> Result<Self> {
        let writer = Arc::new(ArcSwap::from_pointee(Self::create_writer(
            path.clone(),
            schema.clone(),
        )?));

        Ok(Self {
            schema: schema.clone(),
            inner: writer.clone(),
            rotation_interval: tokio::time::Duration::from_secs(300),
        })
    }

    /// Spawn background rotation task.
    ///
    /// # Rotation Timing
    /// Fixed 5-minute interval provides predictable file sizes and query patterns.
    /// High-volume classes may produce 100MB+ files; low-volume classes stay small.
    pub async fn run(&mut self) -> Result<()> {
        let path = self
            .inner
            .load()
            .lock()
            .await
            .as_ref()
            .map(|w| w.path.clone())
            .ok_or_else(|| anyhow::anyhow!("Writer not initialized"))?;

        tokio::spawn({
            let schema = self.schema.clone();
            let inner = self.inner.clone();
            let rotation_interval = self.rotation_interval;
            async move {
                loop {
                    tokio::time::sleep(rotation_interval).await;
                    Self::rotate(path.clone(), schema.clone(), inner.clone())
                        .await
                        .ok();
                }
            }
        });
        Ok(())
    }

    /// Create a new writer instance with temporary file.
    ///
    /// # Design Choice: Temp File vs Final File
    /// Using temporary files prevents corrupt/partial files in storage directory
    /// if process crashes mid-write. Only non-empty, finalized files appear.
    ///
    /// Trade-off: Extra disk I/O for atomic move, but negligible for 5min files.
    fn create_writer(path: String, schema: SchemaRef) -> Result<WriterInstanceMutex> {
        let tempfile = NamedTempFile::new()?;
        trace!(
            "{} created temporary file: {}",
            schema
                .metadata
                .get("description")
                .unwrap_or(&"unknown".into()),
            tempfile.path().display()
        );

        let mut metadata = vec![KeyValue {
            key: "created_by".to_string(),
            value: Some(format!(
                "StrIEM version {} (build {})",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_GIT_SHA")
            )),
        }];

        if let Some(desc) = schema.metadata.get("description") {
            metadata.push(KeyValue {
                key: "description".to_string(),
                value: Some(desc.to_string()),
            });
        }
        if let Some(file) = schema.metadata.get("schema_file") {
            metadata.push(KeyValue {
                key: "schema_file".to_string(),
                value: Some(file.to_string()),
            });
        }

        let props = WriterProperties::builder()
            .set_writer_version(WriterVersion::PARQUET_2_0)
            .set_compression(Compression::SNAPPY)
            .set_key_value_metadata(Some(metadata))
            .build();

        let options = ArrowWriterOptions::default()
            .with_properties(props)
            .with_skip_arrow_metadata(true)
            .with_schema_root(
                schema
                    .metadata
                    .get("description")
                    .cloned()
                    .unwrap_or_else(|| "arrow_schema".into()),
            );

        let writer = AsyncArrowWriter::try_new_with_options(
            File::from_std(tempfile.reopen()?),
            schema.clone(),
            options,
        )?;

        Ok(Mutex::new(Some(WriterImpl {
            path,
            tempfile,
            writer,
        })))
    }

    /// Atomically rotate to a new writer, finalizing and moving the old file.
    ///
    /// # Atomicity
    /// ArcSwap enables lock-free rotation - new writes go to new file
    /// while old file is finalized without blocking.
    ///
    /// # File Naming
    /// UUIDv7 provides time-ordered, collision-free names. Sorts chronologically
    /// in filesystem listings and DuckDB queries (`ORDER BY filename`).
    async fn rotate(path: String, schema: SchemaRef, inner: WriterInstance) -> Result<()> {
        let new_writer = Self::create_writer(path, schema.clone())?;
        let guard = inner.swap(Arc::new(new_writer));

        let old = guard.lock().await.take();
        if let Some(mut meta) = old {
            meta.writer.finish().await?;
            if !meta.writer.flushed_row_groups().is_empty()
                && meta.writer.flushed_row_groups()[0].num_rows() != 0
            {
                tokio::fs::create_dir_all(&meta.path).await?;
                let path = format!("{}/{}.parquet", &meta.path, uuid::Uuid::now_v7());
                trace!(
                    "{} wrote new file: {}",
                    schema
                        .metadata
                        .get("description")
                        .unwrap_or(&"unknown".into()),
                    path
                );
                let (file, tmppath) = meta.tempfile.keep()?;
                drop(file);
                tokio::fs::copy(tmppath.clone(), path)
                    .await
                    .map(|_| tokio::fs::remove_file(tmppath))?
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn write(&self, event: &serde_json::Value) -> Result<()> {
        let record_batch = crate::convert_json(event, &self.schema)?;
        trace!(
            "{} writing event",
            self.schema
                .metadata
                .get("description")
                .unwrap_or(&"unknown".into())
        );
        self.write_recordbatch(&record_batch).await
    }

    pub async fn write_recordbatch(&self, batch: &RecordBatch) -> Result<()> {
        loop {
            // if we get None back, it's a race with rotate & we should try again
            // TODO: timeout
            let guard = self.inner.load();
            let mut writer = guard.lock().await;
            if let Some(meta) = writer.as_mut() {
                meta.writer.write(batch).await?;
                break;
            } else {
                debug!("Writer is being rotated, retrying...");
            }
        }
        Ok(())
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let guard = self.inner.load();
        tokio::spawn(async move {
            let mut writer = guard.lock().await;
            if let Some(mut meta) = writer.take() {
                let result = meta.writer.finish().await;
                if result.is_ok() && meta.writer.bytes_written() != 0 {
                    let _ = meta.tempfile.persist(format!(
                        "{}/{}.parquet",
                        &meta.path,
                        uuid::Uuid::now_v7()
                    ));
                }
            }
        });
    }
}
