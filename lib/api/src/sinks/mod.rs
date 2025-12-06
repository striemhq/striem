#![allow(dead_code)]

use std::sync::LazyLock;

use serde::{Serialize, ser::SerializeMap};
use tokio::sync::RwLock;

pub(crate) static SINKS: LazyLock<RwLock<Vec<Sink>>> = LazyLock::new(|| RwLock::new(Vec::new()));

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    #[default]
    Json,
    Text,
}

#[derive(Serialize, Clone, Default)]
pub struct Encoding {
    #[serde(default)]
    pub codec: Codec,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SinkType {
    Http {
        uri: String,
        encoding: Encoding,
        inputs: Vec<String>,
    },
    Vector {
        address: String,
        port: u16,
        encoding: Encoding,
        inputs: Vec<String>,
    },
    Blackhole {
        inputs: Vec<String>,
    },
}

pub struct Sink {
    pub id: String,
    pub config: SinkType,
}

impl Serialize for Sink {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut state = serializer.serialize_map(Some(1))?;
        state.serialize_entry(&format!("sink-{}", self.id.clone()), &self.config.clone())?;
        state.end()
    }
}
