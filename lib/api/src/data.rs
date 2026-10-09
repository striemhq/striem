//! Read access to the stored event data.
//!
//! The API does not read the stored data itself. [`StrIEMData`] is the trait
//! for the endpoints that show that data: the alerts list, the alert detail,
//! and the live search. A backend implements the trait. The API calls it
//! through `ApiState::data`.
//!
//! A configured [`Storage`](crate::storage::Storage) backend implements the
//! trait. With no storage, the API uses [`NoData`]: the endpoints answer with
//! empty results, and the live search says that no storage is configured.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One row of the alerts list: a summary of a detection finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub time: String,
    pub severity: String,
    pub title: String,
    /// Other fields for the UI. For example, `_file` tells the detail view
    /// where the full finding is.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// The error of a stub method: the operation has no backend. An endpoint
/// answers it with `501 Not Implemented`. A backend error is not this type, so
/// it stays a `500`. The text is the full message for the caller.
#[derive(Debug)]
pub(crate) struct Unavailable(pub &'static str);

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Unavailable {}

/// The HTTP status for an error from a [`StrIEMData`] method:
/// `501 Not Implemented` for [`Unavailable`], else `500`.
pub(crate) fn error_status(e: &anyhow::Error) -> axum::http::StatusCode {
    if e.is::<Unavailable>() {
        axum::http::StatusCode::NOT_IMPLEMENTED
    } else {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// The data source for the alert and live-search endpoints.
#[tonic::async_trait]
pub(crate) trait StrIEMData: Send + Sync {
    /// Lists the newest alerts in the time range `start..=end`, at most `limit`
    /// of them, newest first.
    ///
    /// Stub: gives an empty list.
    async fn list_alerts(
        &self,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
        _limit: usize,
    ) -> Result<Vec<Alert>> {
        Ok(Vec::new())
    }

    /// Gets the full finding of one alert, as JSON. `file` is the `_file` hint
    /// from the alerts list, if the caller has it. Gives `None` when there is no
    /// alert with this id.
    ///
    /// Stub: gives `None`.
    async fn get_alert(&self, _id: &str, _file: Option<&str>) -> Result<Option<Value>> {
        Ok(None)
    }

    /// Runs a live-search query, and gives at most `limit` rows as a JSON
    /// array.
    ///
    /// Stub: gives an error, because there is no search backend.
    async fn search(&self, _query: &str, _limit: usize) -> Result<Value> {
        Err(Unavailable("live search is not available").into())
    }
}

/// A data source with no data. The API uses it when no storage is configured.
pub(crate) struct NoData;

#[tonic::async_trait]
impl StrIEMData for NoData {
    async fn search(&self, _query: &str, _limit: usize) -> Result<Value> {
        Err(Unavailable("no storage is configured: add one under Data > Storage").into())
    }
}
