//! The detection microservice that you can run.
//!
//! This module connects all the parts to run the detection service:
//! - StrIEM's Vector gRPC input listener ([`striem_vector::Server`]),
//! - the rsigma [`RuntimeEngine`] and [`LogProcessor`]. They have a
//!   [`LogSourceExtractor`](rsigma_eval::LogSourceExtractor), so the evaluation
//!   takes the `logsource_compatible` conflict-pruning path.
//! - the [`DetectionHandler`] that changes events into OCSF findings,
//! - an optional downstream Vector [`Client`](striem_vector::Client) that
//!   forwards these findings.
//!
//! This is the same topology as StrIEM's own `detection` service. But the
//! `sigmars` engine is replaced with rsigma's runtime.

use std::sync::Arc;

use anyhow::{Context, Result};
use backoff::{ExponentialBackoff, future::retry};
use log::{debug, info, warn};
use rsigma_eval::CorrelationConfig;
use rsigma_runtime::{LogProcessor, NoopMetrics, RuntimeEngine};
use striem_common::{SysMessage, event::Event};
use crate::proto::sigma_collection_server::SigmaCollectionServer;
use striem_vector::{Client as VectorClient, Server as VectorServer};
use tokio::sync::broadcast;

use crate::admin::CollectionAdmin;
use crate::config::Config;
use crate::detection::DetectionHandler;

/// The detection service. It loads the rules. It holds the engine and the
/// channels for the input → detect → output pipeline.
pub struct DetectionService {
    config: Config,
    processor: Arc<LogProcessor>,
    /// The broadcast channel that sends the findings to the downstream Vector
    /// client.
    events: broadcast::Sender<Arc<Vec<Event>>>,
    sys: broadcast::Sender<SysMessage>,
}

impl DetectionService {
    /// Loads the configured Sigma rules and prepares the service.
    ///
    /// The function installs the logsource extractor first. Then it loads and
    /// compiles the rules. Thus a bad rule set, or a bad logsource
    /// configuration, fails immediately at startup. This is the same as StrIEM's
    /// behavior.
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

        // The install of the extractor before load_rules() selects the
        // conflict-based `logsource_compatible` pruning path. The engine skips a
        // rule whose logsource conflicts with the extracted event logsource. A
        // rule with no conflict still runs. A rule with no logsource also runs.
        // This is fail-open: an event with no logsource evaluates against all
        // the rules.
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

    /// Runs the service until a [`SysMessage::Shutdown`] comes.
    pub async fn run(&self) -> Result<()> {
        let addr = self.config.input;

        let mut server = VectorServer::new();
        let src = server.subscribe()?;

        // The detection pipeline: input events -> rsigma engine -> findings.
        let mut handler = DetectionHandler::new(
            src,
            self.events.clone(),
            self.processor.clone(),
            self.config.batch_size,
            self.sys.subscribe(),
        );
        tokio::spawn(async move { handler.run().await });

        // Send the findings to a downstream Vector, if you configure one.
        if let Some(ref url) = self.config.output {
            self.run_downstream(url);
        } else {
            info!("no downstream Vector configured; findings are engine-side only");
        }

        let vector_service = server.service()?;
        // Rule administration (List/Get/Create/SetEnabled) on the same gRPC
        // server. It uses the engine's on-disk rules directory.
        let admin = SigmaCollectionServer::new(CollectionAdmin::new(self.processor.clone()));
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

    /// Starts a downstream Vector client that forwards the detection findings.
    /// The client tries again with exponential backoff. Thus a short network
    /// failure or a Vector restart does not stop the service. This is the same
    /// as StrIEM's `DetectionService::run_downstream`.
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
