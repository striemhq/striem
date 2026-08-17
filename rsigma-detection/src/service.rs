//! The runnable detection microservice.
//!
//! Wires together everything needed to run rsigma-detection:
//! - StrIEM's Vector gRPC ingest listener ([`striem_vector::Server`]),
//! - the rsigma [`RuntimeEngine`] + [`LogProcessor`], configured with a
//!   [`LogSourceExtractor`](rsigma_eval::LogSourceExtractor) so evaluation
//!   takes the `logsource_compatible` conflict-pruning path,
//! - the [`DetectionHandler`] that turns events into OCSF findings,
//! - an optional downstream Vector [`Client`](striem_vector::Client) that
//!   forwards those findings.
//!
//! This is the same topology as StrIEM's own `detection` service, with the
//! `sigmars` engine swapped for rsigma's runtime.

use std::sync::Arc;

use anyhow::{Context, Result};
use backoff::{ExponentialBackoff, future::retry};
use log::{debug, info, warn};
use rsigma_eval::CorrelationConfig;
use rsigma_runtime::{LogProcessor, NoopMetrics, RuntimeEngine};
use striem_common::{SysMessage, event::Event};
use striem_detection::detections_server::DetectionsServer;
use striem_vector::{Client as VectorClient, Server as VectorServer};
use tokio::sync::broadcast;

use crate::admin::DetectionAdmin;
use crate::config::Config;
use crate::detection::DetectionHandler;

/// The detection service: loads rules and holds the engine + channels needed
/// to run the ingest → detect → egress pipeline.
pub struct DetectionService {
    config: Config,
    processor: Arc<LogProcessor>,
    /// Broadcast channel carrying findings to the downstream Vector client.
    events: broadcast::Sender<Arc<Vec<Event>>>,
    sys: broadcast::Sender<SysMessage>,
}

impl DetectionService {
    /// Load configured Sigma rules and prepare the service.
    ///
    /// Rules are loaded and compiled up front — with the logsource extractor
    /// installed first — so an invalid rule set (or an invalid logsource
    /// configuration) fails fast at startup, mirroring StrIEM's behaviour.
    pub fn new(config: Config, sys: broadcast::Sender<SysMessage>) -> Result<Self> {
        let extractor = config
            .build_logsource_extractor()
            .map_err(|e| anyhow::anyhow!("logsource configuration error: {e}"))?;

        let mut engine = RuntimeEngine::new(
            config.rules.clone(),
            Vec::new(),
            CorrelationConfig::default(),
            false,
        );

        // Installing the extractor before load_rules() is what selects the
        // conflict-based `logsource_compatible` pruning path: rules whose
        // logsource conflicts with the extracted event logsource are skipped,
        // while rules with no conflict (and all logsource-less rules) still
        // run. Fail-open — an event with no extractable logsource evaluates
        // against everything.
        engine.set_logsource_extractor(extractor);

        let stats = engine
            .load_rules()
            .map_err(|e| anyhow::anyhow!("failed to load rules: {e}"))?;
        info!(
            "... loaded {} Sigma detections ({} correlation rules), logsource pruning: {}",
            stats.detection_rules, stats.correlation_rules, config.logsource.enabled
        );

        let processor = Arc::new(LogProcessor::new(engine, Arc::new(NoopMetrics)));

        Ok(Self {
            config,
            processor,
            events: broadcast::channel::<Arc<Vec<Event>>>(256).0,
            sys,
        })
    }

    /// Run the service until a [`SysMessage::Shutdown`] is broadcast.
    pub async fn run(&self) -> Result<()> {
        let addr = self.config.input;

        let mut server = VectorServer::new();
        let src = server.subscribe()?;

        // Detection pipeline: ingested events -> rsigma engine -> findings.
        let mut handler = DetectionHandler::new(
            src,
            self.events.clone(),
            self.processor.clone(),
            self.config.batch_size,
            self.sys.subscribe(),
        );
        tokio::spawn(async move { handler.run().await });

        // Forward findings to a downstream Vector, if configured.
        if let Some(ref url) = self.config.output {
            self.run_downstream(url);
        } else {
            info!("no downstream Vector configured; findings are engine-side only");
        }

        let vector_service = server.service()?;
        // Rule administration (List/Get/Create/SetEnabled) over the same gRPC
        // server, backed by the engine's on-disk rules directory.
        let admin = DetectionsServer::new(DetectionAdmin::new(self.processor.clone()));
        info!("... detection service listening on {addr}");

        let mut shutdown = self.sys.subscribe();
        tonic::transport::Server::builder()
            .add_service(vector_service)
            .add_service(admin)
            .serve_with_shutdown(addr, async move {
                loop {
                    match shutdown.recv().await {
                        Ok(SysMessage::Shutdown) => break,
                        Ok(_) => continue,
                        Err(_) => {
                            warn!("system broadcast channel closed unexpectedly");
                            break;
                        }
                    }
                }
                info!("detection service shutting down...");
            })
            .await
            .context("Vector ingest server failed")?;

        Ok(())
    }

    /// Spawn a resilient downstream Vector client that forwards detection
    /// findings, retrying with exponential backoff so transient network
    /// failures or Vector restarts don't tear down the service. Mirrors
    /// StrIEM's `DetectionService::run_downstream`.
    fn run_downstream(&self, url: &str) {
        let url = url.to_string();
        let rx = self.events.subscribe();
        let shutdown = self.sys.subscribe();
        tokio::spawn(async move {
            let mut sink = retry(ExponentialBackoff::default(), || async {
                VectorClient::new(&url, rx.resubscribe(), shutdown.resubscribe())
                    .await
                    .map_err(|e| {
                        warn!("Failed to connect to Vector at {url}: {e}");
                        e.into()
                    })
            })
            .await
            .expect("Failed to connect to downstream Vector client");

            info!("... connected to downstream Vector at {url}");
            debug!("forwarding detection findings downstream");

            sink.run().await.expect("Vector client failed");
        });
    }
}
