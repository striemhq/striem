//! Slack notification. It sends alerts to a channel with `chat.postMessage`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::graph::Transform;
use crate::sinks::{
    AuthConfig, BatchConfig, Codec, Encoding, Framing, Sink, SinkCategory, SinkType, ALERTS_INPUT,
};

#[derive(Serialize, Deserialize, Clone)]
pub struct SlackSettings {
    pub token: String,
    pub channel: String,
}

pub struct Slack {
    pub id: String,
    pub settings: SlackSettings,
}

impl Slack {
    fn pre_id(&self) -> String {
        format!("sink-pre-slack_{}", self.id)
    }

    /// The VRL that makes a Slack `chat.postMessage` payload from an alert.
    fn vrl(&self) -> String {
        format!(
            r#"
            msg = {{}}
            msg.channel = "{}"
            msg.text = .message || .finding_info.title || "Alert"
            . = msg
            "#,
            self.settings.channel
        )
    }
}

impl Sink for Slack {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn category(&self) -> SinkCategory {
        SinkCategory::Notification
    }

    fn typename(&self) -> String {
        "slack".to_string()
    }

    fn name(&self) -> String {
        format!("Slack ({})", self.settings.channel)
    }

    fn config(&self) -> SinkType {
        SinkType::Http {
            uri: "https://slack.com/api/chat.postMessage".to_string(),
            encoding: Encoding {
                codec: Codec::Json,
                only_fields: Some(vec![
                    "channel".to_string(),
                    "text".to_string(),
                    "blocks".to_string(),
                ]),
            },
            inputs: vec![self.pre_id()],
            auth: Some(AuthConfig {
                strategy: "bearer".to_string(),
                token: self.settings.token.clone(),
            }),
            batch: Some(BatchConfig { max_events: 1 }),
            framing: Some(Framing {
                method: "bytes".to_string(),
            }),
        }
    }

    fn settings(&self) -> serde_json::Value {
        serde_json::json!({ "token": self.settings.token, "channel": self.settings.channel })
    }

    fn transforms(&self) -> BTreeMap<String, Transform> {
        BTreeMap::from([(
            self.pre_id(),
            Transform::remap(self.vrl()).with_inputs([ALERTS_INPUT]),
        )])
    }
}
