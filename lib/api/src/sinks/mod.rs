//! Vector output sinks. There are two types. A **destination** keeps the
//! `ocsf-*` event stream. A **notification** sends `alerts`. Both types are a
//! [`Sink`]. The service saves them in the `sinks` table. [`SinkCategory`]
//! shows the type of each sink.
#![allow(dead_code)]

use std::collections::BTreeMap;

use serde::Serialize;

use crate::ApiState;
use crate::graph::{Pipeline, RenderCtx, Transform, component};

pub mod destinations;
pub mod notifications;

/// The Vector input that the destination sinks keep: every normalized OCSF
/// event.
//pub const OCSF_INPUT: &str = "ocsf-*";
pub const OCSF_INPUT: &str = "final-ocsf";
/// The Vector input that the notification sinks send: detection findings
/// (class 2004).
pub const ALERTS_INPUT: &str = "alerts";

/// The type of a sink. This sets the UI section and the route namespace.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SinkCategory {
    /// Keeps the OCSF event stream (Local Files, S3, Clickhouse).
    Destination,
    /// Sends alerts (Slack, Email, Webhook).
    Notification,
}

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    #[default]
    Json,
    Text,
}

#[derive(Serialize, Clone, Default)]
pub struct Encoding {
    #[serde(default)]
    pub codec: Codec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only_fields: Option<Vec<String>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct BatchConfig {
    pub max_events: i32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct Framing {
    pub method: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct AuthConfig {
    pub strategy: String,
    pub token: String,
}

/// The way that Vector's parquet encoder processes an event field that is not
/// in the schema. `auto_infer` makes the schema from the batch, so it does not
/// need a `schema_file`. This is the correct default for the mixed `ocsf-*`
/// stream.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParquetSchemaMode {
    Relaxed,
    Strict,
    #[default]
    AutoInfer,
}

/// The batch (columnar) encoder for file destinations. This is Vector's
/// `batch_encoding`.
#[derive(Serialize, Clone)]
#[serde(tag = "codec", rename_all = "snake_case")]
pub enum BatchEncoding {
    /// Apache Parquet columnar encoding.
    Parquet {
        schema_mode: ParquetSchemaMode,
        #[serde(skip_serializing_if = "Option::is_none")]
        schema_file: Option<String>,
    },
}

/// The batch behavior. It applies when you set a columnar `batch_encoding`.
#[derive(Serialize, Clone)]
pub struct FileBatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_events: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct S3Auth {
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Serialize, Clone)]
pub struct BasicAuth {
    pub user: String,
    pub password: String,
    pub strategy: Option<String>
}

/// A Vector sink body. `tag = "type"` writes the name of the Vector sink type
/// (`http`, `aws_s3`, `clickhouse`, `file`, and others).
#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SinkType {
    Http {
        uri: String,
        encoding: Encoding,
        inputs: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<AuthConfig>,
        #[serde(skip_serializing_if = "Option::is_none")]
        batch: Option<BatchConfig>,
        #[serde(skip_serializing_if = "Option::is_none")]
        framing: Option<Framing>,
    },
    Vector {
        address: String,
        port: u16,
        encoding: Encoding,
        inputs: Vec<String>,
    },
    Blackhole {
        inputs: Vec<String>,
    },
    /// The local file sink (the Local Files / parquet destination).
    ///
    /// Vector's schema needs the `encoding` field. But Vector ignores it after
    /// you set `batch_encoding`. Vector then makes batches of events. It writes
    /// them as columnar (Parquet) files.
    File {
        path: String,
        encoding: Encoding,
        inputs: Vec<String>,
        batch_encoding: BatchEncoding,
        #[serde(skip_serializing_if = "Option::is_none")]
        batch: Option<FileBatch>,
    },
    /// The AWS S3 destination.
    AwsS3 {
        bucket: String,
        region: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        key_prefix: String,
        encoding: Encoding,
        inputs: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<S3Auth>,
    },
    /// The Clickhouse destination.
    Clickhouse {
        endpoint: String,
        database: String,
        table: String,
        inputs: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<BasicAuth>,
    },
}

pub trait Sink: Send + Sync {
    /// The stable resource id. This is also the key for storage.
    fn id(&self) -> String;

    /// Tells if this sink is a destination or a notification.
    fn category(&self) -> SinkCategory;

    fn config(&self) -> SinkType;

    /// The type name. The service uses it to save and rebuild this sink.
    fn typename(&self) -> String;

    /// A clear name for the UI list.
    fn name(&self) -> String {
        self.typename()
    }

    /// The settings that the service needs to rebuild this sink from storage.
    fn settings(&self) -> serde_json::Value;

