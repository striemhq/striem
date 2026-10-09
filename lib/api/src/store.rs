//! Storage layer for sources, sinks, and the storage backend.
//!
//! [`Store`] is the repository trait. The API state uses it to load and save
//! the configured [`Source`]s, [`Sink`]s, and [`Storage`]. A backend implements only the
//! operations that it supports. The default methods do nothing.
//!
//! [`JsonFileStore`] is the default backend. It keeps everything in one JSON
//! file.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use anyhow::{Context, Result};
use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sinks::Sink;
use crate::sources::Source;
use crate::storage::Storage;

/// The file name of the store, in the `db` directory of the configuration.
pub(crate) const STORE_FILE: &str = "store.json";

/// The store for the API's sources and sinks.
///
/// By default all the methods do nothing. Thus a backend implements only the
/// operations that it supports.
pub(crate) trait Store: Send + Sync {
    fn load_sources(&self) -> Result<Vec<Box<dyn Source>>> {
        Ok(Vec::new())
    }
    fn add_source(&self, _source: &dyn Source) -> Result<()> {
        Ok(())
    }
    fn remove_source(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    fn load_sinks(&self) -> Result<Vec<Box<dyn Sink>>> {
        Ok(Vec::new())
    }
    fn add_sink(&self, _sink: &dyn Sink) -> Result<()> {
        Ok(())
    }
    fn remove_sink(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    /// Loads the storage backend for the Explore and Alerts views, if one is
    /// configured.
    fn load_storage(&self) -> Result<Option<Box<dyn Storage>>> {
        Ok(None)
    }
    /// Sets the storage backend, or removes it with `None`.
    fn set_storage(&self, _storage: Option<&dyn Storage>) -> Result<()> {
        Ok(())
    }
}

/// One saved source or sink: the data that rebuilds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    /// The source type or the sink type name.
    #[serde(rename = "type")]
    kind: String,
    id: String,
    config: Value,
}

/// The content of the store file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoreFile {
    #[serde(default)]
    sources: Vec<Record>,
    #[serde(default)]
    sinks: Vec<Record>,
    /// The one storage backend, if one is configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    storage: Option<Record>,
}

/// A [`Store`] that keeps the sources and sinks in one JSON file.
///
/// The store keeps a copy of the file in memory. Each change writes the full
/// file again: first to a temporary file, then with a rename over the old
/// file. Thus a crash during a write does not leave a partial file. If the
/// write fails, the change is not applied, and the copy in memory stays the
/// same as the file.
pub(crate) struct JsonFileStore {
    path: PathBuf,
    contents: Mutex<StoreFile>,
}

impl JsonFileStore {
    /// Opens the store at `path`. If the file does not exist, the store starts
    /// empty and makes the file on the first change.
    ///
    /// A file that is not valid is an error. The store does not start empty in
    /// that case, because the next change would write over the saved sources
    /// and sinks.
    pub(crate) fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let contents = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("store file {} is not valid", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => StoreFile::default(),
            Err(e) => {
                return Err(e).with_context(|| format!("cannot read store file {}", path.display()));
            }
        };
        Ok(Self {
            path,
            contents: Mutex::new(contents),
        })
    }

    /// Changes a copy of the contents, writes the copy to the file, and then
    /// keeps the copy. The lock stays held across the write, so two changes
    /// cannot overwrite each other.
    fn update(&self, change: impl FnOnce(&mut StoreFile)) -> Result<()> {
        let mut contents = self.contents.lock().unwrap_or_else(PoisonError::into_inner);
        let mut next = contents.clone();
        change(&mut next);
        write_atomic(&self.path, &next)?;
        *contents = next;
        Ok(())
    }

    fn records(&self, pick: impl FnOnce(&StoreFile) -> &Vec<Record>) -> Vec<Record> {
        let contents = self.contents.lock().unwrap_or_else(PoisonError::into_inner);
        pick(&contents).clone()
    }
}

/// Writes `contents` to a temporary file next to `path`, then renames it to
/// `path`. On Unix, only the owner can read the file, because source and sink
/// settings can hold credentials (for example, API tokens).
fn write_atomic(path: &Path, contents: &StoreFile) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("cannot make store directory {}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(contents)?;
    {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options
            .open(&tmp)
            .with_context(|| format!("cannot write store file {}", tmp.display()))?;
        file.write_all(&data)
            .and_then(|()| file.sync_all())
            .with_context(|| format!("cannot write store file {}", tmp.display()))?;
    }
    std::fs::rename(&tmp, path)
        .with_context(|| format!("cannot replace store file {}", path.display()))?;
    Ok(())
}

/// Adds `record`, or replaces the record with the same id.
fn upsert(records: &mut Vec<Record>, record: Record) {
    match records.iter_mut().find(|r| r.id == record.id) {
        Some(existing) => *existing = record,
        None => records.push(record),
    }
}

/// Rebuilds each record. A record that does not rebuild (for example, a type
/// that this version does not know) is logged and skipped. It stays in the
/// file. Thus one bad record does not remove all the others.
fn rebuild<T>(records: Vec<Record>, what: &str) -> Vec<T>
where
    (String, String, Value): TryInto<T, Error = anyhow::Error>,
{
    records
        .into_iter()
        .filter_map(|r| {
            let id = r.id.clone();
            (r.kind, r.id, r.config)
                .try_into()
                .inspect_err(|e| warn!("skipping saved {what} {id}: {e}"))
                .ok()
        })
        .collect()
}

impl Store for JsonFileStore {
    fn load_sources(&self) -> Result<Vec<Box<dyn Source>>> {
        Ok(rebuild(self.records(|c| &c.sources), "source"))
    }

