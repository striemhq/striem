use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageConfig {
    /// The directory where Vector writes OCSF parquet. The API queries this
    /// directory with DuckDB.
    pub path: PathBuf,
}
