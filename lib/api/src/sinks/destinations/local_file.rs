//! Local Files (parquet) destination.
//!
//! Each OCSF class has its own parquet schema. Thus one sink cannot encode the
//! mixed `final-ocsf` stream. In place of one sink, this destination makes these
//! components:
//!
//! 1. a `remap` that flattens events to fit the parquet schemas (`remap.vrl`),
//! 2. an `exclusive_route` that splits the stream into one port for each class,
//!    and
//! 3. one `file` sink for each class. Each sink encodes with that class's
//!    schema.
//!
//! The names of the routes and sinks are the OCSF class short names (for
//! example `api_activity`). The service reads these names from the embedded
//! `ocsf_class_category.json`. The service writes the schema and remap paths as
//! `${STRIEM_SCHEMA_DIR}/…`. Vector replaces this variable when it loads the
//! configuration. Thus the API does not open the schema files.

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

/// The embedded OCSF class/category map. It is the source of truth for the
/// class list. Thus the fan-out does not read the schema files.
const OCSF_CLASS_CATEGORY: &str = include_str!("../../../ocsf_class_category.json");

/// The root of the schema directory. Vector replaces this variable from its
/// environment. It points to the versioned schema set (the directory with the
/// category subdirectories and `remap.vrl`).
const SCHEMA_DIR: &str = "${STRIEM_SCHEMA_DIR}";

#[derive(Deserialize)]
struct OcsfMap {
    /// `category_uid` -> category directory name (e.g. `"6"` -> `application`).
    categories: BTreeMap<String, String>,
    /// `class_uid` -> class short name (e.g. `"6003"` -> `api_activity`).
    classes: BTreeMap<String, String>,
}

/// An OCSF class and the parquet schema file for it.
struct OcsfClass {
    /// The short name (e.g. `api_activity`).
    name: String,
    /// The numeric `class_uid`. The route condition uses it.
    uid: u32,
    /// The path to the `<class>.parquet.schema` file. The API writes this path
    /// for Vector to read. The API does not open the file.
    schema_file: String,
}

/// The full fan-out layout. It comes from the embedded JSON only. It does not
/// read the filesystem. The paths are `${STRIEM_SCHEMA_DIR}/…`. Vector replaces
/// this variable.
struct OcsfLayout {
    /// The path to `remap.vrl` under the schema root.
    remap_file: String,
    classes: Vec<OcsfClass>,
}

/// Makes the layout from the embedded OCSF map. The `schema_file` and `remap`
/// paths are `${STRIEM_SCHEMA_DIR}/<category>/<class>.parquet.schema`. The
/// function gets a class's category directory from its uid (`class_uid / 1000`).
/// It gives `None` only if the embedded JSON does not parse.
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
    /// The Hive-partitioned parquet path for one class. An example is
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

    /// The parquet `file` sink for one class. It encodes with the class schema.
    fn class_sink(&self, route_id: &str, class: &OcsfClass) -> SinkType {
        SinkType::File {
            path: self.class_path(&class.name),
            // Vector's schema needs this field. Vector ignores it after you set
            // batch_encoding.
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
        // This is the same as the Vector sink type. It is also the storage type
        // name and the prefix of the `sink-file_<id>` component id.
        "file".to_string()
    }

    fn name(&self) -> String {
        format!("Files ({})", self.settings.path)
    }

    /// The fallback form with one sink. The service uses it only when it finds
    /// no schemas. This is one parquet sink for the full stream. It makes its
    /// schema automatically.
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

        // The class list comes from the embedded JSON. Vector replaces
        // `${STRIEM_SCHEMA_DIR}` in the paths. Thus the fan-out does not read the
        // API's filesystem.
        let layout = ocsf_layout();
        let classes = layout.as_ref().map(|l| l.classes.as_slice()).unwrap_or_default();

        // The function uses the fallback only if the embedded map does not
        // parse. This does not happen in normal operation.
        let (Some(layout), false) = (&layout, classes.is_empty()) else {
            let mut pipeline = Pipeline::default();
            pipeline.sinks.insert(base, component(self.config())?);
            return Ok(pipeline);
        };

        let remap_id = format!("{}-remap", base);
        let route_id = format!("{}-route", base);
        let mut pipeline = Pipeline::default();

        // 1. Flatten the events so they fit the parquet schemas.
        pipeline.transforms.insert(
            remap_id.clone(),
            Transform::remap_file(layout.remap_file.clone()).with_inputs([OCSF_INPUT]),
        );

        // 2. Route the events by class_uid.
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

        // 3. Make one parquet sink for each class. Each sink reads that class's
        // route port.
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

/// Gives the parquet directory of `sink` if `sink` is a Local Files
/// destination.
pub fn storage_path(sink: &dyn Sink) -> Option<String> {
    (sink.typename() == "file")
        .then(|| sink.settings().get("path")?.as_str().map(str::to_string))
        .flatten()
}
