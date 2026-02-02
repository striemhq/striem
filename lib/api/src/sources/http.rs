use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Source, SourceType};

pub struct HttpRoute {
    pub(crate) id: String,
    pub(crate) config: HttpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub(crate) name: Option<String>,
    pub(crate) logsource: BTreeMap<String, String>,
    pub(crate) vrl: String,
}

impl Source for HttpRoute {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn name(&self) -> String {
        self.config.name.clone().unwrap_or_else(|| self.id.clone())
    }

    fn sourcetype(&self) -> SourceType {
        SourceType::Http
    }

    fn config(&self) -> &dyn erased_serde::Serialize {
        &self.config
    }

    fn logsource_vendor(&self) -> Option<String> {
        self.config.logsource.get("vendor").cloned()
    }

    fn logsource_product(&self) -> Option<String> {
        self.config.logsource.get("product").cloned()
    }

    fn logsource_service(&self) -> Option<String> {
        self.config.logsource.get("service").cloned()
    }

    fn preprocess_transforms(
        &self,
    ) -> Option<(std::collections::BTreeMap<String, super::Transform>, String)> {
        let transforms = std::collections::BTreeMap::from([(
            format!("ocsf-{}_{}", self.sourcetype(), self.id()),
            super::Transform {
                inputs: vec![],
                source: Some(self.config.vrl.clone()),
                file: None,
                ..Default::default()
            },
        )]);
        Some((transforms, "".to_string()))
    }
}
