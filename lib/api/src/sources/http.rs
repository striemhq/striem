use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Logsource, Source, SourceType, logsource_meta};
use crate::graph::{Component, Pipeline, RenderCtx, Transform, naming};

/// The Vector component id of the shared HTTP ingest listener. Vector only
/// allows one `http_server` per port, so every HTTP source multiplexes off
/// this single listener rather than binding its own. Because pipelines merge
/// by component id, each source contributing this key de-duplicates to one
/// listener.
pub const HTTP_LISTENER: &str = "source-http";

/// Fallback listen address used when no HTTP ingest address is configured.
const DEFAULT_HTTP_ADDRESS: &str = "0.0.0.0:8080";

pub struct HttpRoute {
    pub(crate) id: String,
    pub(crate) config: HttpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub(crate) name: Option<String>,
    pub(crate) logsource: BTreeMap<String, String>,
    pub(crate) vrl: String,
}

/// The shared `http_server` listener. Github webhooks (among others) send JSON
/// without a `content-type` header, so rather than rely on Vector's JSON codec
/// we take the raw body and parse it with VRL.
fn http_listener(ctx: &RenderCtx) -> anyhow::Result<Component> {
    let address = ctx
        .http_address
        .clone()
        .unwrap_or_else(|| DEFAULT_HTTP_ADDRESS.to_string());

    let vrl = [
        r#"body, _ = to_string(.)"#,
        r#"if !is_null(body) {"#,
        r#"  . = parse_json(body) ?? body"#,
        r#"}"#,
        r#"."#,
    ]
    .join("\n");

    Ok(Component::Table(toml::toml! {
        type = "http_server"
        address = address
        headers = ["*"]
        strict_path = false

        [decoding]
        codec = "vrl"
        vrl = { source = vrl }
    }))
}

impl Source for HttpRoute {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn name(&self) -> String {
        self.config.name.clone().unwrap_or_else(|| self.id.clone())
    }

    fn sourcetype(&self) -> SourceType {
        SourceType::Http
    }

    fn config(&self) -> &dyn erased_serde::Serialize {
        &self.config
    }

    fn logsource(&self) -> Logsource {
        Logsource {
            vendor: self.config.logsource.get("vendor").cloned(),
            product: self.config.logsource.get("product").cloned(),
            service: self.config.logsource.get("service").cloned(),
        }
    }

    /// HTTP sources don't bind their own listener. Instead each source filters
    /// the shared listener by request path, then tags and normalizes as usual:
    /// `source-http -> route -> logsource -> ocsf`.
    fn pipeline(&self, ctx: &RenderCtx) -> anyhow::Result<Pipeline> {
        let sourcetype = self.sourcetype();
        let id = self.id();

        // The logical source id recorded in event metadata, distinct from the
        // shared listener's component id.
        let source_id = naming::source(&sourcetype, &id);
        let route_id = format!("route-{}_{}", sourcetype, id);
        let logsource_id = naming::logsource(&sourcetype, &id);
        let ocsf_id = naming::ocsf(&sourcetype, &id);

        let mut pipeline = Pipeline::default();

        // Shared listener; de-duplicated across all HTTP sources on merge.
        pipeline
            .sources
            .insert(HTTP_LISTENER.to_string(), http_listener(ctx)?);

        // Select only this source's ingest path off the shared listener.
        pipeline.transforms.insert(
            route_id.clone(),
            Transform::filter(format!("%http_server.path == \"/{}\"", id))
                .with_inputs([HTTP_LISTENER.to_string()]),
        );

        // Tag with logsource metadata.
        pipeline.transforms.insert(
            logsource_id.clone(),
            Transform::remap(logsource_meta(&source_id, &self.logsource()))
                .with_inputs([route_id]),
        );

        // Normalize to OCSF with the user-supplied VRL.
        pipeline.transforms.insert(
            ocsf_id,
            Transform::remap(self.config.vrl.clone()).with_inputs([logsource_id]),
        );

        Ok(pipeline)
    }
}
