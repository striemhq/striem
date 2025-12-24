//! Configuration management for StrIEM.
//!
//! Uses [Config](https://docs.rs/config/latest/config/index.html), supports loading from:
//! - Configuration files (YAML, JSON, TOML)
//! - Environment variables (STRIEM_ prefix)
//! - Defaults
//!
//! Environment variables override file settings, enabling Docker/K8s deployments
//! without rebuilding config files.

use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    path::PathBuf,
};
use url::Url;

use anyhow::{Result, anyhow};
use config::Config;
use serde::{Deserialize, Serialize};

pub mod api;
pub mod input;
pub mod output;
pub mod storage;

mod tests;

/// Configuration value that accepts either a single string or array of strings
#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum StringOrList {
    String(String),
    List(Vec<String>),
}

#[derive(Debug, Serialize, Clone)]
pub struct HostConfig {
    pub address: Option<SocketAddr>,
    pub url: Option<Url>,
    pub port: u16,
}

impl Default for HostConfig {
    fn default() -> Self {
        HostConfig {
            address: Some(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))),
            url: None,
            port: 0,
        }
    }
}

impl<'de> Deserialize<'de> for HostConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct HostConfigHelper {
            address: Option<SocketAddr>,
            url: Option<Url>,
            port: Option<u16>,
        }

        let helper = HostConfigHelper::deserialize(deserializer)?;

        if helper.address.is_none() && helper.url.is_none() {
            return Err(serde::de::Error::custom(
                "HostConfig requires either 'address' or 'url'",
            ));
        }

        let port = if let Some(p) = helper.port {
            p
        } else if let Some(addr) = helper.address
            && addr.port() != 0
        {
            addr.port()
        } else if let Some(url) = &helper.url {
            url.port().unwrap_or(0)
        } else {
            unreachable!()
        };

        Ok(HostConfig {
            address: helper.address,
            url: helper.url,
            port,
        })
    }
}

impl HostConfig {
    pub fn address(&self) -> SocketAddr {
        if let Some(addr) = self.address {
            if addr.port() == 0 {
                let mut addr = addr;
                addr.set_port(self.port);
                addr
            } else {
                addr
            }
        } else {
            match &self.url {
                Some(url) => match url.host() {
                    Some(url::Host::Domain(host)) => {
                        if host == "localhost" {
                            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, self.port))
                        } else {
                            host.parse::<SocketAddr>().unwrap_or_else(|_| {
                                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, self.port))
                            })
                        }
                    }
                    Some(url::Host::Ipv4(ip)) => SocketAddr::V4(SocketAddrV4::new(ip, self.port)),
                    Some(url::Host::Ipv6(ip)) => {
                        SocketAddr::V6(SocketAddrV6::new(ip, self.port, 0, 0))
                    }
                    None => SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, self.port)),
                },
                None => unreachable!(),
            }
        }
    }

    pub fn url(&self) -> String {
        match &self.url {
            Some(url) => url.to_string(),
            None => {
                if self.address().ip().is_unspecified() {
                    format!("http://localhost:{}", self.port)
                } else {
                    format!("http://{}", self.address())
                }
            }
        }
    }
    pub fn set_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
}

const CWD: fn() -> PathBuf = || {
    std::env::current_dir()
        .unwrap_or_else(|e| panic!("Failed to get current working directory: {}", e))
};

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
struct StrIEMConfigOptions {
    /// Path to the StrIEM source configuration & rule database
    /// (defaults to current working directory)
    #[serde(default = "CWD")]
    db: PathBuf,

    /// Location of top-level Sigma detection directory
    /// (can be a list or single path)
    #[serde(with = "serde_yaml::with::singleton_map")]
    detections: Option<StringOrList>,

    /// Input listener configuration
    #[serde(with = "serde_yaml::with::singleton_map")]
    input: Option<input::Listener>,

    /// Output destination configuration
    #[serde(with = "serde_yaml::with::singleton_map")]
    output: Option<output::Destination>,

    /// Storage backend configuration
    storage: Option<storage::StorageConfig>,

    /// API server configuration
    api: Option<api::ApiConfig>,

