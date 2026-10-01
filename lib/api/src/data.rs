//! Read access to the stored event data.
//!
//! The API does not read the stored data itself. [`StrIEMData`] is the trait
//! for the endpoints that show that data: the alerts list, the alert detail,
//! and the live search. A backend implements the trait. The API calls it
//! through `ApiState::data`.
//!
//! The methods of the trait are stubs by default. Thus [`NoData`] needs no
//! code, and the endpoints answer with empty results (or with "not available"
//! for the live search) until a backend is available.

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
/// it stays a `500`.
#[derive(Debug)]
pub(crate) struct Unavailable(pub &'static str);

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not available", self.0)
    }
}

impl std::error::Error for Unavailable {}

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
        Err(Unavailable("live search").into())
    }
}

/// A data source with no data. The API uses it until a backend is available.
pub(crate) struct NoData;

impl StrIEMData for NoData {}