    /// The transforms that this sink puts before the sink component. The
    /// `inputs` of its configuration must point to them.
    fn transforms(&self) -> BTreeMap<String, Transform> {
        BTreeMap::new()
    }

    /// The Vector components that this sink adds to the graph.
    fn pipeline(&self, _ctx: &RenderCtx) -> anyhow::Result<Pipeline> {
        let mut pipeline = Pipeline::default();
        let component_id = format!("sink-{}_{}", self.typename(), self.id());
        pipeline.sinks.insert(component_id, component(self.config())?);
        pipeline.transforms.extend(self.transforms());
        Ok(pipeline)
    }
}

pub type ExistingSink = (String, String, serde_json::Value);

impl TryInto<Box<dyn Sink>> for ExistingSink {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<Box<dyn Sink>, Self::Error> {
        let (sinktype, id, config) = self;
        Ok(match sinktype.as_str() {
            "file" => Box::new(destinations::local_file::LocalFile {
                id,
                settings: serde_json::from_value(config)?,
            }),
            "aws_s3" => Box::new(destinations::aws_s3::AwsS3 {
                id,
                settings: serde_json::from_value(config)?,
            }),
            "clickhouse" => Box::new(destinations::clickhouse::Clickhouse {
                id,
                settings: serde_json::from_value(config)?,
            }),
            "slack" => Box::new(notifications::slack::Slack {
                id,
                settings: serde_json::from_value(config)?,
            }),
            "email" => Box::new(notifications::email::Email {
                id,
                settings: serde_json::from_value(config)?,
            }),
            "webhook" => Box::new(notifications::webhook::Webhook {
                id,
                settings: serde_json::from_value(config)?,
            }),
            _ => return Err(anyhow::anyhow!("Unsupported sink type: {}", sinktype)),
        })
    }
}

/// The shared list handler. It gives the saved sinks in `category` as
/// `{ id, sinktype, name, config }`.
pub(crate) async fn list_by_category(
    state: &ApiState,
    category: SinkCategory,
) -> axum::Json<Vec<serde_json::Value>> {
    let sinks = state.sinks.read().await;
    axum::Json(
        sinks
            .iter()
            .filter(|s| s.category() == category)
            .map(|s| {
                serde_json::json!({
                    "id": s.id(),
                    "sinktype": s.typename(),
                    "name": s.name(),
                    "config": s.settings(),
                })
            })
            .collect(),
    )
}

/// The shared delete handler for a sink id in a category.
pub(crate) async fn delete(
    state: &ApiState,
    category: SinkCategory,
    id: &str,
) -> Result<axum::Json<()>, (axum::http::StatusCode, String)> {
    let mut sinks = state.sinks.write().await;
    let index = sinks
        .iter()
        .position(|s| s.id() == id && s.category() == category)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Sink with id {} not found", id),
            )
        })?;

    state
        .store
        .remove_sink(id)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    sinks.remove(index);
    Ok(axum::Json(()))
}

