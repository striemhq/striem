#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::Serialize;

use crate::graph::{Pipeline, Transform, component};
pub mod slack;

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

    /// Discriminator used to persist and reconstruct this sink.
    fn typename(&self) -> String;

    /// The settings needed to reconstruct this sink from storage.
    fn settings(&self) -> serde_json::Value;

    /// Transforms this sink prepends ahead of the sink component (its config's
    /// `inputs` should reference them).
    fn transforms(&self) -> BTreeMap<String, Transform> {
        BTreeMap::new()
    }

    /// The Vector components this sink contributes to the graph.
    fn pipeline(&self) -> anyhow::Result<Pipeline> {
        let mut pipeline = Pipeline::default();
        pipeline.sinks.insert(self.id(), component(self.config())?);
        pipeline.transforms.extend(self.transforms());
        Ok(pipeline)
    }
}

pub type ExistingSink = (String, String, serde_json::Value);

impl TryInto<Box<dyn Sink>> for ExistingSink {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<Box<dyn Sink>, Self::Error> {
        let (sinktype, id, config) = self;
        match sinktype.as_str() {
            "slack" => {
                let settings: slack::SlackSettings =
                    serde_json::from_value(config).map_err(|e| anyhow::anyhow!(e))?;
                Ok(Box::new(slack::Slack {
                    id,
                    token: settings.token,
                    channel: settings.channel,
                }))
            }
            _ => Err(anyhow::anyhow!("Unsupported sink type: {}", sinktype)),
        }
    }
}

