//! Persistence abstraction for sources and sinks.
//!
//! [`Store`] is the repository trait the API state depends on to load and
//! persist its configured [`Source`]s and [`Sink`]s. Backends implement only
//! the operations they support — the default methods are no-ops so
//! [`NullStore`] (used when no database is configured) needs no code at all.

use std::sync::Arc;

use anyhow::Result;

use crate::sinks::Sink;
use crate::sources::Source;

/// Backing store for the API's sources and sinks.
///
/// All methods default to no-ops so a backend can implement only what it
/// supports and callers can treat "no database" uniformly.
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
    // No delete-sink endpoint yet; kept for symmetry with sources.
    #[allow(dead_code)]
    fn remove_sink(&self, _id: &str) -> Result<()> {
        Ok(())
    }
}

/// A store that persists nothing, used when no database is configured.
pub(crate) struct NullStore;

impl Store for NullStore {}

/// Build the [`Store`] for the given (optional) database pool.
///
/// Falls back to [`NullStore`] when there is no pool or the backing store
/// fails to initialise.
pub(crate) fn open(db: &Option<crate::Pool>) -> Arc<dyn Store> {
    let _ = db;

    #[cfg(feature = "duckdb")]
    if let Some(pool) = db {
        match duckdb_store::DuckdbStore::new(pool.clone()) {
            Ok(store) => return Arc::new(store),
            Err(e) => log::error!("failed to initialise persistence store: {}", e),
        }
    }

    Arc::new(NullStore)
}

#[cfg(feature = "duckdb")]
mod duckdb_store {
    use super::Store;
    use crate::Pool;
    use crate::sinks::Sink;
    use crate::sources::Source;
    use anyhow::Result;
    use duckdb::params;
    use serde::Serialize;
    use serde_json::Value;

    const CREATE_SOURCES_SQL: &str = r#"CREATE TABLE IF NOT EXISTS sources (
            id UUID PRIMARY KEY,
            type TEXT,
            config JSON);"#;

    const CREATE_SINKS_SQL: &str = r#"CREATE TABLE IF NOT EXISTS sinks (
            id TEXT PRIMARY KEY,
            type TEXT,
            config JSON);"#;

    /// DuckDB-backed [`Store`] sharing the API's connection pool.
    pub(crate) struct DuckdbStore {
        pool: Pool,
    }

    impl DuckdbStore {
        pub(crate) fn new(pool: Pool) -> Result<Self> {
            let conn = pool.get()?;
            conn.execute(CREATE_SOURCES_SQL, [])?;
            conn.execute(CREATE_SINKS_SQL, [])?;
            Ok(Self { pool })
        }
    }

    impl Store for DuckdbStore {
        fn load_sources(&self) -> Result<Vec<Box<dyn Source>>> {
            let conn = self.pool.get()?;
            conn.prepare("SELECT type, id, config FROM sources")?
                .query([])?
                .mapped(|row| {
                    let sourcetype: String = row.get(0)?;
                    let id: String = row.get(1)?;
                    let config: Value = row.get(2)?;
                    Ok((sourcetype, id, config))
                })
                .map(|row| Ok(row?.try_into()))
                .collect::<Result<_, Box<dyn std::error::Error>>>()
                .map_err(|e| anyhow::anyhow!("Failed to fetch sources from database: {}", e))?
        }

        fn add_source(&self, source: &dyn Source) -> Result<()> {
            let conn = self.pool.get()?;
            let sourcetype = source.sourcetype().to_string();
            let id = source.id();
            let config = source.config().serialize(serde_json::value::Serializer)?;
            conn.prepare("INSERT INTO sources (type, id, config) VALUES (?, ?, ?)")?
                .execute(params![&sourcetype, &id, &config])?;
            Ok(())
        }

        fn remove_source(&self, id: &str) -> Result<()> {
            let conn = self.pool.get()?;
            conn.prepare("DELETE FROM sources WHERE id = ?")?
                .execute(params![&id])?;
            Ok(())
        }

        fn load_sinks(&self) -> Result<Vec<Box<dyn Sink>>> {
            let conn = self.pool.get()?;
            conn.prepare("SELECT type, id, config FROM sinks")?
                .query([])?
                .mapped(|row| {
                    let sinktype: String = row.get(0)?;
                    let id: String = row.get(1)?;
                    let config: Value = row.get(2)?;
                    Ok((sinktype, id, config))
                })
                .map(|row| Ok(row?.try_into()))
                .collect::<Result<_, Box<dyn std::error::Error>>>()
                .map_err(|e| anyhow::anyhow!("Failed to fetch sinks from database: {}", e))?
        }

        fn add_sink(&self, sink: &dyn Sink) -> Result<()> {
            let conn = self.pool.get()?;
            let sinktype = sink.typename();
            let id = sink.id();
            let config = sink.settings();
            conn.prepare("INSERT INTO sinks (type, id, config) VALUES (?, ?, ?)")?
                .execute(params![&sinktype, &id, &config])?;
            Ok(())
        }

        fn remove_sink(&self, id: &str) -> Result<()> {
            let conn = self.pool.get()?;
            conn.prepare("DELETE FROM sinks WHERE id = ?")?
                .execute(params![&id])?;
            Ok(())
        }
    }
}
