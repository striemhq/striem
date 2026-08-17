//! End-to-end proof that rsigma-detection prunes on the conflict-based
//! `logsource_compatible` path, with the logsource taken from each event's
//! `metadata["logsource"]` (StrIEM's convention) — not from the log body.
//!
//! Three rules with identical detections but different logsources are loaded;
//! a single event whose metadata declares `product: windows` is evaluated. The
//! Windows rule and the logsource-less rule fire; the Linux rule is pruned
//! because its declared product conflicts with the event's. An event whose
//! metadata carries no logsource fails open (all three fire). Driven through
//! the real `DetectionHandler` so the metadata→logsource→extractor bridge is
//! exercised exactly as in production.

use std::collections::HashMap;
use std::sync::Arc;

use rsigma_detection::DetectionHandler;
use rsigma_detection::config::{Config, LogsourceConfig};
use rsigma_eval::CorrelationConfig;
use rsigma_runtime::{LogProcessor, NoopMetrics, RuntimeEngine};
use serde_json::json;
use striem_common::{SysMessage, event::Event};
use tokio::sync::{broadcast, mpsc};

const WINDOWS_RULE: &str = r#"
title: WinRule
id: aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa
status: experimental
logsource:
    product: windows
detection:
    sel:
        EventID: 1
    condition: sel
"#;

const LINUX_RULE: &str = r#"
title: LinuxRule
id: bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb
status: experimental
logsource:
    product: linux
detection:
    sel:
        EventID: 1
    condition: sel
"#;

const NO_LOGSOURCE_RULE: &str = r#"
title: AnyRule
id: cccccccc-cccc-cccc-cccc-cccccccccccc
status: experimental
detection:
    sel:
        EventID: 1
    condition: sel
"#;

fn write_rules(dir: &std::path::Path) {
    std::fs::write(dir.join("windows.yml"), WINDOWS_RULE).unwrap();
    std::fs::write(dir.join("linux.yml"), LINUX_RULE).unwrap();
    std::fs::write(dir.join("any.yml"), NO_LOGSOURCE_RULE).unwrap();
}

/// Build a processor the way `DetectionService::new` does: install the
/// logsource extractor from config, then load rules.
fn processor_for(dir: &std::path::Path, logsource: LogsourceConfig) -> Arc<LogProcessor> {
    let config = Config {
        input: "0.0.0.0:6000".parse().unwrap(),
        output: None,
        rules: dir.to_path_buf(),
        batch_size: 64,
        logsource,
    };
    let extractor = config.build_logsource_extractor().unwrap();
    let mut engine = RuntimeEngine::new(
        config.rules.clone(),
        Vec::new(),
        CorrelationConfig::default(),
        false,
    );
    engine.set_logsource_extractor(extractor);
    engine.load_rules().expect("rules load");
    Arc::new(LogProcessor::new(engine, Arc::new(NoopMetrics)))
}

/// A matching event whose logsource (if any) is carried in metadata.
fn event(logsource: Option<serde_json::Value>) -> Event {
    let mut metadata = HashMap::new();
    if let Some(ls) = logsource {
        metadata.insert("logsource".to_string(), ls);
    }
    Event {
        id: uuid::Uuid::now_v7(),
        data: json!({ "EventID": 1 }),
        metadata,
    }
}

/// Run one event through the handler and collect the titles of fired rules.
async fn fired_titles(processor: Arc<LogProcessor>, event: Event) -> Vec<String> {
    let (src_tx, src_rx) = mpsc::channel::<Vec<Event>>(4);
    let (dest_tx, mut dest_rx) = broadcast::channel::<Arc<Vec<Event>>>(4);
    let (_sys_tx, sys_rx) = broadcast::channel::<SysMessage>(4);

    let mut handler = DetectionHandler::new(src_rx, dest_tx, processor, 64, sys_rx);
    let worker = tokio::spawn(async move { handler.run().await });

    src_tx.send(vec![event]).await.unwrap();
    drop(src_tx);

    let findings = dest_rx.recv().await.expect("a finding batch");
    worker.await.unwrap();

    let mut titles: Vec<String> = findings
        .iter()
        .map(|f| f.data["finding_info"]["title"].as_str().unwrap().to_string())
        .collect();
    titles.sort();
    titles.dedup();
    titles
}

#[tokio::test]
async fn conflicting_logsource_rule_is_pruned() {
    let dir = tempfile::tempdir().unwrap();
    write_rules(dir.path());
    let processor = processor_for(dir.path(), LogsourceConfig::default());

    // metadata.logsource = {product: windows}: Windows + logsource-less fire.
    let fired = fired_titles(processor, event(Some(json!({ "product": "windows" })))).await;
    assert_eq!(
        fired,
        vec!["AnyRule".to_string(), "WinRule".to_string()],
        "Linux rule must be pruned as conflicting; got {fired:?}"
    );
}

#[tokio::test]
async fn missing_logsource_fails_open() {
    let dir = tempfile::tempdir().unwrap();
    write_rules(dir.path());
    let processor = processor_for(dir.path(), LogsourceConfig::default());

    // No logsource in metadata: fail-open, every rule evaluates.
    let fired = fired_titles(processor, event(None)).await;
    assert_eq!(
        fired,
        vec![
            "AnyRule".to_string(),
            "LinuxRule".to_string(),
            "WinRule".to_string()
        ],
        "absent logsource must fail open; got {fired:?}"
    );
}

#[tokio::test]
async fn pruning_disabled_runs_every_rule() {
    let dir = tempfile::tempdir().unwrap();
    write_rules(dir.path());
    let processor = processor_for(
        dir.path(),
        LogsourceConfig {
            enabled: false,
            ..Default::default()
        },
    );

    // With no extractor installed, even a conflicting Linux rule runs.
    let fired = fired_titles(processor, event(Some(json!({ "product": "windows" })))).await;
    assert_eq!(
        fired,
        vec![
            "AnyRule".to_string(),
            "LinuxRule".to_string(),
            "WinRule".to_string()
        ],
        "disabled pruning must evaluate all rules; got {fired:?}"
    );
}
