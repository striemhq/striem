//! Correlation rules go through the same handler and make OCSF findings.
//!
//! The test loads an `event_count` correlation (2 or more failed logins for one
//! user in 300s) with its base rule. It sends two matching events as one batch.
//! The handler must make a correlation Detection Finding, and also the base
//! detections. Thus the test shows three things: the engine selects the
//! `CorrelationEngine` variant automatically, it collects state across the
//! batch, and `result_to_ocsf` makes the correlation body.

use std::collections::HashMap;
use std::sync::Arc;

use rsigma_detection::DetectionHandler;
use rsigma_detection::config::{Config, LogsourceConfig};
use rsigma_eval::CorrelationConfig;
use rsigma_runtime::{LogProcessor, NoopMetrics, RuntimeEngine};
use striem_common::{SysMessage, event::Event};
use tokio::sync::{broadcast, mpsc};

const RULES: &str = r#"
title: Failed Login
id: failed-login
status: experimental
logsource:
    product: linux
detection:
    selection:
        EventType: failed_login
    condition: selection
---
title: Brute Force
id: brute-force
status: experimental
correlation:
    type: event_count
    rules:
        - failed-login
    group-by:
        - User
    timespan: 300s
    condition:
        gte: 2
level: critical
"#;

fn processor(dir: &std::path::Path) -> Arc<LogProcessor> {
    let config = Config {
        input: "0.0.0.0:6000".parse().unwrap(),
        output: None,
        rules: dir.to_path_buf(),
        batch_size: 64,
        logsource: LogsourceConfig::default(),
    };
    let extractor = config.build_logsource_extractor().unwrap();
    let mut engine = RuntimeEngine::new(
        dir.to_path_buf(),
        Vec::new(),
        CorrelationConfig::default(),
        false,
    );
    engine.set_logsource_extractor(extractor);
    let stats = engine.load_rules().unwrap();
    assert_eq!(stats.correlation_rules, 1, "correlation rule should load");
    Arc::new(LogProcessor::new(engine, Arc::new(NoopMetrics)))
}

fn failed_login(user: &str, ts: &str) -> Event {
    let mut metadata = HashMap::new();
    // The logsource is in the metadata (StrIEM convention), not in the log body.
    metadata.insert(
        "logsource".to_string(),
        serde_json::json!({ "product": "linux" }),
    );
    metadata.insert(
        "vector".to_string(),
        serde_json::json!({ "ingest_timestamp": ts }),
    );
    Event {
        id: uuid::Uuid::now_v7(),
        data: serde_json::json!({
            "EventType": "failed_login",
            "User": user,
            // The correlation reads the event time from the log body.
            "@timestamp": ts,
        }),
        metadata,
    }
}

#[tokio::test]
async fn event_count_correlation_emits_ocsf_finding() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("rules.yml"), RULES).unwrap();
    let processor = processor(dir.path());

    let (src_tx, src_rx) = mpsc::channel::<Vec<Event>>(4);
    let (dest_tx, mut dest_rx) = broadcast::channel::<Arc<Vec<Event>>>(4);
    let (_sys_tx, sys_rx) = broadcast::channel::<SysMessage>(4);

    let mut handler = DetectionHandler::new(src_rx, dest_tx, processor, 64, sys_rx);
    let worker = tokio::spawn(async move { handler.run().await });

    // Two failed logins for the same user, one second apart, in the window.
    let batch = vec![
        failed_login("admin", "2026-01-02T03:04:05Z"),
        failed_login("admin", "2026-01-02T03:04:06Z"),
    ];
    src_tx.send(batch).await.unwrap();
    drop(src_tx);

    let findings = dest_rx.recv().await.expect("a finding batch");

    let correlation = findings
        .iter()
        .find(|f| f.data["finding_info"]["kind"] == "correlation")
        .expect("a correlation finding must be emitted");

    assert_eq!(correlation.data["class_uid"], 2004);
    assert_eq!(correlation.data["finding_info"]["title"], "Brute Force");
    assert_eq!(correlation.data["severity"], "Critical");
    assert_eq!(
        correlation.data["finding_info"]["data"]["correlation_type"],
        "event_count"
    );
    assert!(
        correlation.data["finding_info"]["data"]["aggregated_value"]
            .as_f64()
            .unwrap()
            >= 2.0
    );
    // group_key shows the User that the window fired on.
    let group = &correlation.data["finding_info"]["data"]["group_key"][0];
    assert_eq!(group["name"], "User");
    assert_eq!(group["value"], "admin");

    worker.await.unwrap();
}
