//! Typed model of a Vector component graph.
//!
//! StrIEM makes a Vector configuration. It merges small [`Pipeline`] fragments
//! onto a boilerplate [`VectorConfig`]. There is one fragment for each source
//! and each sink. The Vector component id is the key for each component. Thus a
//! merge is only a union of maps. Some components are shared — one example is
//! the single HTTP listener for many HTTP sources. These shared components
//! become one component. The merge does not need special logic to do this.

use std::collections::BTreeMap;
use std::fmt::Display;

use anyhow::Result;
use serde::Serialize;

/// A Vector component body (a source or a sink). It is an inline table. The
/// table has keys for the component, for example `type`, `address`, `inputs`.
pub type Component = toml::Value;

/// Makes a [`Component`] table from a component configuration.
pub fn component(body: impl Serialize) -> Result<Component> {
    Ok(toml::Value::try_from(body)?)
}

/// The context for the full deployment. A pipeline needs this context to render.
/// One source or one sink does not own it.
#[derive(Clone, Default)]
pub struct RenderCtx {
    /// The directory with the OCSF remap VRL files. There is one file for each
    /// source.
    pub remaps_dir: String,
    /// The bind address for the shared HTTP input listener, if you configure it.
    pub http_address: Option<String>,
}

impl RenderCtx {
    /// Gives the path to the OCSF remap VRL file for a source type.
    pub fn remap_file(&self, sourcetype: impl Display) -> String {
        format!("{}/{}/remap.vrl", self.remaps_dir, sourcetype)
    }
}

/// One named route of an `exclusive_route` transform. The transform compares
/// each event with the routes in sequence. The first route that agrees wins.
/// Each route is the output port `<transform>.<name>`.
#[derive(Serialize, Clone)]
pub struct Route {
    pub name: String,
    /// The VRL boolean expression that selects events for this route.
    pub condition: String,
}

/// A Vector `transform` component.
#[derive(Serialize, Default, Clone)]
pub struct Transform {
    #[serde(flatten)]
    pub transform_type: TransformType,
    pub inputs: Vec<String>,
    /// The inline VRL program (for `remap`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The path to a VRL program file (for `remap`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The VRL condition (for `filter`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// The named routes (for `exclusive_route`).
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
    /// A `remap` transform that runs an inline VRL program.
    pub fn remap(source: impl Into<String>) -> Self {
        Transform {
            transform_type: TransformType::Remap,
            source: Some(source.into()),
            ..Default::default()
        }
    }

    /// A `remap` transform that runs a VRL program from a file.
    pub fn remap_file(file: impl Into<String>) -> Self {
        Transform {
            transform_type: TransformType::Remap,
            file: Some(file.into()),
            ..Default::default()
        }
    }

    /// A `filter` transform that keeps only the events that agree with
    /// `condition`.
    pub fn filter(condition: impl Into<String>) -> Self {
        Transform {
            transform_type: TransformType::Filter,
            condition: Some(condition.into()),
            ..Default::default()
        }
    }

    /// An `exclusive_route` transform. It splits the stream into one output port
    /// for each route. The first route that agrees wins.
    pub fn exclusive_route(routes: Vec<Route>) -> Self {
        Transform {
            transform_type: TransformType::ExclusiveRoute,
            routes: Some(routes),
            ..Default::default()
        }
    }

    /// Sets the inputs of this transform (the ids of the upstream components).
    pub fn with_inputs<I, S>(mut self, inputs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.inputs = inputs.into_iter().map(Into::into).collect();
        self
    }
}

/// A part of the Vector component graph. One source or one sink gives this part.
#[derive(Default)]
pub struct Pipeline {
    pub sources: BTreeMap<String, Component>,
    pub transforms: BTreeMap<String, Transform>,
    pub sinks: BTreeMap<String, Component>,
}

/// The top-level Vector schema options.
#[derive(Serialize)]
pub struct Schema {
    pub log_namespace: bool,
}

impl Default for Schema {
    fn default() -> Self {
        // StrIEM needs the log namespace to carry the source metadata.
        Schema {
            log_namespace: true,
        }
    }
}

/// A complete Vector configuration document. It has the boilerplate and the
/// merged pipelines of every source and sink.
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
    /// Merges a source or sink [`Pipeline`] into this document. Two components
    /// with the same id become one component. One example is a shared HTTP
    /// listener.
    pub fn merge(&mut self, pipeline: Pipeline) {
        self.sources.extend(pipeline.sources);
        self.transforms.extend(pipeline.transforms);
        self.sinks.extend(pipeline.sinks);
    }
}

/// The rules for Vector component ids. These functions are the single source of
/// truth. They set the names of the nodes in a source's normalization chain.
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

    /// The logsource-tag transform: `logsource-<type>_<id>`.
    pub fn logsource(sourcetype: impl Display, id: &str) -> String {
        format!("logsource-{}_{}", sourcetype, id)
    }

    /// The last node, normalized to OCSF: `ocsf-<type>_<id>`.
    pub fn ocsf(sourcetype: impl Display, id: &str) -> String {
        format!("ocsf-{}_{}", sourcetype, id)
    }
}
