//! Storage backends: where the Explore and Alerts views read event data.
//!
//! A [`Storage`] is a configurable backend, like a source or a sink. It has an
//! id, a type, and settings that the [`Store`](crate::store::Store) saves. It
//! also implements [`StrIEMData`], so the alert and live-search endpoints can
//! read through it. At most one storage is active at a time. With no storage,
//! those endpoints use [`NoData`](crate::data::NoData).
//!
//! The backends:
//! - [`clickhouse::Clickhouse`] reads the `striem_ocsf` table that the
//!   ClickHouse destination writes.
//! - [`athena::Athena`] is a stub. It saves its settings, but each read gives
//!   `501 Not Implemented`.
//!
//! The routes:
//! - `GET /api/1/storage` gives the active storage, with no secrets.
//! - `PUT /api/1/storage/{type}` sets the active storage. If the new storage
//!   has the same type as the old one, an omitted secret (for example, the
//!   password) keeps its old value.
//! - `DELETE /api/1/storage` removes the active storage.

pub mod athena;
pub mod clickhouse;

use std::sync::Arc;

use anyhow::Result;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::ApiState;
use crate::data::StrIEMData;

/// A storage backend for the Explore and Alerts views.
#[tonic::async_trait]
pub(crate) trait Storage: StrIEMData {
    /// The stable resource id. This is also the key for storage.
    fn id(&self) -> String;

    /// The type name. The service uses it to save and rebuild this storage.
    fn typename(&self) -> String;

    /// A clear name for the UI.
    fn name(&self) -> String {
        self.typename()
    }

    /// The settings that the service needs to rebuild this storage. They can
    /// hold secrets.
    fn settings(&self) -> Value;

    /// The keys of [`settings`](Self::settings) that hold secrets. The API does
    /// not show them.
    fn secret_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// Tells if the backend can read data. A stub gives `false`.
    fn implemented(&self) -> bool {
        true
    }

    /// Checks that the backend is available. A failure does not prevent the
    /// save of the storage. The API gives it to the caller as a warning.
    async fn check(&self) -> Result<()> {
        Ok(())
    }
}

/// The settings to show in the API: each secret is removed, and
/// `<key>_set` tells if it has a value.
fn public_settings(storage: &dyn Storage) -> Value {
    let mut settings = storage.settings();
    if let Some(map) = settings.as_object_mut() {
        for key in storage.secret_keys() {
            let set = map
                .remove(*key)
                .is_some_and(|v| v.as_str().is_none_or(|s| !s.is_empty()) && !v.is_null());
            map.insert(format!("{key}_set"), json!(set));
        }
    }
    settings
}

/// Copies each secret of `old` into `config` when `config` does not give it.
/// Thus a client can change the other settings without the secret.
fn keep_secrets(config: &mut Value, old: &dyn Storage) {
    let old_settings = old.settings();
    let Some(map) = config.as_object_mut() else {
        return;
    };
    for key in old.secret_keys() {
        let missing = map.get(*key).is_none_or(|v| v.is_null());
        if missing && let Some(value) = old_settings.get(*key) {
            map.insert((*key).to_string(), value.clone());
        }
    }
}

/// The storage types that the API knows.
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
    Clickhouse,
    Athena,
}

impl StorageType {
    fn as_str(self) -> &'static str {
        match self {
            StorageType::Clickhouse => "clickhouse",
            StorageType::Athena => "athena",
        }
    }
}

/// A saved storage: `(type, id, settings)`.
pub type ExistingStorage = (String, String, Value);

impl TryInto<Box<dyn Storage>> for ExistingStorage {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<Box<dyn Storage>, Self::Error> {
        let (kind, id, config) = self;
        Ok(match kind.as_str() {
            "clickhouse" => Box::new(clickhouse::Clickhouse::new(
                id,
                serde_json::from_value(config)?,
            )?),
            "athena" => Box::new(athena::Athena {
                id,
                settings: serde_json::from_value(config)?,
            }),
            _ => anyhow::bail!("unsupported storage type: {kind}"),
        })
    }
}

/// The API view of a storage.
fn describe(storage: &dyn Storage) -> Value {
    json!({
        "configured": true,
        "id": storage.id(),
        "storagetype": storage.typename(),
        "name": storage.name(),
        "implemented": storage.implemented(),
        "config": public_settings(storage),
    })
}

