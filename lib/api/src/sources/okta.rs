use serde::{Deserialize, Serialize};

use super::{Source, SourceType};

#[derive(Debug, Clone, Serialize)]
pub struct OktaConfig {
    #[serde(rename = "type")]
    _type: String,
    pub domain: String,
    pub token: String,
    pub scrape_interval_secs: Option<u64>,
    pub scrape_timeout_secs: Option<u64>,
    pub since: Option<u64>,
}

impl<'de> Deserialize<'de> for OktaConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct OktaConfigHelper {
            pub domain: String,
            pub token: String,
            pub scrape_interval_secs: Option<u64>,
            pub scrape_timeout_secs: Option<u64>,
            pub since: Option<u64>,
        }

        let helper = OktaConfigHelper::deserialize(deserializer)?;
        Ok(OktaConfig {
            _type: "okta".into(),
            domain: helper.domain,
            token: helper.token,
            scrape_interval_secs: helper.scrape_interval_secs,
            scrape_timeout_secs: helper.scrape_timeout_secs,
            since: helper.since,
        })
    }
}

pub struct Okta {
    pub(super) id: String,
    pub(super) config: OktaConfig,
}

impl Source for Okta {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn name(&self) -> String {
        self.config.domain.clone()
    }

    fn sourcetype(&self) -> SourceType {
        SourceType::Okta
    }

    fn config(&self) -> &dyn erased_serde::Serialize {
        &self.config
    }

    fn logsource_vendor(&self) -> Option<String> {
        Some("okta".to_string())
    }

    fn logsource_product(&self) -> Option<String> {
        Some("audit".to_string())
    }
}
