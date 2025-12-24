use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageConfig {
    pub schema: PathBuf,
    pub path: PathBuf,
}
