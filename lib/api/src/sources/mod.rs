mod aws_cloudtrail;
pub mod http;
mod okta;
use std::collections::BTreeMap;
use std::fmt::Display;

use axum::{Router, extract::State};
use erased_serde as es;
use serde::{Deserialize, Serialize};

use serde_json::{Value, json};

use crate::ApiState;
use crate::graph::{Pipeline, RenderCtx, Transform, component, naming};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    AwsCloudtrail,
    Http,
    Okta,
}

impl Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceType::AwsCloudtrail => write!(f, "aws_cloudtrail"),
            SourceType::Http => write!(f, "http_server"),
            SourceType::Okta => write!(f, "okta"),
        }
    }
}

#[derive(Serialize, Clone, Default)]
#[serde(tag = "codec", rename_all = "snake_case")]
pub enum Decoding {
    #[default]
    Json,
}

/// The Sigma logsource taxonomy of a source. The service puts it into the event
/// metadata. Thus a detection rule can apply to only the correct product or
/// service.
#[derive(Default)]
pub struct Logsource {
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub service: Option<String>,
}

impl Logsource {
    fn to_map(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        if let Some(vendor) = &self.vendor {
            map.insert("vendor".to_string(), vendor.clone());
        }
        if let Some(product) = &self.product {
            map.insert("product".to_string(), product.clone());
        }
        if let Some(service) = &self.service {
            map.insert("service".to_string(), service.clone());
        }
        map
    }
}

/// A data source. It sets its own Sigma taxonomy, its Vector `source`
/// configuration, and the `transform`s that normalize events into OCSF.
///
/// Each source adds a chain of components. The last node of the chain is
/// `ocsf-<type>_<id>` (see [`Source::output`]). [`Source::pipeline`] makes these
/// components. The default implementation builds the standard
/// `source -> [pre ->] logsource -> ocsf` chain.
pub trait Source: Send + Sync {
    fn id(&self) -> String;

    /// The Vector source type.
    fn sourcetype(&self) -> SourceType;

    /// A clear name.
    fn name(&self) -> String {
        self.sourcetype().to_string()
    }

    /// The Sigma logsource taxonomy of this source.
    fn logsource(&self) -> Logsource {
        Logsource::default()
    }

    /// The Vector source configuration.
    fn config(&self) -> &dyn es::Serialize;

    /// An optional preprocessing transform. It comes between the raw source and
    /// the logsource-tag step. The standard pipeline connects its input to the
    /// source.
    fn preprocess(&self) -> Option<Transform> {
        None
    }

    /// The node where downstream consumers read this source's normalized OCSF
    /// events: `ocsf-<type>_<id>`.
    fn output(&self) -> String {
        naming::ocsf(self.sourcetype(), &self.id())
    }

    /// The Vector components that this source adds to the graph.
    fn pipeline(&self, ctx: &RenderCtx) -> anyhow::Result<Pipeline> {
        standard_pipeline(self, ctx)
    }
}

/// The VRL that tags an event with its source id and its Sigma logsource.
fn logsource_meta(source_id: &str, logsource: &Logsource) -> String {
    let sigma = json!({ "logsource": logsource.to_map() });
    format!("%source_id = \"{}\"\n%sigma = {}\n", source_id, sigma)
}

