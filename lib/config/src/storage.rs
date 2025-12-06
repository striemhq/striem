use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageConfig {
    pub schema: String,
    pub path: String,
}