/// The shared add handler. It saves the sink, adds it to the live state, and
/// gives its id.
pub(crate) async fn add(
    state: &ApiState,
    sink: Box<dyn Sink>,
) -> Result<String, (axum::http::StatusCode, String)> {
    let id = sink.id();
    state
        .store
        .add_sink(sink.as_ref())
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    state.sinks.write().await.push(sink);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{RenderCtx, VectorConfig};
    use serde_json::json;

    fn sink(kind: &str, id: &str, config: serde_json::Value) -> Box<dyn Sink> {
        (kind.to_string(), id.to_string(), config).try_into().unwrap()
    }

    #[test]
    fn destinations_archive_the_ocsf_stream_and_round_trip() {
        let cases = [
            sink("file", "d1", json!({ "path": "/data/storage" })),
            sink("aws_s3", "d2", json!({ "bucket": "b", "region": "us-east-1" })),
            sink("clickhouse", "d3", json!({ "endpoint": "http://ch:8123" })),
        ];

        let reads = |c: &toml::Value, input: &str| {
            c.get("inputs")
                .and_then(|v| v.as_array())
                .is_some_and(|a| a.iter().any(|i| i.as_str() == Some(input)))
        };

        let mut cfg = VectorConfig::default();
        for s in &cases {
            assert!(matches!(s.category(), SinkCategory::Destination));
            let pipeline = s.pipeline(&RenderCtx::default()).unwrap();

            // Every destination keeps the OCSF stream. It reads the stream
            // directly or through a transform that comes before it. (Local
            // Files makes many sinks; Clickhouse remaps.)
            assert!(!pipeline.sinks.is_empty());
            assert!(
                pipeline
                    .transforms
                    .values()
                    .any(|t| t.inputs.iter().any(|i| i == OCSF_INPUT))
                    || pipeline.sinks.values().any(|c| reads(c, OCSF_INPUT)),
                "{} reads the OCSF stream",
                s.typename()
            );

            cfg.merge(pipeline);
        }

        // The full document goes to TOML and back with no change.
        let rendered = toml::to_string(&cfg).unwrap();
        toml::from_str::<toml::Value>(&rendered).unwrap();
    }

    #[test]
    fn clickhouse_prepends_event_id_remap() {
        // The minimal settings: no database (the default is `default`), no
        // table.
        let s = sink("clickhouse", "d3", json!({ "endpoint": "http://ch:8123" }));
        let p = s.pipeline(&RenderCtx::default()).unwrap();

        // A remap copies the OCSF uid to _event_id. It reads the OCSF stream.
        let remap = p.transforms.get("sink-clickhouse_d3-remap").expect("remap");
        assert_eq!(remap.source.as_deref(), Some("._event_id = .metadata.uid"));
        assert_eq!(remap.inputs, vec![OCSF_INPUT]);

        // The sink reads the remap. It writes to the fixed default.striem_ocsf
        // table.
        let ch = p.sinks.get("sink-clickhouse_d3").expect("clickhouse sink");
        let inputs = ch.get("inputs").unwrap().as_array().unwrap();
        assert_eq!(inputs[0].as_str(), Some("sink-clickhouse_d3-remap"));
        assert_eq!(ch.get("database").and_then(|v| v.as_str()), Some("default"));
        assert_eq!(ch.get("table").and_then(|v| v.as_str()), Some("striem_ocsf"));
    }

    #[test]
    fn local_file_fans_out_one_parquet_sink_per_ocsf_class() {
        // The fan-out comes from JSON only. It does not read the filesystem.
        let s = sink("file", "d1", json!({ "path": "/data/storage" }));
        let p = s.pipeline(&RenderCtx::default()).unwrap();

        // The remap (from remap.vrl) sends events to the exclusive_route.
        let remap = p.transforms.get("sink-file_d1-remap").expect("remap");
        assert_eq!(
            remap.file.as_deref(),
            Some("${STRIEM_SCHEMA_DIR}/remap.vrl")
        );
        assert_eq!(remap.inputs, vec![OCSF_INPUT]);

        let route = p.transforms.get("sink-file_d1-route").expect("route");
        assert_eq!(route.inputs, vec!["sink-file_d1-remap"]);
        let routes = route.routes.as_ref().expect("routes");
        assert!(!routes.is_empty());

        // There is one parquet sink for each class. Each sink connects to its
        // route port and uses its own schema file under the schema directory.
        let api = p
            .sinks
            .get("sink-file_d1-api_activity")
            .expect("api_activity sink");
        let inputs = api.get("inputs").unwrap().as_array().unwrap();
        assert_eq!(
            inputs[0].as_str(),
            Some("sink-file_d1-route.api_activity")
        );
        let schema_file = api
            .get("batch_encoding")
            .and_then(|b| b.get("schema_file"))
            .and_then(|v| v.as_str())
            .unwrap();
        assert_eq!(
            schema_file,
            "${STRIEM_SCHEMA_DIR}/application/api_activity.parquet.schema"
        );

        // There is one sink for each route.
        assert_eq!(p.sinks.len(), routes.len());

        // The full document goes to TOML and back with no change.
        let mut cfg = VectorConfig::default();
        cfg.merge(p);
        let rendered = toml::to_string(&cfg).unwrap();
        toml::from_str::<toml::Value>(&rendered).unwrap();
    }

    #[test]
    fn notifications_deliver_alerts() {
        for s in [
            sink("slack", "n1", json!({ "token": "t", "channel": "#c" })),
            sink("webhook", "n2", json!({ "url": "https://hook.example/x" })),
            sink(
                "email",
                "n3",
                json!({ "endpoint": "https://mail.example/send", "to": "a@b.c", "from": "s@t.u" }),
            ),
        ] {
            assert!(matches!(s.category(), SinkCategory::Notification));
            let pipeline = s.pipeline(&RenderCtx::default()).unwrap();
            // The sink or a transform that comes before it reads the alerts
            // stream.
            let reads_alerts = pipeline
                .transforms
                .values()
                .any(|t| t.inputs.iter().any(|i| i == ALERTS_INPUT))
                || pipeline.sinks.values().any(|c| {
                    c.get("inputs")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().any(|i| i.as_str() == Some(ALERTS_INPUT)))
                        .unwrap_or(false)
                });
            assert!(reads_alerts, "{} should read the alerts stream", s.typename());
        }
    }
}
