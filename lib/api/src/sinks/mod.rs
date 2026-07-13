#![allow(dead_code)]

use std::{collections::BTreeMap, sync::LazyLock};

use serde::{Serialize, ser::SerializeMap};
use tokio::sync::RwLock;

use crate::sources::{Transform, TransformType};
pub mod slack;

pub(crate) static SINKS: LazyLock<RwLock<Vec<Box<dyn Sink>>>> = LazyLock::new(|| RwLock::new(Vec::new()));

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only_fields: Option<Vec<String>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct BatchConfig {
    pub max_events: i32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct Framing {
    pub method: String,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct AuthConfig {
    pub strategy: String,
    pub token: String,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SinkType {
    Http {
        uri: String,
        encoding: Encoding,
        inputs: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<AuthConfig>,
        #[serde(skip_serializing_if = "Option::is_none")]
        batch: Option<BatchConfig>,
        #[serde(skip_serializing_if = "Option::is_none")]
        framing: Option<Framing>,
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

pub trait Sink: Send + Sync {
    fn id(&self) -> String;
    fn config(&self) -> SinkType;
    fn pre(&self) -> Option<(BTreeMap<String, Transform>, String)> {
        None
    }
}

impl Serialize for dyn Sink {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {

        let transforms = self.pre();

        let len = match &transforms {
            Some(_) => 2,
            None => 1,
        };

        let mut map = serializer.serialize_map(Some(len))?;

        if let Some((transforms, _)) = transforms {
            map.serialize_entry("sinks", &BTreeMap::from([(self.id(), &self.config())]))?;
            map.serialize_entry("transforms", &transforms)?;
        } else {
            map.serialize_entry("sinks", &BTreeMap::from([(self.id(), &self.config())]))?;
        }
        map.end()
    }
}

