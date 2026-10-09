//! AWS Athena storage. **Not implemented yet.**
//!
//! The service saves the settings and shows them in the UI, but each read gives
//! `501 Not Implemented`. A full backend would run the queries with the Athena
//! API (`StartQueryExecution`, then `GetQueryResults`) over the OCSF tables in
//! `database`, with the AWS default credential chain.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::data::{Alert, StrIEMData, Unavailable};

use super::Storage;

const NOT_IMPLEMENTED: &str = "Athena storage is not implemented yet";

#[derive(Serialize, Deserialize, Clone)]
pub struct AthenaSettings {
    pub region: String,
    /// The Glue database that has the OCSF tables.
    pub database: String,
    /// The Athena workgroup. The default is `primary`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workgroup: Option<String>,
    /// The S3 location for query results, for example `s3://bucket/athena/`.
    /// A workgroup can set it instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_location: Option<String>,
}

pub struct Athena {
    pub id: String,
    pub settings: AthenaSettings,
}

#[tonic::async_trait]
impl StrIEMData for Athena {
    async fn list_alerts(
        &self,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
        _limit: usize,
    ) -> Result<Vec<Alert>> {
        Err(Unavailable(NOT_IMPLEMENTED).into())
    }

    async fn get_alert(&self, _id: &str, _file: Option<&str>) -> Result<Option<Value>> {
        Err(Unavailable(NOT_IMPLEMENTED).into())
    }

    async fn search(&self, _query: &str, _limit: usize) -> Result<Value> {
        Err(Unavailable(NOT_IMPLEMENTED).into())
    }
}

#[tonic::async_trait]
impl Storage for Athena {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn typename(&self) -> String {
        "athena".to_string()
    }

    fn name(&self) -> String {
        format!("AWS Athena ({}, {})", self.settings.database, self.settings.region)
    }

    fn settings(&self) -> Value {
        serde_json::to_value(&self.settings).unwrap_or_default()
    }

    fn implemented(&self) -> bool {
        false
    }

    async fn check(&self) -> Result<()> {
        Err(Unavailable(NOT_IMPLEMENTED).into())
    }
}
