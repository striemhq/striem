//! HTTP API server for StrIEM management interface.
//!
//! Provides REST endpoints for:
//! - Source management (add/remove data sources)
//! - Detection rule management (list/enable/disable/upload)
//! - Data querying (DuckDB SQL queries on Parquet files)
//! - Vector configuration generation
//!
//! # Architecture
//! - Axum for HTTP routing and middleware
//! - Tower HTTP for CORS and static file serving
//! - DuckDB connection pool for query execution
//! - Shared state (Arc) for detection rules and configuration

use std::sync::Arc;

use anyhow::Result;
use arc_swap::ArcSwap;
use axum::http::HeaderValue;
use axum::middleware;
use log::{error, info};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use striem_detection::detections_client::DetectionsClient;

use striem_config::StrIEMConfig;
use striem_config::StringOrList;

use striem_common::SysMessage;

use crate::{
    ApiState, actions::Mcp, features::feature_flag_middleware, initdb, routes::create_router, store,
};

/// Initialize and run the API server.
///
/// # Database Initialization
/// Creates DuckDB connection pool if storage is configured.
/// Uses file-backed DB if data_dir specified, otherwise in-memory.
/// Enables parquet_metadata_cache for faster queries on large datasets.
///
/// # UI Serving
/// Serves Next.js static export from binary path or configured ui.path.
/// Redirects / to /ui for convenience.
pub async fn serve(
    config: &Arc<ArcSwap<StrIEMConfig>>,
    sys: tokio::sync::broadcast::Sender<SysMessage>,
) -> Result<()> {
    let config_container = config.clone();
    let config = config.load();

    let mut features: Vec<String> = Vec::new();

    // Detection rule management is delegated to the detection microservice over
    // gRPC. Connect lazily so the API can start before the detection service is
    // reachable; per-request calls establish the connection on first use.
    let endpoint = config.detection_endpoint();
    let detections = DetectionsClient::new(
        tonic::transport::Endpoint::from_shared(endpoint.clone())
            .map_err(|e| anyhow::anyhow!("invalid detection endpoint {}: {}", endpoint, e))?
            .connect_lazy(),
    );
    info!("detection admin client targeting {}", endpoint);

    // Create DB connection pool
    let db = initdb(&config).inspect(|_| {
        #[cfg(feature = "duckdb")]
        features.push("duckdb".to_string());
    });

    let store = store::open(&db);
    let sources = Arc::new(RwLock::new(store.load_sources().unwrap_or_default()));
    let sinks = Arc::new(RwLock::new(store.load_sinks().unwrap_or_default()));

    let actions = if let Some(mcp_config) = &config.api.mcp {
        match &mcp_config.url {
            StringOrList::String(url) => Some(Arc::new(Mcp::new(url.clone()))),
            StringOrList::List(urls) if !urls.is_empty() => {
                Some(Arc::new(Mcp::new(urls[0].clone())))
            }
            _ => None,
        }
    } else {
        None
    }
    .inspect(|_| {
        features.push("mcp".to_string());
    });

    let ui = config
        .api
        .ui
        .as_ref()
        .and_then(|ui| if ui.enabled { ui.path.clone() } else { None })
        .map(std::path::PathBuf::from)
        // Fallback: look for 'ui' directory next to binary (production deployment)
        // This supports cargo build integration where UI is copied to target/ui
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
        actions,
        db,
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
