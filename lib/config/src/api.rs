use serde::{Deserialize, Serialize};

use crate::{HostConfig, StringOrList};
use striem_common::prelude::*;

const TRUE: fn() -> bool = || true;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MCPConfig {
    pub url: StringOrList,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct UIConfig {
    #[serde(default = "TRUE")]
    pub enabled: bool,
    pub path: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ApiConfig {
    pub enabled: bool,
    pub data: Option<String>,
    pub mcp: Option<MCPConfig>,
    pub ui: Option<UIConfig>,
    pub host: HostConfig,
}

impl<'de> Deserialize<'de> for ApiConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            enabled: Option<bool>,
            #[serde(flatten)]
            host: Option<HostConfig>,
            data: Option<String>,
            mcp: Option<MCPConfig>,
            ui: Option<UIConfig>,
        }

        let helper = Helper::deserialize(deserializer)?;

        let enabled = helper
            .enabled
            .unwrap_or_else(|| helper.host.is_some() || helper.ui.is_some());

        Ok(ApiConfig {
            enabled,
            host: helper
                .host
                .unwrap_or_else(|| HostConfig::default().set_port(DEFAULT_API_LISTEN_PORT)),
            data: helper.data,
            mcp: helper.mcp,
            ui: helper.ui,
        })
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        ApiConfig {
            enabled: true,
            host: HostConfig::default().set_port(DEFAULT_API_LISTEN_PORT),
            data: None,
            mcp: None,
            ui: Some(UIConfig::default()),
        }
    }
}
