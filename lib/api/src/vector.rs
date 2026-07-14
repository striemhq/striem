use crate::ApiState;
use crate::graph::{Component, RenderCtx, Transform, VectorConfig};
use axum::{Router, extract::State, routing::get};
use striem_config::StrIEMConfig;
use toml::toml;

/// The `${STRIEM_REMAPS}` env var isn't interpolated by Vector's HTTP config
/// provider, so we resolve it here and fall back to the literal placeholder.
fn remaps_dir() -> String {
    std::env::var("STRIEM_REMAPS").unwrap_or_else(|_| "${STRIEM_REMAPS}".to_string())
}

/// The static scaffolding every generated config starts from: a stdin seed so
/// the `ocsf-*` wildcard always has a producer, the `alerts` filter, and the
/// `sink-striem` forwarder.
fn boilerplate(config: &StrIEMConfig) -> VectorConfig {
    let fqdn = config.fqdn.clone().unwrap_or_else(|| config.input.url());

    let mut cfg = VectorConfig::default();

    cfg.sources.insert(
        "ocsf-stdin".to_string(),
        Component::Table(toml! {
            type = "stdin"
            decoding = { codec = "json" }
            framing = { method = "newline_delimited" }
        }),
    );

    cfg.transforms.insert(
        "alerts".to_string(),
        Transform::filter(".class_uid == 2004").with_inputs(["ocsf-*"]),
    );

    cfg.sinks.insert(
        "sink-striem".to_string(),
        Component::Table(toml! {
            type = "vector"
            inputs = ["ocsf-*"]
            address = fqdn
        }),
    );

    cfg
}

/// Apply the pieces derived from the configured Vector destination: the API
/// endpoint, the primary `vector` source, and optional HEC ingest. Returns the
/// HTTP ingest address (if any) for use by HTTP sources.
fn apply_destination(cfg: &mut VectorConfig, config: &StrIEMConfig) -> Option<String> {
    let Some(vector) = &config.output else {
        return None;
    };

    if let Some(api) = &vector.api {
        let address = api.address().to_string();
        cfg.api = Some(Component::Table(toml! {
            enabled = true
            address = address
        }));
    }

    let address = vector.cfg.address().to_string();
    cfg.sources.insert(
        "source-striem".to_string(),
        Component::Table(toml! {
            type = "vector"
            address = address
            version = "2"
        }),
    );

    if let Some(hec) = &vector.hec {
        let address = hec.address().to_string();
        cfg.sources.insert(
            "source-hec".to_string(),
            Component::Table(toml! {
                type = "splunk_hec"
                address = address
                store_hec_token = true
            }),
        );
    }

    vector.http.as_ref().map(|http| http.address().to_string())
}

async fn get_vector_config(
    State(state): State<ApiState>,
) -> Result<String, (axum::http::StatusCode, String)> {
    let config = state.config.load();

    let mut cfg = boilerplate(&config);
    let http_address = apply_destination(&mut cfg, &config);

    let ctx = RenderCtx {
        remaps_dir: remaps_dir(),
        http_address,
    };

    let internal = |e: anyhow::Error| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            e.to_string(),
        )
    };

    for source in state.sources.read().await.iter() {
        cfg.merge(source.pipeline(&ctx).map_err(internal)?);
    }
    for sink in state.sinks.read().await.iter() {
        cfg.merge(sink.pipeline().map_err(internal)?);
    }

    toml::to_string(&cfg).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            e.to_string(),
        )
    })
}

pub fn create_router() -> axum::Router<ApiState> {
    Router::new().route("/", get(get_vector_config))
}
