//! Round-trip test for the detection worker: an ingested StrIEM [`Event`] that
//! matches a rule produces a broadcast OCSF Detection Finding correlated back
//! to the source event.

use std::collections::HashMap;
use std::sync::Arc;

use rsigma_detection::DetectionHandler;
use rsigma_detection::config::{Config, LogsourceConfig};
use rsigma_eval::CorrelationConfig;
use rsigma_runtime::{LogProcessor, NoopMetrics, RuntimeEngine};
use striem_common::{SysMessage, event::Event};
use tokio::sync::{broadcast, mpsc};

const RULE: &str = r#"
title: WinRule
id: aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa
status: experimental
level: high
tags:
    - attack.t1059
logsource:
    product: windows
detection:
    sel:
        EventID: 1
    condition: sel
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
    engine.load_rules().unwrap();
    Arc::new(LogProcessor::new(engine, Arc::new(NoopMetrics)))
}

#[tokio::test]
async fn matching_event_emits_correlated_ocsf_finding() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("win.yml"), RULE).unwrap();
    let processor = processor(dir.path());

    let (src_tx, src_rx) = mpsc::channel::<Vec<Event>>(4);
    let (dest_tx, mut dest_rx) = broadcast::channel::<Arc<Vec<Event>>>(4);
    let (_sys_tx, sys_rx) = broadcast::channel::<SysMessage>(4);

    let mut handler = DetectionHandler::new(src_rx, dest_tx, processor, 64, sys_rx);
    let worker = tokio::spawn(async move { handler.run().await });

    // A matching Windows event: logsource in metadata (StrIEM convention),
    // plus a Vector ingest timestamp so the finding carries the event time.
    let mut metadata = HashMap::new();
    metadata.insert(
        "logsource".to_string(),
        serde_json::json!({ "product": "windows" }),
    );
    metadata.insert(
        "vector".to_string(),
        serde_json::json!({ "ingest_timestamp": "2026-01-02T03:04:05Z" }),
    );
    let event = Event {
        id: uuid::Uuid::now_v7(),
        data: serde_json::json!({ "EventID": 1 }),
        metadata,
    };
    let source_id = event.id.to_string();

    src_tx.send(vec![event]).await.unwrap();
    // Closing the source lets the worker drain and exit cleanly.
    drop(src_tx);

    let findings = dest_rx.recv().await.expect("a finding batch");
    assert_eq!(findings.len(), 1, "one rule should fire");
    let finding = &findings[0];

    assert_eq!(finding.data["class_uid"], 2004);
    assert_eq!(finding.data["finding_info"]["title"], "WinRule");
    assert_eq!(finding.data["severity"], "High");
    assert_eq!(
        finding.data["finding_info"]["attacks"][0]["technique"]["uid"],
        "T1059"
    );
    // Correlated back to the triggering event id.
    assert_eq!(finding.data["metadata"]["correlation_uid"], source_id);
    // Event time carried from the Vector ingest timestamp (epoch millis).
    let expected_ms = chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(finding.data["time"], expected_ms);
    // The finding's own uid is mirrored onto wire metadata.
    assert_eq!(finding.metadata["uid"], finding.data["metadata"]["uid"]);

    worker.await.unwrap();
}
