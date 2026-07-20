//! Local Files (parquet) destination.
//!
//! Each OCSF class has its own parquet schema, so a single sink can't encode the
//! heterogeneous `final-ocsf` stream. Instead this destination expands into:
//!
//! 1. a `remap` that flattens events to fit the parquet schemas (`remap.vrl`),
//! 2. an `exclusive_route` that splits the stream into one port per class, and
//! 3. one `file` sink per class, each encoding against that class's schema.
//!
//! Routes/sinks are named by the OCSF class short name (e.g. `api_activity`),
//! read from the embedded `ocsf_class_category.json`. Schema/remap paths are
//! emitted as `${STRIEM_SCHEMA_DIR}/…` and interpolated by Vector at load time,
//! so the API never touches the schema files itself.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::graph::{Pipeline, RenderCtx, Route, Transform, component};
use crate::sinks::{
    BatchEncoding, Encoding, FileBatch, ParquetSchemaMode, Sink, SinkCategory, SinkType, OCSF_INPUT,
};

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalFileSettings {
    pub path: String,
}

pub struct LocalFile {
    pub id: String,
    pub settings: LocalFileSettings,
}

/// The embedded OCSF class/category map — the source of truth for the class
/// list, so the fan-out doesn't depend on scanning the schema files.
const OCSF_CLASS_CATEGORY: &str = include_str!("../../../ocsf_class_category.json");

/// Schema-directory root, interpolated by Vector from its environment. Points
/// at the versioned schema set (the directory holding the category subdirs and
/// `remap.vrl`).
const SCHEMA_DIR: &str = "${STRIEM_SCHEMA_DIR}";

#[derive(Deserialize)]
struct OcsfMap {
    /// `category_uid` -> category directory name (e.g. `"6"` -> `application`).
    categories: BTreeMap<String, String>,
    /// `class_uid` -> class short name (e.g. `"6003"` -> `api_activity`).
    classes: BTreeMap<String, String>,
}

/// An OCSF class and the parquet schema file that describes it.
struct OcsfClass {
    /// Short name (e.g. `api_activity`).
    name: String,
    /// Numeric `class_uid`, used for the route condition.
    uid: u32,
    /// Path to the `<class>.parquet.schema` file — emitted for Vector to read;
    /// the API never opens it.
    schema_file: String,
}

/// The whole fan-out layout derived purely from the embedded JSON — no
/// filesystem access. Paths are `${STRIEM_SCHEMA_DIR}/…` for Vector to resolve.
struct OcsfLayout {
    /// Path to `remap.vrl` under the schema root.
    remap_file: String,
    classes: Vec<OcsfClass>,
}

/// Build the layout from the embedded OCSF map. `schema_file`/`remap` paths are
/// `${STRIEM_SCHEMA_DIR}/<category>/<class>.parquet.schema`; a class's category
/// directory is derived from its uid (`class_uid / 1000`). Returns `None` only
/// if the embedded JSON fails to parse.
fn ocsf_layout() -> Option<OcsfLayout> {
    let map: OcsfMap = serde_json::from_str(OCSF_CLASS_CATEGORY).ok()?;

    let mut classes: Vec<OcsfClass> = map
        .classes
        .iter()
        .filter_map(|(uid, name)| {
            let uid = uid.parse::<u32>().ok()?;
            let category = map.categories.get(&(uid / 1000).to_string())?;
            Some(OcsfClass {
                name: name.clone(),
                uid,
                schema_file: format!("{}/{}/{}.parquet.schema", SCHEMA_DIR, category, name),
            })
        })
        .collect();
    classes.sort_by(|a, b| a.name.cmp(&b.name));

    Some(OcsfLayout {
        remap_file: format!("{}/remap.vrl", SCHEMA_DIR),
        classes,
    })
}

impl LocalFile {
    /// Hive-partitioned parquet path for a single class, e.g.
    /// `<base>/class=api_activity/year=%Y/month=%m/day=%d/%H%M%S.parquet`.
    fn class_path(&self, class: &str) -> String {
        Path::new(&self.settings.path)
            .join(format!("class={}", class))
            .join("year=%Y")
            .join("month=%m")
            .join("day=%d")
            .join("%H%M%S")
            .with_extension("parquet")
            .to_string_lossy()
            .into_owned()
    }

