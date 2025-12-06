use crate::{
    event::{EventWrapper, event_wrapper::Event as VectorEvent},
    vector::{self, vector_client::VectorClient},
};
use anyhow::Result;
use log::info;
use std::sync::Arc;
use striem_common::event::Event;
use tokio::sync::broadcast;

pub struct Client {
    client: VectorClient<tonic::transport::channel::Channel>,
    rx: broadcast::Receiver<Arc<Vec<Event>>>,
    shutdown: broadcast::Receiver<()>,
}

impl Client {
    pub async fn new(
        addr: &str,
        rx: broadcast::Receiver<Arc<Vec<Event>>>,
        shutdown: broadcast::Receiver<()>,
    ) -> Result<Self> {
        let uri = tonic::transport::Uri::try_from(addr)?;
        let client = VectorClient::connect(uri).await?;
        Ok(Self {
            client,
            rx,
            shutdown,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let request = tonic::Request::new(vector::HealthCheckRequest {});

        let _ = &self.client.health_check(request).await?;

        loop {
            tokio::select! {
                result = self.rx.recv() => {
                    if let Ok(events) = result {
                        let events: Vec<EventWrapper> = events
                            .iter()
                            .map(|e| EventWrapper {
                                event: Some(VectorEvent::Log(e.into())),
                            })
                            .collect();
                        let request = tonic::Request::new(vector::PushEventsRequest { events });
                        let _ = &self.client.push_events(request).await?;
                    } else {
                        log::info!("Vector client channel closed");
                        break;
                    }
                },
                _ = self.shutdown.recv() => {
                    info!("Vector client received shutdown signal");
                    break;
                }
            }
        }
        Ok(())
    }
}
