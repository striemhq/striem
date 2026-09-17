use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Logsource, Source, SourceType, logsource_meta};
use crate::graph::{Component, Pipeline, RenderCtx, Transform, naming};

/// The Vector component id of the shared HTTP input listener. Vector allows only
/// one `http_server` for each port. Thus every HTTP source uses this one
/// listener. A source does not bind its own listener. Pipelines merge by
/// component id. Thus each source that adds this key becomes one listener.
pub const HTTP_LISTENER: &str = "source-http";

/// The fallback listen address. The service uses it when you configure no HTTP
/// input address.
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

/// The shared `http_server` listener. Some clients, for example GitHub webhooks,
/// send JSON with no `content-type` header. Thus this function does not use
/// Vector's JSON codec. In place of it, the function takes the raw body and
/// parses it with VRL.
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

    /// An HTTP source does not bind its own listener. In place of this, each
    /// source filters the shared listener by request path. Then it tags and
    /// normalizes the events in the normal way:
    /// `source-http -> route -> logsource -> ocsf`.
    fn pipeline(&self, ctx: &RenderCtx) -> anyhow::Result<Pipeline> {
        let sourcetype = self.sourcetype();
        let id = self.id();

        // The source id in the event metadata. It is not the same as the shared
        // listener's component id.
        let source_id = naming::source(&sourcetype, &id);
        let route_id = format!("route-{}_{}", sourcetype, id);
        let logsource_id = naming::logsource(&sourcetype, &id);
        let ocsf_id = naming::ocsf(&sourcetype, &id);

        let mut pipeline = Pipeline::default();

        // The shared listener. The merge makes it one listener for all the HTTP
        // sources.
        pipeline
            .sources
            .insert(HTTP_LISTENER.to_string(), http_listener(ctx)?);

        // Select only this source's input path from the shared listener.
        pipeline.transforms.insert(
            route_id.clone(),
            Transform::filter(format!("%http_server.path == \"/{}\"", id))
                .with_inputs([HTTP_LISTENER.to_string()]),
        );

        // Tag with the logsource metadata.
        pipeline.transforms.insert(
            logsource_id.clone(),
            Transform::remap(logsource_meta(&source_id, &self.logsource()))
                .with_inputs([route_id]),
        );

        // Normalize to OCSF with the VRL that the user gives.
        pipeline.transforms.insert(
            ocsf_id,
            Transform::remap(self.config.vrl.clone()).with_inputs([logsource_id]),
        );

        Ok(pipeline)
    }
}
