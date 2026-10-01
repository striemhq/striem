//! An end-to-end test that the detection service prunes on the conflict-based
//! `logsource_compatible` path. The logsource comes from each event's
//! `metadata["logsource"]` (StrIEM's convention). It does not come from the log
//! body.
//!
//! The test loads three rules. The rules have the same detection but different
//! logsources. The test evaluates one event whose metadata has
//! `product: windows`. The Windows rule fires. The rule with no logsource also
//! fires. The engine prunes the Linux rule, because its product conflicts with
//! the event's product. An event with no logsource in its metadata fails open;
//! then all three rules fire. The test runs through the real `DetectionHandler`.
//! Thus it exercises the metadata → logsource → extractor path in the same way
//! as production.

use std::collections::HashMap;
use std::sync::Arc;

use striem_detection::DetectionHandler;
use striem_detection::config::{Config, LogsourceConfig};
use rsigma_eval::CorrelationConfig;
use striem_detection::{Processor, RuntimeEngine};
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

/// Makes a processor in the same way as `DetectionService::new`. It installs the
/// logsource extractor from the config, then it loads the rules.
fn processor_for(dir: &std::path::Path, logsource: LogsourceConfig) -> Arc<Processor> {
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
    Arc::new(Processor::new(engine))
}

/// A matching event. Its logsource, if there is one, is in the metadata.
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

/// Runs one event through the handler. Collects the titles of the rules that
/// fired.
async fn fired_titles(processor: Arc<Processor>, event: Event) -> Vec<String> {
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

    // metadata.logsource = {product: windows}: the Windows rule and the rule
    // with no logsource fire.
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

    // No logsource in the metadata: fail-open, so every rule evaluates.
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

    // With no extractor installed, the Linux rule runs too, even though it
    // conflicts.
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
