//! Vector gRPC server.
//!
//! This module obeys Vector's protocol to receive events with gRPC. It accepts
//! only log events. It rejects metric events and trace events.
//!
//! # Protocol
//! Vector sends a PushEventsRequest with a batch of events. The server sends
//! each batch to one downstream consumer, the detection engine. Vector writes
//! the parquet storage. Thus the server does not send events to more than one
//! consumer.

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

/// Vector protocol implementation. It receives batches of events with gRPC. It
/// sends the batches to one downstream consumer, the detection engine.
pub struct VectorService {
    channel: mpsc::Sender<Vec<Event>>,
}

#[tonic::async_trait]
impl Vector for VectorService {
    /// Receives log events and sends them to the detection engine.
    ///
    /// # Event Types
    /// The server accepts only log events. It rejects metric events and trace
    /// events with the UNIMPLEMENTED status. Thus it fails immediately. It does
    /// not drop these events without a message.
    ///
    /// # Backpressure
    /// The server moves the batch into the channel. It does not copy the batch.
    /// The `send` operation waits for space in the channel. Thus a slow
    /// detection engine slows the input. The server does not drop events.
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

/// Vector gRPC server. It holds the input channel to the detection engine.
/// `Server::new` makes the channel. `Server::subscribe` gives the receiver to
/// the consumer. `Server::service` gives the tonic service, which holds the
/// sender.
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
    /// Makes a server with a buffer for 256 batches.
    ///
    /// # Buffer Size
    /// The buffer of 256 holds bursts from a slow consumer. It does not use too
    /// much memory. Vector sends events in batches. Thus this buffer holds
    /// approximately 10 to 50 Vector batches. The count depends on Vector's
    /// batch configuration. When the buffer is full, `push_events` waits for
    /// space.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(256);
        Self {
            service: Some(VectorService { channel: tx }),
            rx: Some(rx),
        }
    }

    /// Gives the tonic service. The caller mounts it on a
    /// [`tonic::transport::Server`] with other services. One example is the
    /// detection-admin service. This function takes the internal service. Call
    /// [`Server::subscribe`] before this function to get the event receiver.
    ///
    /// The service accepts Gzip compression. This agrees with Vector's default
    /// client compression.
    pub fn service(&mut self) -> Result<VectorServer<VectorService>> {
        let service = self
            .service
            .take()
            .ok_or_else(|| anyhow!("service already taken"))?;

        Ok(VectorServer::new(service).accept_compressed(tonic::codec::CompressionEncoding::Gzip))
    }

    /// Takes the input receiver. There is only one consumer, so you can call
    /// this function only one time.
    pub fn subscribe(&mut self) -> Result<mpsc::Receiver<Vec<Event>>> {
        self.rx
            .take()
            .ok_or_else(|| anyhow!("event receiver already taken"))
    }
}
