//! AWS S3 destination. It keeps the OCSF event stream in an S3 bucket.

use serde::{Deserialize, Serialize};

use crate::sinks::{Encoding, S3Auth, Sink, SinkCategory, SinkType, OCSF_INPUT};

#[derive(Serialize, Deserialize, Clone)]
pub struct AwsS3Settings {
    pub bucket: String,
    pub region: String,
    #[serde(default)]
    pub key_prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_key_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_access_key: Option<String>,
}

pub struct AwsS3 {
    pub id: String,
    pub settings: AwsS3Settings,
}

impl Sink for AwsS3 {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn category(&self) -> SinkCategory {
        SinkCategory::Destination
    }

    fn typename(&self) -> String {
        "aws_s3".to_string()
    }

    fn name(&self) -> String {
        format!("AWS S3 ({})", self.settings.bucket)
    }

    fn config(&self) -> SinkType {
        // Use fixed credentials only when you give both values. If not, Vector
        // uses the default AWS credential chain (IAM role, environment, and
        // others).
        let auth = match (&self.settings.access_key_id, &self.settings.secret_access_key) {
            (Some(id), Some(secret)) if !id.is_empty() && !secret.is_empty() => Some(S3Auth {
                access_key_id: id.clone(),
                secret_access_key: secret.clone(),
            }),
            _ => None,
        };
        SinkType::AwsS3 {
            bucket: self.settings.bucket.clone(),
            region: self.settings.region.clone(),
            key_prefix: self.settings.key_prefix.clone(),
            encoding: Encoding::default(),
            inputs: vec![OCSF_INPUT.to_string()],
            auth,
        }
    }

    fn settings(&self) -> serde_json::Value {
        serde_json::to_value(&self.settings).unwrap_or_default()
    }
}
