use crate::{ApiState, sinks::SINKS, sources::SOURCES};
use axum::{Router, extract::State, routing::get};
use striem_config::output::Destination;
use toml::{Table, toml};

async fn get_vector_config(
    State(state): State<ApiState>,
) -> Result<String, (axum::http::StatusCode, String)> {
    let mut config = toml! {
        [schema]
        log_namespace = true
    };

    let mut transforms = toml::Table::new();

    let mut sources = toml! {
        // this ensures the ocsf-* wildcard input always has at least one producer
        [ocsf-stdin]
        type = "stdin"
        decoding = { codec = "json" }
        framing = { method = "newline_delimited" }
    };

    let fqdn = state
        .config
        .fqdn.clone()
        .unwrap_or_else(|| state.config.input.url());

    let mut sinks = toml! {
        [sink-striem]
        type = "vector"
        inputs = ["ocsf-*"]
        address = fqdn
    };

    if let Some(Destination::Vector(ref cfg)) = state.config.output {
        if let Some(api) = &cfg.api {
            let api_address = api.address().to_string();
            let api_config = toml! {
                [api]
                enabled = true
                address = api_address
            };
            config.extend(api_config);
        }

        let address = cfg.cfg.address().to_string();

        sources.insert(
            "source-striem".to_string(),
            toml! {
                type = "vector"
                address = address
                version = "2"
            }
            .into(),
        );

        // TODO: set valid_tokens based on the list of sources
        if let Some(hec) = &cfg.hec {
            let address = hec.address().to_string();
            sources.insert(
                "source-hec".to_string(),
                toml! {
                    type = "splunk_hec"
                    address = address
                    store_hec_token = true
                }
                .into(),
            );
        }

        if let Some(http) = &cfg.http {
            /* some log producers, notably Github webhooks
             * send JSON data but don't set the content-type header
             * so rather than relying on Vector's json decoding codec
             * take the raw body and attempt to parse it with VRL
             */
            let vrl = [
                r#"body, _ = string(.)"#,
                r#"if !is_null(body) {"#,
                r#"  . = parse_json(body) ?? body"#,
                r#"}"#,
            ]
            .join("\n");

            let address = http.address().to_string();
            sources.extend(toml! {
                [source-http]
                type = "http_server"
                address = address
                headers = ['*']
                strict_path = false

                [source-http.decoding]
                codec = "vrl"
                vrl = {"source" = vrl}
            });
        }
    }

    SOURCES.read().await.iter().for_each(|source| {
        Table::try_from(source)
            .map(|t| {
                if let Some(s) = t.get("sources").and_then(|s| s.as_table()) { sources.extend(s.clone()); }

                if let Some(t) = t.get("transforms").and_then(|t| t.as_table()) { transforms.extend(t.clone()); }
            })
            .ok();
    });

    SINKS.read().await.iter().for_each(|sink| {
        Table::try_from(sink)
            .map(|s| {
                sinks.extend(s);
            })
            .ok();
    });

    if !sources.is_empty() {
        config.insert("sources".to_string(), sources.into());
    }
    if !transforms.is_empty() {
        config.insert("transforms".to_string(), transforms.into());
    }
    if !sinks.is_empty() {
        config.insert("sinks".to_string(), sinks.into());
    }

    Ok(config.to_string())
}

pub fn create_router() -> axum::Router<ApiState> {
    Router::new().route("/", get(get_vector_config))
}