    fn add_source(&self, source: &dyn Source) -> Result<()> {
        let record = Record {
            kind: source.sourcetype().to_string(),
            id: source.id(),
            config: serde_json::to_value(source.config())?,
        };
        self.update(|c| upsert(&mut c.sources, record))
    }

    fn remove_source(&self, id: &str) -> Result<()> {
        self.update(|c| c.sources.retain(|r| r.id != id))
    }

    fn load_sinks(&self) -> Result<Vec<Box<dyn Sink>>> {
        Ok(rebuild(self.records(|c| &c.sinks), "sink"))
    }

    fn add_sink(&self, sink: &dyn Sink) -> Result<()> {
        let record = Record {
            kind: sink.typename(),
            id: sink.id(),
            config: sink.settings(),
        };
        self.update(|c| upsert(&mut c.sinks, record))
    }

    fn remove_sink(&self, id: &str) -> Result<()> {
        self.update(|c| c.sinks.retain(|r| r.id != id))
    }

    fn load_storage(&self) -> Result<Option<Box<dyn Storage>>> {
        let record = {
            let contents = self.contents.lock().unwrap_or_else(PoisonError::into_inner);
            contents.storage.clone()
        };
        Ok(rebuild(record.into_iter().collect(), "storage").pop())
    }

    fn set_storage(&self, storage: Option<&dyn Storage>) -> Result<()> {
        let record = storage.map(|s| Record {
            kind: s.typename(),
            id: s.id(),
            config: s.settings(),
        });
        self.update(|c| c.storage = record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sink(kind: &str, id: &str, config: Value) -> Box<dyn Sink> {
        (kind.to_string(), id.to_string(), config).try_into().unwrap()
    }

    fn source(kind: &str, id: &str, config: Value) -> Box<dyn Source> {
        (kind.to_string(), id.to_string(), config).try_into().unwrap()
    }

    #[test]
    fn missing_file_starts_empty_and_is_made_on_first_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(STORE_FILE);
        let store = JsonFileStore::open(&path).unwrap();
        assert!(store.load_sources().unwrap().is_empty());
        assert!(!path.exists());

        store
            .add_sink(sink("file", "d1", json!({ "path": "/data/storage" })).as_ref())
            .unwrap();
        assert!(path.exists());
    }

    #[test]
    fn sources_and_sinks_survive_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        {
            let store = JsonFileStore::open(&path).unwrap();
            store
                .add_source(
                    source("okta", "s1", json!({ "domain": "acme.okta.com", "token": "t" }))
                        .as_ref(),
                )
                .unwrap();
            store
                .add_sink(sink("file", "d1", json!({ "path": "/data/storage" })).as_ref())
                .unwrap();
        }

        let store = JsonFileStore::open(&path).unwrap();
        let sources = store.load_sources().unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id(), "s1");
        assert_eq!(sources[0].sourcetype().to_string(), "okta");
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["sources"][0]["config"]["domain"], "acme.okta.com");
        let sinks = store.load_sinks().unwrap();
        assert_eq!(sinks.len(), 1);
        assert_eq!(sinks[0].id(), "d1");
        assert_eq!(sinks[0].settings()["path"], "/data/storage");
    }

    #[test]
    fn add_replaces_the_same_id_and_remove_deletes_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonFileStore::open(dir.path().join(STORE_FILE)).unwrap();
        store
            .add_sink(sink("file", "d1", json!({ "path": "/a" })).as_ref())
            .unwrap();
        store
            .add_sink(sink("file", "d1", json!({ "path": "/b" })).as_ref())
            .unwrap();
        let sinks = store.load_sinks().unwrap();
        assert_eq!(sinks.len(), 1);
        assert_eq!(sinks[0].settings()["path"], "/b");

        store.remove_sink("d1").unwrap();
        assert!(store.load_sinks().unwrap().is_empty());
        // Removing an id that is not there is not an error.
        store.remove_sink("d1").unwrap();
    }

    #[test]
    fn an_unknown_record_is_skipped_but_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        std::fs::write(
            &path,
            json!({
                "sinks": [
                    { "type": "no_such_sink", "id": "x", "config": {} },
                    { "type": "file", "id": "d1", "config": { "path": "/p" } }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let store = JsonFileStore::open(&path).unwrap();
        let sinks = store.load_sinks().unwrap();
        assert_eq!(sinks.len(), 1);
        assert_eq!(sinks[0].id(), "d1");

        // A later write keeps the record that this version cannot rebuild.
        store.remove_sink("d1").unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["sinks"][0]["id"], "x");
    }

    #[test]
    fn storage_is_saved_replaced_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        let ch: Box<dyn Storage> = (
            "clickhouse".to_string(),
            "st1".to_string(),
            json!({ "endpoint": "http://ch:8123", "password": "pw" }),
        )
            .try_into()
            .unwrap();
        {
            let store = JsonFileStore::open(&path).unwrap();
            assert!(store.load_storage().unwrap().is_none());
            store.set_storage(Some(ch.as_ref())).unwrap();
        }

        let store = JsonFileStore::open(&path).unwrap();
        let loaded = store.load_storage().unwrap().unwrap();
        assert_eq!(loaded.id(), "st1");
        assert_eq!(loaded.typename(), "clickhouse");
        assert_eq!(loaded.settings()["password"], "pw");

        store.set_storage(None).unwrap();
        assert!(JsonFileStore::open(&path).unwrap().load_storage().unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_can_read_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        let store = JsonFileStore::open(&path).unwrap();
        store
            .add_sink(sink("file", "d1", json!({ "path": "/p" })).as_ref())
            .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn a_corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(STORE_FILE);
        std::fs::write(&path, "{ not json").unwrap();
        assert!(JsonFileStore::open(&path).is_err());
        // The file is left as it was.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    }
}