/// Builds the standard `source -> [pre ->] logsource -> ocsf` chain. Every
/// source that inputs through its own Vector source component uses this chain.
fn standard_pipeline<S: Source + ?Sized>(
    src: &S,
    ctx: &RenderCtx,
) -> anyhow::Result<Pipeline> {
    let sourcetype = src.sourcetype();
    let id = src.id();

    let source_id = naming::source(&sourcetype, &id);
    let logsource_id = naming::logsource(&sourcetype, &id);
    let ocsf_id = naming::ocsf(&sourcetype, &id);

    let mut pipeline = Pipeline::default();
    pipeline
        .sources
        .insert(source_id.clone(), component(src.config())?);

    // The optional preprocessing comes between the source and the logsource tag.
    let tagged_input = match src.preprocess() {
        Some(pre) => {
            let pre_id = naming::pre(&sourcetype, &id);
            pipeline
                .transforms
                .insert(pre_id.clone(), pre.with_inputs([source_id.clone()]));
            pre_id
        }
        None => source_id.clone(),
    };

    pipeline.transforms.insert(
        logsource_id.clone(),
        Transform::remap(logsource_meta(&source_id, &src.logsource()))
            .with_inputs([tagged_input]),
    );
    pipeline.transforms.insert(
        ocsf_id,
        Transform::remap_file(ctx.remap_file(&sourcetype)).with_inputs([logsource_id]),
    );

    Ok(pipeline)
}

pub type ExistingSource = (String, String, serde_json::Value);

impl TryInto<Box<dyn Source>> for ExistingSource {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<Box<dyn Source>, Self::Error> {
        let (sourcetype, id, config) = self;
        match sourcetype.as_str() {
            "aws_cloudtrail" => Ok(Box::new(aws_cloudtrail::AwsCloudtrail {
                id,
                config: serde_json::from_value(config).map_err(|e| anyhow::anyhow!(e))?,
            })),
            "okta" => Ok(Box::new(okta::Okta {
                id,
                config: serde_json::from_value(config).map_err(|e| anyhow::anyhow!(e))?,
            })),
            "http" | "http_server" => Ok(Box::new(http::HttpRoute {
                id,
                config: serde_json::from_value(config).map_err(|e| anyhow::anyhow!(e))?,
            })),
            _ => Err(anyhow::anyhow!("Unsupported source type: {}", sourcetype))?,
        }
    }
}

async fn list_sources(State(state): State<ApiState>) -> axum::Json<Vec<serde_json::Value>> {
    let sources = state.sources.read().await;

    axum::Json(
        sources
            .iter()
            .map(|source| {
                serde_json::json!({
                    "id": source.id(),
                    "sourcetype": source.sourcetype(),
                    "name": source.name(),
                })
            })
            .collect(),
    )
}

async fn get_source(
    State(state): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let sources = state.sources.read().await;

    let source = sources
        .iter()
        .find(|source| source.id() == id)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Source with id {} not found", id),
            )
        })?;

    let config = serde_json::to_value(source.config())
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(json!({
        "id": source.id(),
        "sourcetype": source.sourcetype(),
        "name": source.name(),
        "config": config,
    })))
}

async fn delete_source(
    State(state): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<axum::Json<()>, (axum::http::StatusCode, String)> {
    let mut sources = state.sources.write().await;

    let index = sources
        .iter()
        .position(|source| source.id() == id)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Source with id {} not found", id),
            )
        })?;

    state
        .store
        .remove_source(&id)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sources.remove(index);

    Ok(axum::Json(()))
}

async fn add_source(
    State(state): State<ApiState>,
    axum::extract::Path(sourcetype): axum::extract::Path<SourceType>,
    axum::extract::Json(config): axum::extract::Json<Value>,
) -> Result<axum::Json<Value>, (axum::http::StatusCode, String)> {
    let id = uuid::Uuid::now_v7().to_string();

    let source: Box<dyn Source> = match sourcetype {
        SourceType::AwsCloudtrail => {
            let cfg = serde_json::from_value(config)
                .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
            Box::new(aws_cloudtrail::AwsCloudtrail { id, config: cfg })
        }
        SourceType::Okta => {
            let cfg = serde_json::from_value(config)
                .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
            Box::new(okta::Okta { id, config: cfg })
        }
        SourceType::Http => {
            let cfg = serde_json::from_value(config)
                .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?;
            Box::new(http::HttpRoute { id, config: cfg })
        }
    };

    let sourcetype = source.sourcetype();
    let source_id = source.id();

    state
        .store
        .add_source(source.as_ref())
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut sources = state.sources.write().await;

    sources.push(source);

    let sourcetype_value =
        serde_json::to_value(&sourcetype).unwrap_or_else(|_| serde_json::Value::Null);

    let ingest_path = if matches!(sourcetype, SourceType::Http) {
        Some(format!("/{}", source_id))
    } else {
        None
    };

    Ok(axum::Json(json!({
        "id": source_id,
        "sourcetype": sourcetype_value,
        "ingest_path": ingest_path,
    })))
}