    /// Fully qualified domain name for this StrIEM instance
    fqdn: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StrIEMConfig {
    pub db: Option<PathBuf>,

    pub detections: Option<StringOrList>,

    pub input: input::Listener,

    pub output: Option<output::Destination>,

    pub storage: Option<storage::StorageConfig>,

    pub api: api::ApiConfig,

    pub fqdn: Option<String>,
}

impl From<StrIEMConfigOptions> for StrIEMConfig {
    fn from(val: StrIEMConfigOptions) -> Self {
        StrIEMConfig {
            db: Some(val.db.clone()),
            detections: val.detections,
            input: val.input.unwrap_or_default(),
            output: val.output,
            storage: val.storage,
            api: val.api.unwrap_or_default(),
            fqdn: val.fqdn,
        }
    }
}

impl StrIEMConfig {
    pub fn new() -> Result<Self> {
        let builder = Config::builder()
            .add_source(config::File::from_str(
                serde_json::to_string(&StrIEMConfigOptions::default())?.as_str(),
                config::FileFormat::Json,
            ))
            .add_source(config::Environment::with_prefix("STRIEM").separator("_"))
            .build()?;

        let config: StrIEMConfigOptions = builder.try_deserialize()?;

        Self::check(&config)?;

        Ok(config.into())
    }

    pub fn from_file(file: &str) -> Result<Self> {
        let builder = Config::builder()
            .add_source(config::File::from_str(
                serde_json::to_string(&StrIEMConfigOptions::default())?.as_str(),
                config::FileFormat::Json,
            ))
            .add_source(config::File::with_name(file))
            .add_source(config::Environment::with_prefix("STRIEM").separator("_"))
            .build()?;

        let config: StrIEMConfigOptions = builder.try_deserialize()?;
        Self::check(&config)?;

        Ok(config.into())
    }

    pub fn from_multi_file(files: Vec<PathBuf>) -> Result<Self> {
        let mut builder = Config::builder().add_source(config::File::from_str(
            serde_json::to_string(&StrIEMConfigOptions::default())?.as_str(),
            config::FileFormat::Json,
        ));

        builder = builder.add_source(config::Environment::with_prefix("STRIEM").separator("_"));

        for file in files {
            if let Some(filename) = file.to_str() {
                builder = builder.add_source(config::File::with_name(filename));
            } else {
                log::error!("Invalid config file path: {:?}", file);
            }
        }

        let built = builder.build()?;

        let config: StrIEMConfigOptions = built.try_deserialize()?;
        Self::check(&config)?;

        Ok(config.into())
    }

    pub fn from_yaml(s: &str) -> Result<Self> {
        let builder = Config::builder()
            .add_source(config::File::from_str(
                serde_json::to_string(&StrIEMConfigOptions::default())?.as_str(),
                config::FileFormat::Json,
            ))
            .add_source(config::File::from_str(s, config::FileFormat::Yaml))
            .add_source(config::Environment::with_prefix("STRIEM").separator("_"))
            .build()?;

        let config: StrIEMConfigOptions = builder.try_deserialize()?;
        Self::check(&config)?;

        Ok(config.into())
    }

    pub fn from_json(s: &str) -> Result<Self> {
        let builder = Config::builder()
            .add_source(config::File::from_str(
                serde_json::to_string(&StrIEMConfigOptions::default())?.as_str(),
                config::FileFormat::Json,
            ))
            .add_source(config::File::from_str(s, config::FileFormat::Json))
            .add_source(config::Environment::with_prefix("STRIEM").separator("_"))
            .build()?;

        let config: StrIEMConfigOptions = builder.try_deserialize()?;
        Self::check(&config)?;

        Ok(config.into())
    }

    pub fn from_toml(s: &str) -> Result<Self> {
        let builder = Config::builder()
            .add_source(config::File::from_str(
                serde_json::to_string(&StrIEMConfigOptions::default())?.as_str(),
                config::FileFormat::Json,
            ))
            .add_source(config::File::from_str(s, config::FileFormat::Toml))
            .add_source(config::Environment::with_prefix("STRIEM").separator("_"))
            .build()?;

        let config: StrIEMConfigOptions = builder.try_deserialize()?;
        Self::check(&config)?;

        Ok(config.into())
    }

    fn check(config: &StrIEMConfigOptions) -> Result<()> {
        let api = if let Some(ref api) = config.api {
            api.enabled
        } else {
            false
        };
        if !(config.output.is_some() || config.storage.is_some()) {
            if !api {
                Err(anyhow!(
                    "No output, storage, or API configured; StrIEM cannot run"
                ))?
            }
            log::warn!("No output or storage configured; events will be dropped");
        }
        Ok(())
    }
}
