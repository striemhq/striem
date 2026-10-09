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
pub(crate) mod storage;
mod store;
mod vector;

use arc_swap::ArcSwap;

use axum::http::HeaderValue;
pub use server::serve;
use striem_common::SysMessage;

use std::sync::Arc;
use tokio::sync::RwLock;

use striem_config::StrIEMConfig;

use data::{NoData, StrIEMData};
use storage::Storage;
use sinks::Sink;
use sources::Source;
use store::Store;

use striem_detection::sigma_collection_client::SigmaCollectionClient;

/// The gRPC client to the detection microservice's admin API.
pub(crate) type DetectionClient = SigmaCollectionClient<tonic::transport::Channel>;

#[derive(Clone)]
pub(crate) struct ApiState {
    pub detections: DetectionClient,
    /// The active storage backend, if one is configured. The alert and
    /// live-search endpoints read through it (see [`ApiState::data`]).
    pub storage: Arc<RwLock<Option<Arc<dyn Storage>>>>,
    pub features: HeaderValue,
    pub sys: tokio::sync::broadcast::Sender<SysMessage>,
    pub config: Arc<ArcSwap<StrIEMConfig>>,
    pub sources: Arc<RwLock<Vec<Box<dyn Source>>>>,
    pub sinks: Arc<RwLock<Vec<Box<dyn Sink>>>>,
    pub store: Arc<dyn Store>,
}

impl ApiState {
    /// The read access to the stored event data: the active storage, or
    /// [`NoData`] when no storage is configured.
    pub(crate) async fn data(&self) -> Arc<dyn StrIEMData> {
        match self.storage.read().await.as_ref() {
            Some(storage) => storage.clone(),
            None => Arc::new(NoData),
        }
    }
}

#[cfg(not(feature = "mcp"))]
mod actions {
    pub fn create_router() -> axum::Router<crate::ApiState> {
        axum::Router::new()
    }
}