    /// The parquet `file` sink for one class, encoding against its schema.
    fn class_sink(&self, route_id: &str, class: &OcsfClass) -> SinkType {
        SinkType::File {
            path: self.class_path(&class.name),
            // Required by Vector's schema; ignored once batch_encoding is set.
            encoding: Encoding::default(),
            inputs: vec![format!("{}.{}", route_id, class.name)],
            batch_encoding: BatchEncoding::Parquet {
                schema_mode: ParquetSchemaMode::Relaxed,
                schema_file: Some(class.schema_file.clone()),
            },
            batch: Some(FileBatch {
                max_events: Some(100_000),
                timeout_secs: Some(300),
            }),
        }
    }
}

impl Sink for LocalFile {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn category(&self) -> SinkCategory {
        SinkCategory::Destination
    }

    fn typename(&self) -> String {
        // Matches the Vector sink type; also the persistence discriminator and
        // the `sink-file_<id>` component-id prefix.
        "file".to_string()
    }

    fn name(&self) -> String {
        format!("Files ({})", self.settings.path)
    }

    /// Fallback single-sink form, used only when no baked schemas are found:
    /// one auto-inferred parquet sink over the whole stream.
    fn config(&self) -> SinkType {
        SinkType::File {
            path: format!("{}/%Y-%m-%d.parquet", self.settings.path.trim_end_matches('/')),
            encoding: Encoding::default(),
            inputs: vec![OCSF_INPUT.to_string()],
            batch_encoding: BatchEncoding::Parquet {
                schema_mode: ParquetSchemaMode::AutoInfer,
                schema_file: None,
            },
            batch: Some(FileBatch {
                max_events: Some(100_000),
                timeout_secs: Some(300),
            }),
        }
    }

    fn pipeline(&self, _ctx: &RenderCtx) -> anyhow::Result<Pipeline> {
        let base = format!("sink-{}_{}", self.typename(), self.id());

        // The class list comes from the embedded JSON and paths resolve via
        // Vector's `${STRIEM_SCHEMA_DIR}`, so the fan-out never touches the
        // API's filesystem.
        let layout = ocsf_layout();
        let classes = layout.as_ref().map(|l| l.classes.as_slice()).unwrap_or_default();

        // Only falls back if the embedded map failed to parse (shouldn't happen).
        let (Some(layout), false) = (&layout, classes.is_empty()) else {
            let mut pipeline = Pipeline::default();
            pipeline.sinks.insert(base, component(self.config())?);
            return Ok(pipeline);
        };

        let remap_id = format!("{}-remap", base);
        let route_id = format!("{}-route", base);
        let mut pipeline = Pipeline::default();

        // 1. Flatten events so they fit the generated parquet schemas.
        pipeline.transforms.insert(
            remap_id.clone(),
            Transform::remap_file(layout.remap_file.clone()).with_inputs([OCSF_INPUT]),
        );

        // route by class_uid
        let routes = classes
            .iter()
            .map(|c| Route {
                name: c.name.clone(),
                condition: format!(
                    ".class_uid == {}",
                    c.uid
                ),
            })
            .collect();
        pipeline.transforms.insert(
            route_id.clone(),
            Transform::exclusive_route(routes).with_inputs([remap_id.as_str()]),
        );

        // 3. One parquet sink per class, reading that class's route port.
        for class in classes {
            pipeline.sinks.insert(
                format!("{}-{}", base, class.name),
                component(self.class_sink(&route_id, class))?,
            );
        }

        Ok(pipeline)
    }

    fn settings(&self) -> serde_json::Value {
        serde_json::json!({ "path": self.settings.path })
    }
}

/// If `sink` is a Local Files destination, the parquet directory it owns.
pub fn storage_path(sink: &dyn Sink) -> Option<String> {
    (sink.typename() == "file")
        .then(|| sink.settings().get("path")?.as_str().map(str::to_string))
        .flatten()
}
