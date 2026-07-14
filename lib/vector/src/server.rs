//! Vector gRPC server implementation.
//!
//! Implements Vector's protocol for receiving events via gRPC.
//! Only supports log events; metric and trace events are rejected.
//!
//! # Protocol
//! Vector sends PushEventsRequest with batches of events.
//! The server forwards each batch to the single downstream consumer (the
//! detection engine). Parquet storage is handled by Vector itself, so there is
//! no in-process fan-out to justify broadcasting.

use anyhow::{Result, anyhow};
use log::debug;
use striem_common::event::Event;
use tokio::sync::mpsc;

use crate::{
    event::event_wrapper::Event as VectorEventWrapper,
    vector::{
        self,
        vector_server::{Vector, VectorServer},
    },
};

/// Vector protocol implementation. Receives event batches over gRPC and forwards
/// them to the single downstream consumer (the detection engine).
pub struct VectorService {
    channel: mpsc::Sender<Vec<Event>>,
}

#[tonic::async_trait]
impl Vector for VectorService {
    /// Receive log events and forward them to the detection engine.
    ///
    /// # Event Type Filtering
    /// Only log events are supported. Metrics and traces are rejected
    /// with UNIMPLEMENTED status to fail fast rather than silently drop.
    ///
    /// # Backpressure
    /// The batch is moved into the channel with no cloning. `send` awaits
    /// capacity, so a slow detection engine backpressures ingestion rather than
    /// dropping events.
    async fn push_events(
        &self,
        request: tonic::Request<vector::PushEventsRequest>,
    ) -> Result<tonic::Response<vector::PushEventsResponse>, tonic::Status> {
        let events = request
            .into_inner()
            .events
            .iter_mut()
            .map(|wrapped| {
                let event = wrapped
                    .event
                    .take()
                    .ok_or_else(|| tonic::Status::invalid_argument("missing event"))?;
                match event {
                    VectorEventWrapper::Log(e) => {
                        debug!("received log event: {:?}", e);
                        Ok(e.into())
                    }
                    _ => Err(tonic::Status::unimplemented(
                        "only log events are supported by this server",
                    )),
                }
            })
            .collect::<Result<Vec<Event>, tonic::Status>>()?;

        self.channel
            .send(events)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(vector::PushEventsResponse {}))
    }

    async fn health_check(
        &self,
        _: tonic::Request<vector::HealthCheckRequest>,
    ) -> Result<tonic::Response<vector::HealthCheckResponse>, tonic::Status> {
        Ok(tonic::Response::new(vector::HealthCheckResponse {
            status: vector::ServingStatus::Serving.into(),
        }))
    }
}

/// Vector gRPC server holding the ingestion channel to the detection engine.
/// The channel is created at construction; the receiver is handed to the
/// consumer via [`Server::subscribe`] and the service via [`Server::service`].
pub struct Server {
    service: Option<VectorService>,
    rx: Option<mpsc::Receiver<Vec<Event>>>,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    /// Create server with a 256-batch buffer.
    ///
    /// # Buffer Sizing
    /// 256 buffers slow-consumer bursts without excessive memory. Vector batches
    /// events, so this represents ~10-50 Vector batches depending on its batch
    /// settings; beyond it, `push_events` awaits capacity (backpressure).
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            service: Some(VectorService { channel: tx }),
            rx: Some(rx),
        }
    }

    /// Take the configured tonic service so the caller can mount it on a
    /// [`tonic::transport::Server`] alongside other services (e.g. the
    /// detection-admin service). Consumes the internal service; call
    /// [`Server::subscribe`] first to obtain event receivers.
    ///
    /// Gzip decompression is accepted to match Vector's default client
    /// compression settings.
    pub fn service(&mut self) -> Result<VectorServer<VectorService>> {
        let service = self
            .service
            .take()
            .ok_or_else(|| anyhow!("service already taken"))?;

        Ok(VectorServer::new(service).accept_compressed(tonic::codec::CompressionEncoding::Gzip))
    }

    /// Take the ingestion receiver. Single-consumer: can only be called once.
    pub fn subscribe(&mut self) -> Result<mpsc::Receiver<Vec<Event>>> {
        self.rx
            .take()
            .ok_or_else(|| anyhow!("event receiver already taken"))
    }
}
