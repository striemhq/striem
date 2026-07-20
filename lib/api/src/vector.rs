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


    cfg.transforms.insert(
        "final-ocsf".to_string(),
        Transform::remap(/*r#"
          categories = {
            "1": "system",
            "2": "findings",
            "3": "iam",
            "4": "network",
            "5": "discovery",
            "6": "application",
            "7": "remediation",
            "8": "unmanned_systems"
          }
          classes = {
            "1001": "file_activity",
            "1002": "kernel_extension_activity",
            "1003": "kernel_activity",
            "1004": "memory_activity",
            "1005": "module_activity",
            "1006": "scheduled_job_activity",
            "1007": "process_activity",
            "1008": "event_log_activity",
            "1009": "script_activity",
            "1010": "peripheral_activity",
            "2001": "security_finding",
            "2002": "vulnerability_finding",
            "2003": "compliance_finding",
            "2004": "detection_finding",
            "2005": "incident_finding",
            "2006": "data_security_finding",
            "2007": "application_security_posture_finding",
            "2008": "iam_analysis_finding",
            "3001": "account_change",
            "3002": "authentication",
            "3003": "authorize_session",
            "3004": "entity_management",
            "3005": "user_access",
            "3006": "group_management",
            "4001": "network_activity",
            "4002": "http_activity",
            "4003": "dns_activity",
            "4004": "dhcp_activity",
            "4005": "rdp_activity",
            "4006": "smb_activity",
            "4007": "ssh_activity",
            "4008": "ftp_activity",
            "4009": "email_activity",
            "4010": "network_file_activity",
            "4011": "email_file_activity",
            "4012": "email_url_activity",
            "4013": "ntp_activity",
            "4014": "tunnel_activity",
            "5001": "inventory_info",
            "5002": "config_state",
            "5003": "user_inventory",
            "5004": "patch_state",
            "5006": "kernel_object_query",
            "5007": "file_query",
            "5008": "folder_query",
            "5009": "admin_group_query",
            "5010": "job_query",
            "5011": "module_query",
            "5012": "network_connection_query",
            "5013": "networks_query",
            "5014": "peripheral_device_query",
            "5015": "process_query",
            "5016": "service_query",
            "5017": "session_query",
            "5018": "user_query",
            "5019": "device_config_state_change",
            "5020": "software_info",
            "5021": "osint_inventory_info",
            "5022": "startup_item_query",
            "5023": "cloud_resources_inventory_info",
            "5040": "evidence_info",
            "6001": "web_resources_activity",
            "6002": "application_lifecycle",
            "6003": "api_activity",
            "6004": "web_resource_access_activity",
            "6005": "datastore_activity",
            "6006": "file_hosting",
            "6007": "scan_activity",
            "6008": "application_error",
            "7001": "remediation_activity",
            "7002": "file_remediation_activity",
            "7003": "process_remediation_activity",
            "7004": "network_remediation_activity",
            "8001": "drone_flights_activity",
            "8002": "airborne_broadcast_activity"
            }*/
            format!("ocsf = {}\n{}", include_str!("../ocsf_class_category.json"),
            r#"class_uid = to_string(.class_uid) ?? "0"
            category_uid = to_string(.category_uid) ?? "0"

            %ocsf = {}
            %ocsf.class = get(ocsf.classes, [class_uid]) ?? null
            %ocsf.category = get(ocsf.categories, [category_uid]) ?? null
            "#))
        .with_inputs(["ocsf-*"]),
    );

    cfg.sinks.insert(
        "sink-striem".to_string(),
        Component::Table(toml! {
            type = "vector"
            inputs = ["logsource-*"]
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
        "ocsf-striem".to_string(),
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
        cfg.merge(sink.pipeline(&ctx).map_err(internal)?);
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
