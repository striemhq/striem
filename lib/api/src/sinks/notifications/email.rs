//! Email notification.
//!
//! Vector has no native SMTP sink, so email is delivered by POSTing to an
//! email-provider HTTP API (e.g. SendGrid/Mailgun-style endpoint). A transform
//! shapes each alert into a `{to, from, subject, body}` payload.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::graph::Transform;
use crate::sinks::{AuthConfig, Codec, Encoding, Sink, SinkCategory, SinkType, ALERTS_INPUT};

#[derive(Serialize, Deserialize, Clone)]
pub struct EmailSettings {
    /// Email-provider HTTP API endpoint the payload is POSTed to.
    pub endpoint: String,
    pub to: String,
    pub from: String,
    #[serde(default = "default_subject")]
    pub subject: String,
    /// Optional bearer token for the provider API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

fn default_subject() -> String {
    "StrIEM Alert".to_string()
}

pub struct Email {
    pub id: String,
    pub settings: EmailSettings,
}

impl Email {
    fn pre_id(&self) -> String {
        format!("sink-pre-email_{}", self.id)
    }

    fn vrl(&self) -> String {
        format!(
            r#"
            msg = {{}}
            msg.to = "{to}"
            msg.from = "{from}"
            msg.subject = "{subject}"
            msg.body = .message || .finding_info.title || "Alert"
            . = msg
            "#,
            to = self.settings.to,
            from = self.settings.from,
            subject = self.settings.subject,
        )
    }
}

impl Sink for Email {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn category(&self) -> SinkCategory {
        SinkCategory::Notification
    }

    fn typename(&self) -> String {
        "email".to_string()
    }

    fn name(&self) -> String {
        format!("Email ({})", self.settings.to)
    }

    fn config(&self) -> SinkType {
        let auth = self.settings.token.as_ref().filter(|t| !t.is_empty()).map(|t| AuthConfig {
            strategy: "bearer".to_string(),
            token: t.clone(),
        });
        SinkType::Http {
            uri: self.settings.endpoint.clone(),
            encoding: Encoding {
                codec: Codec::Json,
                only_fields: None,
            },
            inputs: vec![self.pre_id()],
            auth,
            batch: None,
            framing: None,
        }
    }

    fn settings(&self) -> serde_json::Value {
        serde_json::to_value(&self.settings).unwrap_or_default()
    }

    fn transforms(&self) -> BTreeMap<String, Transform> {
        BTreeMap::from([(
            self.pre_id(),
            Transform::remap(self.vrl()).with_inputs([ALERTS_INPUT]),
        )])
    }
}