pub fn create_router() -> axum::Router<ApiState> {
    Router::new()
        .route("/", axum::routing::get(list_sources))
        .route(
            "/{id}",
            axum::routing::get(get_source)
                .delete(delete_source)
                .post(add_source),
        )
}

#[test]
fn http_pipeline_terminates_at_ocsf_node() {
    use crate::sources::http::{HttpConfig, HttpRoute};

    let source = Box::new(HttpRoute {
        id: "test_http".to_string(),
        config: HttpConfig {
            name: Some("Test HTTP Source".to_string()),
            logsource: BTreeMap::from([
                ("vendor".to_string(), "test_vendor".to_string()),
                ("product".to_string(), "test_product".to_string()),
            ]),
            vrl: "some vrl".to_string(),
        },
    }) as Box<dyn Source>;

    let ctx = RenderCtx {
        remaps_dir: "/remaps".to_string(),
        http_address: Some("0.0.0.0:8080".to_string()),
        ..Default::default()
    };
    let pipeline = source.pipeline(&ctx).unwrap();

    // The chain ends at the ocsf-<type>_<id> node...
    assert_eq!(source.output(), "ocsf-http_server_test_http");
    assert!(pipeline.transforms.contains_key(&source.output()));

    // ...and all the HTTP sources share one listener, which removes duplicates.
    assert!(pipeline.sources.contains_key(http::HTTP_LISTENER));
}

#[test]
fn merged_graph_dedupes_http_listener_and_serializes() {
    use crate::graph::VectorConfig;
    use crate::sources::http::{HttpConfig, HttpRoute};

    let ctx = RenderCtx {
        remaps_dir: "/remaps".to_string(),
        http_address: Some("0.0.0.0:8080".to_string()),
        ..Default::default()
    };

    let aws: Box<dyn Source> = (
        "aws_cloudtrail".to_string(),
        "aws1".to_string(),
        json!({ "sqs": { "queue_url": "https://sqs.example/q" } }),
    )
        .try_into()
        .unwrap();
    let okta: Box<dyn Source> = (
        "okta".to_string(),
        "okta1".to_string(),
        json!({ "domain": "acme.okta.com", "token": "secret" }),
    )
        .try_into()
        .unwrap();
    let http = |id: &str| {
        Box::new(HttpRoute {
            id: id.to_string(),
            config: HttpConfig {
                name: None,
                logsource: BTreeMap::new(),
                vrl: ". = .".to_string(),
            },
        }) as Box<dyn Source>
    };

    let mut cfg = VectorConfig::default();
    for source in [aws, okta, http("gh"), http("ci")] {
        cfg.merge(source.pipeline(&ctx).unwrap());
    }

    // There are two HTTP sources, but only one shared listener.
    assert!(cfg.sources.contains_key(http::HTTP_LISTENER));
    assert!(cfg.sources.contains_key("source-aws_cloudtrail_aws1"));
    assert!(cfg.transforms.contains_key("ocsf-http_server_gh"));
    assert!(cfg.transforms.contains_key("ocsf-http_server_ci"));
    assert!(cfg.transforms.contains_key("ocsf-okta_okta1"));

    // The full document goes to TOML and back with no change.
    let rendered = toml::to_string(&cfg).unwrap();
    toml::from_str::<toml::Value>(&rendered).unwrap();
    println!("{}", rendered);
}
