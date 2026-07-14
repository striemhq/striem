use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageConfig {
    /// Directory where Vector writes OCSF parquet; the API queries it via DuckDB.
    pub path: PathBuf,
}
