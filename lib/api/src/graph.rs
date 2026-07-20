//! Typed model of a Vector component graph.
//!
//! StrIEM builds a Vector configuration by merging small [`Pipeline`]
//! fragments — one per source and sink — onto a boilerplate [`VectorConfig`].
//! Every component is keyed by its Vector component id, so merging is just a
//! map union: shared components (e.g. the single HTTP listener that fronts
//! many HTTP sources) de-duplicate naturally instead of needing special-cased
//! merge logic.

use std::collections::BTreeMap;
use std::fmt::Display;

use anyhow::Result;
use serde::Serialize;

/// A Vector component body (a source or a sink) — an inline table of
/// arbitrary, component-specific keys such as `type`, `address`, `inputs`.
pub type Component = toml::Value;

/// Serialize any component config into a [`Component`] table.
pub fn component(body: impl Serialize) -> Result<Component> {
    Ok(toml::Value::try_from(body)?)
}

/// Deployment-wide context needed to render pipelines but not owned by an
/// individual source or sink.
#[derive(Clone, Default)]
pub struct RenderCtx {
    /// Directory holding the per-source OCSF remap VRL files.
    pub remaps_dir: String,
    /// Bind address for the shared HTTP ingest listener, if configured.
    pub http_address: Option<String>,
}

impl RenderCtx {
    /// Path to the OCSF remap VRL file for a given source type.
    pub fn remap_file(&self, sourcetype: impl Display) -> String {
        format!("{}/{}/remap.vrl", self.remaps_dir, sourcetype)
    }
}

/// A named route of an `exclusive_route` transform. Events are matched against
/// routes in order (first match wins); each route is exposed as the output port
/// `<transform>.<name>`.
#[derive(Serialize, Clone)]
pub struct Route {
    pub name: String,
    /// VRL boolean expression selecting events for this route.
    pub condition: String,
}

/// A Vector `transform` component.
#[derive(Serialize, Default, Clone)]
pub struct Transform {
    #[serde(flatten)]
    pub transform_type: TransformType,
    pub inputs: Vec<String>,
    /// Inline VRL program (for `remap`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Path to a VRL program file (for `remap`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// VRL condition (for `filter`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Named routes (for `exclusive_route`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routes: Option<Vec<Route>>,
}

#[derive(Serialize, Clone, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformType {
    #[default]
    Remap,
    Filter,
    ExclusiveRoute,
}

impl Transform {
    /// A `remap` transform running an inline VRL program.
    pub fn remap(source: impl Into<String>) -> Self {
        Transform {
            transform_type: TransformType::Remap,
            source: Some(source.into()),
            ..Default::default()
        }
    }

    /// A `remap` transform running a VRL program from a file.
    pub fn remap_file(file: impl Into<String>) -> Self {
        Transform {
            transform_type: TransformType::Remap,
            file: Some(file.into()),
            ..Default::default()
        }
    }

    /// A `filter` transform keeping only events matching `condition`.
    pub fn filter(condition: impl Into<String>) -> Self {
        Transform {
            transform_type: TransformType::Filter,
            condition: Some(condition.into()),
            ..Default::default()
        }
    }

    /// An `exclusive_route` transform splitting the stream into one output port
    /// per route (first match wins).
    pub fn exclusive_route(routes: Vec<Route>) -> Self {
        Transform {
            transform_type: TransformType::ExclusiveRoute,
            routes: Some(routes),
            ..Default::default()
        }
    }

    /// Set this transform's inputs (the ids of upstream components).
    pub fn with_inputs<I, S>(mut self, inputs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.inputs = inputs.into_iter().map(Into::into).collect();
        self
    }
}

/// A fragment of the Vector component graph contributed by a single source or
/// sink.
#[derive(Default)]
pub struct Pipeline {
    pub sources: BTreeMap<String, Component>,
    pub transforms: BTreeMap<String, Transform>,
    pub sinks: BTreeMap<String, Component>,
}

/// Top-level Vector schema options.
#[derive(Serialize)]
pub struct Schema {
    pub log_namespace: bool,
}

impl Default for Schema {
    fn default() -> Self {
        // StrIEM relies on the log namespace to carry source metadata.
        Schema {
            log_namespace: true,
        }
    }
}

/// A complete Vector configuration document, assembled from boilerplate plus
/// the merged pipelines of every configured source and sink.
#[derive(Serialize, Default)]
pub struct VectorConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Component>,
    pub schema: Schema,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, Component>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub transforms: BTreeMap<String, Transform>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub sinks: BTreeMap<String, Component>,
}

impl VectorConfig {
    /// Merge a source/sink [`Pipeline`] into this document. Components with a
    /// colliding id (e.g. a shared HTTP listener) are de-duplicated.
    pub fn merge(&mut self, pipeline: Pipeline) {
        self.sources.extend(pipeline.sources);
        self.transforms.extend(pipeline.transforms);
        self.sinks.extend(pipeline.sinks);
    }
}

/// Vector component id conventions. These functions are the single source of
/// truth for how StrIEM names the nodes of a source's normalization chain.
pub mod naming {
    use std::fmt::Display;

    /// The raw source component: `source-<type>_<id>`.
    pub fn source(sourcetype: impl Display, id: &str) -> String {
        format!("source-{}_{}", sourcetype, id)
    }

    /// An optional preprocessing transform: `pre-<type>_<id>`.
    pub fn pre(sourcetype: impl Display, id: &str) -> String {
        format!("pre-{}_{}", sourcetype, id)
    }

    /// The logsource-tagging transform: `logsource-<type>_<id>`.
    pub fn logsource(sourcetype: impl Display, id: &str) -> String {
        format!("logsource-{}_{}", sourcetype, id)
    }

    /// The terminal, OCSF-normalized node: `ocsf-<type>_<id>`.
    pub fn ocsf(sourcetype: impl Display, id: &str) -> String {
        format!("ocsf-{}_{}", sourcetype, id)
    }
}
