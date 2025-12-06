use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use striem_common::prelude::*;

use crate::HostConfig;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Listener {
    Vector(HostConfig),
    Http(HostConfig),
}

impl Default for Listener {
    fn default() -> Self {
        Listener::Vector(HostConfig::default().set_port(DEFAULT_STRIEM_LISTEN_PORT))
    }
}

impl Listener {
    pub fn url(&self) -> String {
        match self {
            Listener::Vector(vector) => vector.url(),
            Listener::Http(cfg) => cfg.url(),
        }
    }
    pub fn address(&self) -> SocketAddr {
        match self {
            Listener::Vector(cfg) => cfg.address(),
            Listener::Http(cfg) => cfg.address(),
        }
    }
}
