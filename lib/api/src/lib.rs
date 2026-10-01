mod alerts;
mod data;
mod detections;
pub mod features;
mod graph;
mod observability;
mod query;
mod routes;
mod server;
pub(crate) mod sinks;
pub(crate) mod sources;
mod store;
mod vector;

use arc_swap::ArcSwap;

use axum::http::HeaderValue;
pub use server::serve;
use striem_common::SysMessage;

use std::sync::Arc;
use tokio::sync::RwLock;

use striem_config::StrIEMConfig;

use data::StrIEMData;
use sinks::Sink;
use sources::Source;
use store::Store;

use striem_detection::sigma_collection_client::SigmaCollectionClient;

/// The gRPC client to the detection microservice's admin API.
pub(crate) type DetectionClient = SigmaCollectionClient<tonic::transport::Channel>;

#[derive(Clone)]
pub(crate) struct ApiState {
    pub detections: DetectionClient,
    /// The read access to the stored event data (alerts, live search).
    pub data: Arc<dyn StrIEMData>,
    pub features: HeaderValue,
    pub sys: tokio::sync::broadcast::Sender<SysMessage>,
    pub config: Arc<ArcSwap<StrIEMConfig>>,
    pub sources: Arc<RwLock<Vec<Box<dyn Source>>>>,
    pub sinks: Arc<RwLock<Vec<Box<dyn Sink>>>>,
    pub store: Arc<dyn Store>,
}

#[cfg(not(feature = "mcp"))]
mod actions {
    pub fn create_router() -> axum::Router<crate::ApiState> {
        axum::Router::new()
    }
}
