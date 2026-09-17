use serde::{Deserialize, Serialize};

use erased_serde as es;
use std::time::Duration;

use super::{Decoding, Logsource, Source, SourceType};
use crate::graph::Transform;

#[derive(Serialize, Deserialize)]
pub struct ImdsAuthentication {
    max_attempts: u32,
    connect_timeout: Duration,
    read_timeout: Duration,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum AwsAuthentication {
    AccessKey {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
        assume_role: Option<String>,
        external_id: Option<String>,
        region: Option<String>,
        session_name: Option<String>,
    },
    File {
        credentials_file: String,
        profile: String,
        region: Option<String>,
    },
    Role {
        assume_role: String,
        external_id: Option<String>,
        imds: ImdsAuthentication,
        region: Option<String>,
        session_name: Option<String>,
    },
    Default {
        imds: Option<ImdsAuthentication>,
        region: Option<String>,
    },
}
impl Default for AwsAuthentication {
    fn default() -> Self {
        AwsAuthentication::Default {
            imds: None,
            region: None,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct SqsConfig {
    queue_url: String,
}

#[derive(Serialize, Default)]
pub struct AwsCloudtrailConfig {
    #[serde(rename = "type")]
    _type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AwsAuthentication>,
    pub sqs: SqsConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub decoding: Decoding,
}

impl<'de> Deserialize<'de> for AwsCloudtrailConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        #[derive(Deserialize, Default)]
        struct AwsCloudtrailConfigHelper {
            pub auth: Option<AwsAuthentication>,
            pub sqs: SqsConfig,
            pub region: Option<String>,
        }

        let helper = AwsCloudtrailConfigHelper::deserialize(deserializer)?;
        Ok(AwsCloudtrailConfig {
            _type: "aws_s3".to_string(),
            auth: helper.auth,
            sqs: helper.sqs,
            region: helper.region,
            ..Default::default()
        })
    }
}

pub struct AwsCloudtrail {
    pub(super) id: String,
    pub(super) config: AwsCloudtrailConfig,
}

impl Source for AwsCloudtrail {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn name(&self) -> String {
        self.config.sqs.queue_url.clone()
    }

    fn sourcetype(&self) -> SourceType {
        SourceType::AwsCloudtrail
    }

    fn config(&self) -> &dyn es::Serialize {
        &self.config
    }

    fn logsource(&self) -> Logsource {
        Logsource {
            product: Some("aws".to_string()),
            service: Some("cloudtrail".to_string()),
            ..Default::default()
        }
    }

    /// CloudTrail sends events in a `Records` array. Take the events out of the
    /// array before normalization.
    fn preprocess(&self) -> Option<Transform> {
        Some(Transform::remap(". = .Records"))
    }
}
