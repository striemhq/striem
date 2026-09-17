//! Output destination configuration. StrIEM sends detection findings to a
//! downstream Vector instance.

use serde::{Deserialize, Serialize};

use striem_common::prelude::*;

use crate::HostConfig;

/// Vector destination configuration.
///
/// This struct configures two things. It configures the destination where
/// StrIEM sends detection matches. It also configures the Vector configuration
/// that StrIEM makes.
///
/// # Optional Endpoints
/// - `hec`: the configuration of the Splunk HEC listener.
///   - **Use Case**: it starts Vector's HEC listener. The listener receives
///     events from Splunk or from GitHub Enterprise audit logs.
/// - `http`: the configuration of the HTTP listener.
///   - **Use Case**: it starts Vector's HTTP listener. The listener receives
///     events from webhooks.
///
/// # Example
/// ```yaml
/// output:
///   vector:
///     address: 0.0.0.0:9000
///     url: http://localhost:9000
/// ```
#[derive(Debug, Serialize, Clone)]
pub struct VectorDestinationConfig {
    /// The configuration of the primary Vector gRPC endpoint.
    pub cfg: HostConfig,
    /// The optional Splunk HEC endpoint. Vector sends events to it.
    pub hec: Option<HostConfig>,
    /// The optional HTTP endpoint. Vector sends events to it.
    pub http: Option<HostConfig>,
    pub api: Option<HostConfig>,
}

impl<'de> Deserialize<'de> for VectorDestinationConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(flatten)]
            cfg: HostConfig,
            hec: Option<HostConfig>,
            http: Option<HostConfig>,
            api: Option<HostConfig>,
        }

        let mut helper = Helper::deserialize(deserializer)?;

        if helper.cfg.port == 0 {
            helper.cfg.port = DEFAULT_VECTOR_LISTEN_PORT;
        }
        if let Some(hec) = &mut helper.hec
            && hec.port == 0
        {
            hec.port = DEFAULT_VECTOR_HEC_LISTEN_PORT;
        }
        if let Some(http) = &mut helper.http
            && http.port == 0
        {
            http.port = DEFAULT_VECTOR_HTTP_LISTEN_PORT;
        }
        if let Some(api) = &mut helper.api
            && api.port == 0
        {
            api.port = DEFAULT_VECTOR_API_LISTEN_PORT;
        }
        Ok(VectorDestinationConfig {
            cfg: helper.cfg,
            hec: helper.hec,
            http: helper.http,
            api: helper.api,
        })
    }
}

