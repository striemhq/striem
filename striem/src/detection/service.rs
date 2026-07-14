//! Detection microservice runtime.
//!
//! Wires together everything needed to *run* the detection service:
//! - the Vector gRPC listener (event ingestion, from `striem_vector`),
//! - the Sigma [`DetectionHandler`] that turns events into OCSF findings,
//! - an optional downstream Vector client that forwards those findings,
//! - the detection-admin gRPC service ([`DetectionAdmin`]).
//!
//! The Vector ingestion service and the admin service are mounted on a single
//! [`tonic::transport::Server`] bound to the configured input address, so the
//! API service reaches rule administration at the same endpoint Vector pushes to.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use arc_swap::ArcSwap;
use backoff::{ExponentialBackoff, future::retry};
use log::{debug, info, warn};
use tokio::sync::{RwLock, broadcast};

use sigmars::SigmaCollection;
use striem_common::{SysMessage, event::Event};
use striem_config::{self as config, StrIEMConfig, StringOrList};
use striem_detection::detections_server::DetectionsServer;
use striem_vector::{Client as VectorClient, Server as VectorServer};

use crate::detection::{DetectionHandler, admin::DetectionAdmin};

/// The runnable detection microservice.
pub struct DetectionService {
    detections: Arc<RwLock<SigmaCollection>>,
    config: Arc<ArcSwap<StrIEMConfig>>,
    /// Broadcast channel carrying detection findings to the downstream client.
    events: broadcast::Sender<Arc<Vec<Event>>>,
    sys: broadcast::Sender<SysMessage>,
}

impl DetectionService {
    /// Load configured Sigma rules and prepare the service.
    ///
    /// Rules are loaded and compiled up front so invalid rule sets fail fast,
    /// mirroring the previous in-process behaviour.
    pub async fn new(
        config: Arc<ArcSwap<StrIEMConfig>>,
        sys: broadcast::Sender<SysMessage>,
    ) -> Result<Self> {
        let mut detections = SigmaCollection::default();

        // Support both a single directory and a list of directories.
        let count = match &config.load().detections {
            Some(StringOrList::String(path)) => detections
                .load_from_dir(path)
                .map_err(|e| anyhow!(e.to_string())),
            Some(config::StringOrList::List(paths)) => paths
                .iter()
                .map(|path| {
                    detections
                        .load_from_dir(path)
                        .map_err(|e| anyhow!(e.to_string()))
                })
                .collect::<Result<Vec<_>>>()
                .map(|r| r.iter().sum()),
            None => {
                warn!("No detection rules loaded");
                Ok(0)
            }
        }?;

        // Pre-compile/index rules so runtime matching and later add() calls work.
        detections.with_backend()?;
        info!("... loaded {} Sigma detections", count);

        Ok(Self {
            detections: Arc::new(RwLock::new(detections)),
            config,
            events: broadcast::channel::<Arc<Vec<Event>>>(64).0,
            sys,
        })
    }

    /// Run the service until a shutdown signal is broadcast.
    pub async fn run(&self) -> Result<()> {
        let config = self.config.load();

        let addr = config.input.address();

        let mut server = VectorServer::new();

        // Detection pipeline: ingested events -> Sigma matching -> findings channel.
        let src = server.subscribe()?;
        let mut handler = DetectionHandler::new(
            src,
            self.events.clone(),
            self.detections.clone(),
            self.sys.subscribe(),
        );
        tokio::spawn(async move {
            handler.run().await;
        });

        // Forward findings to a downstream Vector instance, if configured.
        if let Some(ref vector) = config.output {
            self.run_downstream(&vector.cfg.url());
        }

        // Mount both the Vector ingestion service and the admin service on one server.
        let vector_service = server.service()?;
        let admin =
            DetectionAdmin::new(self.detections.clone(), self.config.clone());

        info!("... detection service listening on {}", addr);

        let mut shutdown = self.sys.subscribe();
        tonic::transport::Server::builder()
            .add_service(vector_service)
            .add_service(DetectionsServer::new(admin))
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
            .await?;

        Ok(())
    }

    /// Spawn a resilient downstream Vector client that forwards detection findings.
    ///
    /// Retries with exponential backoff so transient network failures or Vector
    /// restarts don't tear down the service.
    fn run_downstream(&self, url: &str) {
        let url = url.to_string();
        let rx = self.events.subscribe();
        let shutdown = self.sys.subscribe();
        tokio::spawn(async move {
            let mut sink = retry(ExponentialBackoff::default(), || async {
                VectorClient::new(&url, rx.resubscribe(), shutdown.resubscribe())
                    .await
                    .map_err(|e| {
                        warn!("Failed to connect to Vector at {}: {}", url, e);
                        e.into()
                    })
            })
            .await
            .expect("Failed to connect to downstream Vector client");

            info!("... connected to downstream Vector at {}", url);
            debug!("forwarding detection findings downstream");

            sink.run().await.expect("Vector client failed");
        });
    }
}