async fn get_storage(State(state): State<ApiState>) -> axum::Json<Value> {
    let storage = state.storage.read().await;
    axum::Json(match storage.as_deref() {
        Some(storage) => describe(storage),
        None => json!({ "configured": false }),
    })
}

async fn set_storage(
    State(state): State<ApiState>,
    Path(storagetype): Path<StorageType>,
    axum::extract::Json(mut config): axum::extract::Json<Value>,
) -> Result<axum::Json<Value>, (StatusCode, String)> {
    let current = state.storage.read().await.clone();
    if let Some(old) = current.as_deref()
        && old.typename() == storagetype.as_str()
    {
        keep_secrets(&mut config, old);
    }

    let id = uuid::Uuid::now_v7().to_string();
    let storage: Box<dyn Storage> = (storagetype.as_str().to_string(), id, config)
        .try_into()
        .map_err(|e: anyhow::Error| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // A backend that is not available yet is not an error: the storage is
    // saved, and the caller gets a warning.
    let warning = storage.check().await.err().map(|e| e.to_string());
    if let Some(warning) = &warning {
        log::warn!("storage {} saved, but its check failed: {warning}", storage.typename());
    }

    state
        .store
        .set_storage(Some(storage.as_ref()))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let storage: Arc<dyn Storage> = Arc::from(storage);
    let mut response = describe(storage.as_ref());
    if let Some(warning) = warning {
        response["warning"] = json!(warning);
    }
    *state.storage.write().await = Some(storage);
    Ok(axum::Json(response))
}

async fn delete_storage(
    State(state): State<ApiState>,
) -> Result<axum::Json<()>, (StatusCode, String)> {
    state
        .store
        .set_storage(None)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    *state.storage.write().await = None;
    Ok(axum::Json(()))
}

pub fn create_router() -> Router<ApiState> {
    Router::new()
        .route("/", get(get_storage).delete(delete_storage))
        .route("/{storagetype}", put(set_storage))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage(kind: &str, config: Value) -> Box<dyn Storage> {
        (kind.to_string(), "s1".to_string(), config).try_into().unwrap()
    }

    #[test]
    fn saved_storage_rebuilds_with_its_settings() {
        let ch = storage(
            "clickhouse",
            json!({ "endpoint": "http://ch:8123", "database": "db", "user": "u", "password": "p" }),
        );
        assert_eq!(ch.typename(), "clickhouse");
        assert!(ch.implemented());
        assert_eq!(ch.settings()["password"], "p");

        let athena = storage("athena", json!({ "region": "us-east-1", "database": "ocsf" }));
        assert_eq!(athena.typename(), "athena");
        assert!(!athena.implemented());

        let unknown: Result<Box<dyn Storage>, _> =
            ("nope".to_string(), "x".to_string(), json!({})).try_into();
        assert!(unknown.is_err());
    }

    #[test]
    fn the_api_view_hides_secrets() {
        let ch = storage(
            "clickhouse",
            json!({ "endpoint": "http://ch:8123", "user": "u", "password": "p" }),
        );
        let view = describe(ch.as_ref());
        assert!(view["config"].get("password").is_none());
        assert_eq!(view["config"]["password_set"], true);
        assert_eq!(view["config"]["user"], "u");

        let no_password = storage("clickhouse", json!({ "endpoint": "http://ch:8123" }));
        assert_eq!(describe(no_password.as_ref())["config"]["password_set"], false);
    }

    #[test]
    fn an_omitted_secret_keeps_its_old_value() {
        let old = storage(
            "clickhouse",
            json!({ "endpoint": "http://old:8123", "password": "secret" }),
        );

        let mut config = json!({ "endpoint": "http://new:8123" });
        keep_secrets(&mut config, old.as_ref());
        assert_eq!(config["password"], "secret");
        assert_eq!(config["endpoint"], "http://new:8123");

        // A given secret replaces the old one.
        let mut config = json!({ "endpoint": "http://new:8123", "password": "other" });
        keep_secrets(&mut config, old.as_ref());
        assert_eq!(config["password"], "other");
    }
}
