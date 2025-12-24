mod actions;
mod alerts;
mod destination;
mod detections;
pub mod features;
mod persist;
mod query;
mod routes;
mod server;
mod sinks;
mod sources;
mod vector;

use arc_swap::ArcSwap;
use log::error;

use axum::http::HeaderValue;
pub use server::serve;
use striem_common::SysMessage;

use std::sync::Arc;
use tokio::sync::RwLock;

use sigmars::SigmaCollection;
use striem_config::StrIEMConfig;

use actions::Mcp;

#[cfg(feature = "duckdb")]
pub(crate) type Pool = r2d2::Pool<duckdb::DuckdbConnectionManager>;
#[cfg(all(feature = "sqlite", not(feature = "duckdb")))]
pub(crate) type Pool = r2d2::Pool<sqlite::SqliteConnectionManager>;
#[cfg(not(any(feature = "duckdb", feature = "sqlite")))]
pub(crate) type Pool = ();

#[derive(Clone)]
pub(crate) struct ApiState {
    pub detections: Arc<RwLock<SigmaCollection>>,
    pub actions: Option<Arc<Mcp>>,
    pub db: Option<Pool>,
    pub features: HeaderValue,
    pub sys: tokio::sync::broadcast::Sender<SysMessage>,
    pub config: Arc<ArcSwap<StrIEMConfig>>,
}

#[cfg(feature = "duckdb")]
pub(crate) fn initdb(config: &StrIEMConfig) -> Option<Pool> {
    // Create DuckDB connection pool with metadata caching enabled
    // Metadata cache significantly improves query performance on large Parquet datasets
    // by avoiding repeated schema reads
    let mut allowed = vec![
        "'application_activity'".to_string(),
        "'discovery'".to_string(),
        "'findings'".to_string(),
        "'identity_access_management'".to_string(),
        "'iam'".to_string(),
        "'network_activity'".to_string(),
        "'remediation'".to_string(),
        "'system_activity'".to_string(),
        "'unmanned_systems'".to_string(),
    ];

    if let Some(storage) = &config.storage {
        allowed.push(format!("'{}'", &storage.path.to_string_lossy()));
    }

    if let Some(ref dbpath) = config.db {
        std::fs::create_dir_all(dbpath)
            .map_err(anyhow::Error::from)
            .and_then(|_| {
                let path = dbpath.join("striem.db");

                allowed.extend([format!("'{}'", dbpath.to_string_lossy())]);

                let allowed_str = format!("[{}]", allowed.join(", "));

                duckdb::DuckdbConnectionManager::file_with_flags(
                    path,
                    duckdb::Config::default()
                        .enable_object_cache(true)
                        .map_err(anyhow::Error::from)?,
                )
                .map_err(anyhow::Error::from)
                .and_then(|db| {
                    r2d2::Pool::builder()
                        .build(db)
                        .inspect(|pool| {
                            pool.get()
                                .map(|conn| {
                                    conn.execute(
                                        "SET allowed_directories = ?;
                                             SET enable_external_access = false;",
                                        duckdb::params![&allowed_str],
                                    )
                                })
                                .ok();
                        })
                        .map_err(anyhow::Error::from)
                })
            })
            .and_then(|pool| {
                let mut conn = pool.get().map_err(anyhow::Error::from)?;
                crate::persist::init(&mut conn)?;
                Ok(pool)
            })
            .inspect_err(|e| {
                error!("{}", e);
            })
            .ok()
    } else if config.storage.is_some() {
        let allowed_str = format!("[{}]", allowed.join(", "));
        duckdb::DuckdbConnectionManager::memory_with_flags(
            duckdb::Config::default().enable_object_cache(true).ok()?,
        )
        .map_err(anyhow::Error::from)
        .and_then(|db| {
            r2d2::Pool::builder()
                .build(db)
                .inspect(|pool| {
                    pool.get()
                        .map(|conn| {
                            conn.execute(
                                "SET allowed_directories = ?;
                                    SET enable_external_access = false;",
                                duckdb::params![&allowed_str],
                            )
                        })
                        .ok();
                })
                .map_err(anyhow::Error::from)
        })
        .inspect_err(|e| {
            error!("{}", e);
        })
        .ok()
    } else {
        None
    }
}

#[cfg(all(feature = "sqlite", not(feature = "duckdb")))]
pub(crate) fn db_pool(config: &StrIEMConfig) -> Option<Pool> {
    unimplemented!("SQLite support is not yet implemented");
    None
}

#[cfg(not(any(feature = "duckdb", feature = "sqlite")))]
pub(crate) fn db_pool(_config: &StrIEMConfig) -> Option<Pool> {
    None
}
