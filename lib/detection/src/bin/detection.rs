//! `detection` — StrIEM's detection service that Vector feeds. rsigma is the
//! engine.
//!
//! Usage:
//!   detection --rules <DIR> [--input <ADDR>] [--output <URL>]
//!             [--batch-size <N>]
//!             [--logsource-field-map <KV>] [--event-logsource <KV>]
//!             [--no-logsource-pruning]
//!
//! Example:
//!   detection --rules ./rules \
//!       --input 0.0.0.0:6000 \
//!       --output http://vector:6001 \
//!       --event-logsource product=windows
//!
//! Each flag also has an environment variable (`RSIGMA_DETECTION_*`).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use log::{error, info};
use striem_detection::config::LogsourceConfig;
use striem_detection::{Config, DetectionService};
use striem_common::SysMessage;
use tokio::sync::broadcast;

fn main() -> ExitCode {
    let config = match parse_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("configuration error: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to start tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    runtime.block_on(async move {
        // Starts logs, traces (OTLP when configured), and the metrics registry.
        striem_telemetry::init("detection");

        // Shows the metrics at a small HTTP server, because the detection
        // service uses only gRPC. The API dashboard reads this endpoint.
        let metrics_addr: std::net::SocketAddr = std::env::var("RSIGMA_DETECTION_METRICS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| ([0, 0, 0, 0], 9101).into());
        tokio::spawn(async move {
            if let Err(e) = striem_telemetry::serve_metrics(metrics_addr).await {
                error!("metrics server failed: {e}");
            }
        });

        let (sys_tx, _sys_rx) = broadcast::channel::<SysMessage>(16);

        let service = match DetectionService::new(config, sys_tx.clone()) {
            Ok(svc) => svc,
            Err(e) => {
                error!("failed to initialize detection service: {e}");
                return ExitCode::FAILURE;
            }
        };

        // Send Shutdown on Ctrl-C or SIGTERM. Thus every task stops cleanly.
        let shutdown_tx = sys_tx.clone();
        tokio::spawn(async move {
            wait_for_signal().await;
            info!("shutdown signal received");
            let _ = shutdown_tx.send(SysMessage::Shutdown);
        });

        match service.run().await {
            Ok(()) => {
                info!("detection service stopped");
                ExitCode::SUCCESS
            }
            Err(e) => {
                error!("detection service failed: {e}");
                ExitCode::FAILURE
            }
        }
    })
}

/// Waits for an OS shutdown signal (SIGINT, or SIGTERM on Unix).
async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

const USAGE: &str = "\
Usage: detection --rules <DIR> [options]

Options:
  --rules <PATH>                 Sigma rules directory or file (required)
  --input <ADDR>                 Vector ingest bind address [default: 0.0.0.0:6000]
  --output <URL>                 Downstream Vector endpoint for findings (optional)
  --batch-size <N>               Events per engine evaluation [default: 64]
  --logsource-field-map <KV>     Override which metadata.logsource sub-keys feed
                                 each dimension (product=<key>,service=<key>,...);
                                 rarely needed — defaults to the standard keys
  --event-logsource <KV>         Static logsource when the event's metadata
                                 carries none (product=...,service=...,category=...)
  --no-logsource-pruning         Disable conflict-based logsource pruning
  -h, --help                     Print this help

Environment fallbacks: RSIGMA_DETECTION_RULES, _INPUT, _OUTPUT, _BATCH_SIZE,
_LOGSOURCE_FIELD_MAP, _EVENT_LOGSOURCE.";

/// Parses the CLI flags. Each flag also reads a `RSIGMA_DETECTION_*` environment
/// variable.
fn parse_config() -> Result<Config, String> {
    let mut rules: Option<String> = std::env::var("RSIGMA_DETECTION_RULES").ok();
    let mut input: Option<String> = std::env::var("RSIGMA_DETECTION_INPUT").ok();
    let mut output: Option<String> = std::env::var("RSIGMA_DETECTION_OUTPUT").ok();
    let mut batch_size: Option<String> = std::env::var("RSIGMA_DETECTION_BATCH_SIZE").ok();
    let mut field_map: Option<String> = std::env::var("RSIGMA_DETECTION_LOGSOURCE_FIELD_MAP").ok();
    let mut event_logsource: Option<String> =
        std::env::var("RSIGMA_DETECTION_EVENT_LOGSOURCE").ok();
    let mut pruning_enabled = true;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut take = |name: &str| -> Result<String, String> {
            args.next().ok_or_else(|| format!("missing value for {name}"))
        };
        match arg.as_str() {
            "--rules" => rules = Some(take("--rules")?),
            "--input" => input = Some(take("--input")?),
            "--output" => output = Some(take("--output")?),
            "--batch-size" => batch_size = Some(take("--batch-size")?),
            "--logsource-field-map" => field_map = Some(take("--logsource-field-map")?),
            "--event-logsource" => event_logsource = Some(take("--event-logsource")?),
            "--no-logsource-pruning" => pruning_enabled = false,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
    }

    let rules = rules.ok_or("--rules is required")?;
    let input: SocketAddr = input
        .unwrap_or_else(|| "0.0.0.0:6000".to_string())
        .parse()
        .map_err(|e| format!("invalid --input address: {e}"))?;
    let batch_size = match batch_size {
        Some(s) => s
            .parse::<usize>()
            .map_err(|e| format!("invalid --batch-size: {e}"))?
            .max(1),
        None => 64,
    };

    Ok(Config {
        input,
        output: output.filter(|s| !s.is_empty()),
        rules: PathBuf::from(rules),
        batch_size,
        logsource: LogsourceConfig {
            enabled: pruning_enabled,
            field_map: field_map.filter(|s| !s.is_empty()),
            event_logsource: event_logsource.filter(|s| !s.is_empty()),
        },
    })
}
