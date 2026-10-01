//! HTTP API server for the StrIEM management interface.
//!
//! This module gives REST endpoints for these tasks:
//! - Source management (add or remove a data source)
//! - Detection rule management (list, enable, disable, or upload a rule)
//! - Alerts and live search (read through [`StrIEMData`](crate::data::StrIEMData))
//! - Vector configuration generation
//!
//! # Architecture
//! - Axum does the HTTP routing and the middleware.
//! - Tower HTTP does the CORS and serves the static files.
//! - A shared state (Arc) holds the detection rules and the configuration.

use std::sync::Arc;

use anyhow::Result;
use arc_swap::ArcSwap;
use axum::http::HeaderValue;
use axum::middleware;
use log::{error, info};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use striem_detection::sigma_collection_client::SigmaCollectionClient;

use striem_config::StrIEMConfig;

use striem_common::SysMessage;

use crate::{
    ApiState, data::NoData, features::feature_flag_middleware, routes::create_router,
    store::{JsonFileStore, STORE_FILE, Store},
};

/// Starts the API server and runs it.
///
/// # UI Serving
/// This function serves the Next.js static export. It reads the files from the
/// binary path or the configured `ui.path`. It also redirects / to /ui.
pub async fn serve(
    config: &Arc<ArcSwap<StrIEMConfig>>,
    sys: tokio::sync::broadcast::Sender<SysMessage>,
) -> Result<()> {
    let config_container = config.clone();
    let config = config.load();

    // The sources and sinks are always saved (see the store below), so the UI
    // can offer to add them.
    let features: Vec<String> = vec!["persistence".to_string()];

    // The detection microservice manages the detection rules. The API reaches it
    // over gRPC. The connection is lazy, so the API can start before the
    // detection service is available. The first request makes the connection.
    let endpoint = config.detection_endpoint();
    let detections = SigmaCollectionClient::new(
        tonic::transport::Endpoint::from_shared(endpoint.clone())
            .map_err(|e| anyhow::anyhow!("invalid detection endpoint {}: {}", endpoint, e))?
            .connect_lazy(),
    );
    info!("detection admin client targeting {}", endpoint);

    // The API does not read the stored event data. The data endpoints use the
    // stub `NoData`.
    //
    // The sources and sinks are saved in a JSON file in the `db` directory
    // (STRIEM_DB), or in the working directory if that is not set. A store file
    // that is not valid stops the start, so that no change writes over it.
    let store_path = config
        .db
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(STORE_FILE);
    let store: Arc<dyn Store> = Arc::new(JsonFileStore::open(&store_path)?);
    info!("sources and sinks are saved in {}", store_path.display());
    let sources = Arc::new(RwLock::new(store.load_sources().unwrap_or_default()));
    let sinks = Arc::new(RwLock::new(store.load_sinks().unwrap_or_default()));

    let ui = config
        .api
        .ui
        .as_ref()
        .and_then(|ui| if ui.enabled { ui.path.clone() } else { None })
        .map(std::path::PathBuf::from)
        // Fallback: look for a 'ui' directory next to the binary (a production
        // deployment). This supports the cargo build, which copies the UI to
        // target/ui.
        .or_else(|| {
            std::env::current_exe()
                .map_err(anyhow::Error::from)
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .map(|p| p.join("ui"))
        })
        .filter(|p| p.exists());

    let state = ApiState {
        detections,
        data: Arc::new(NoData),
        config: config_container,
        sys: sys.clone(),
        features: HeaderValue::from_str(&features.join(","))?,
        sources,
        sinks,
        store,
    };

    let mut app = create_router()
        .layer(CorsLayer::permissive())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            feature_flag_middleware,
        ))
        // Records the latency of every request.
        .layer(middleware::from_fn(crate::observability::track_latency))
        .with_state(state);

    if let Some(path) = ui {
        app = app
            .nest_service(
                "/ui",
                ServeDir::new(path).append_index_html_on_directories(true),
            )
            .route(
                "/",
                axum::routing::get(|| async { axum::response::Redirect::to("/ui") }),
            );
    }

    let listener = tokio::net::TcpListener::bind(&config.api.host.address()).await?;

    log::info!(
        "API server listening on http://{}",
        config.api.host.address()
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let mut rx = sys.subscribe();
            loop {
                match rx.recv().await {
                    Ok(SysMessage::Shutdown) => break,
                    Ok(_) => continue,
                    Err(_) => {
                        error!("system broadcast channel closed unexpectedly");
                        break;
                    }
                }
            }
            info!("API shutting down...");
        })
        .await?;
    Ok(())
}
