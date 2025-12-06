//! Output destination configuration for forwarding events.
//!
//! Defines where StrIEM sends processed events and detection findings.
//! Supports Vector (for downstream pipelines) and HTTP endpoints.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use striem_common::prelude::*;

use crate::HostConfig;

/// Vector destination configuration
///
/// Configures both the destination StrIEM sends detection matches, and the configuration
/// StrIEM generates for Vector
///
/// # Optional Endpoints
/// - `hec`: Splunk HEC endpoint listener configuration
///   - **Use Case**: Enables Vector's HEC listener, for receiving events from Splunk
///     or Github Enterprise Audit logs.
/// - `http`: HTTP listener configuration
///   - **Use Case**: Enables Vector's HTTP listener, for receiving events from webhooks
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
    /// Primary Vector gRPC endpoint configuration
    pub cfg: HostConfig,
    /// Optional Splunk HEC endpoint for Vector to forward events
    pub hec: Option<HostConfig>,
    /// Optional HTTP endpoint for Vector to forward events
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
            && hec.port == 0 {
                hec.port = DEFAULT_VECTOR_HEC_LISTEN_PORT;
            }
        if let Some(http) = &mut helper.http
            && http.port == 0 {
                http.port = DEFAULT_VECTOR_HTTP_LISTEN_PORT;
            }
        if let Some(api) = &mut helper.api
            && api.port == 0 {
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

/// Output destination for processed events and detection findings.
///
/// StrIEM can forward events to downstream systems for additional processing,
/// alerting, or long-term storage. The destination type determines the protocol
/// and endpoint configuration.
///
/// # Variants
/// - `Vector`: Forward to downstream Vector instance (most common)
/// - `Http`: Forward to HTTP endpoint (webhooks, custom receivers)
///
/// # Use Cases
/// - **Vector**: Chain multiple StrIEM instances or forward to Vector sinks
/// - **Http**: Send to alerting systems, ticketing, or custom integrations
///
/// # Example
/// ```yaml
/// # Forward detection findings to downstream Vector
/// output:
///   vector:
///     url: http://downstream-vector:9000
/// ```
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    /// Forward events to a Vector instance via gRPC
    Vector(Box<VectorDestinationConfig>),
    /// Forward events to an HTTP endpoint
    Http(Box<HostConfig>),
}

impl Destination {
    pub fn url(&self) -> String {
        match self {
            Destination::Vector(vector) => vector.cfg.url(),
            Destination::Http(cfg) => cfg.url(),
        }
    }
    pub fn address(&self) -> SocketAddr {
        match self {
            Destination::Vector(cfg) => cfg.cfg.address(),
            Destination::Http(cfg) => cfg.address(),
        }
    }
}
