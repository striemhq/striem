//! Webhook notification. It sends each alert as JSON to any URL with an HTTP
//! POST.

use serde::{Deserialize, Serialize};

use crate::sinks::{AuthConfig, Codec, Encoding, Sink, SinkCategory, SinkType, ALERTS_INPUT};

#[derive(Serialize, Deserialize, Clone)]
pub struct WebhookSettings {
    pub url: String,
    /// The optional bearer token. The sink sends it as `Authorization: Bearer …`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

pub struct Webhook {
    pub id: String,
    pub settings: WebhookSettings,
}

impl Sink for Webhook {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn category(&self) -> SinkCategory {
        SinkCategory::Notification
    }

    fn typename(&self) -> String {
        "webhook".to_string()
    }

    fn name(&self) -> String {
        format!("Webhook ({})", self.settings.url)
    }

    fn config(&self) -> SinkType {
        let auth = self.settings.token.as_ref().filter(|t| !t.is_empty()).map(|t| AuthConfig {
            strategy: "bearer".to_string(),
            token: t.clone(),
        });
        SinkType::Http {
            uri: self.settings.url.clone(),
            encoding: Encoding {
                codec: Codec::Json,
                only_fields: None,
            },
            inputs: vec![ALERTS_INPUT.to_string()],
            auth,
            batch: None,
            framing: None,
        }
    }

    fn settings(&self) -> serde_json::Value {
        serde_json::to_value(&self.settings).unwrap_or_default()
    }
}
