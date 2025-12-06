use std::net::SocketAddr;

mod convert;
mod server;

#[allow(unused)]
pub mod event {
    include!(concat!(env!("OUT_DIR"), "/proto/event.rs"));
}

#[allow(unused)]
pub mod vector {
    include!(concat!(env!("OUT_DIR"), "/proto/vector.rs"));
}

use tokio::main;

use crate::server::Server;

#[main]
async fn main() -> anyhow::Result<()> {
    let addr: SocketAddr = "0.0.0.0:50051".parse()?;
    let mut server = Server::new();
    let mut rx = server.subscribe().await?;

    tokio::spawn(async move {
        loop {
            let events = rx.recv().await;
            match events {
                Ok(events) => {
                    println!("Received {} events", events.len());
                }
                Err(e) => {
                    println!("Error receiving events: {}", e);
                }
            }
        }
    });

    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        println!("Shutting down server...");
        shutdown_tx.send(()).unwrap();
    });

    server.serve(&addr, shutdown_rx).await?;

    Ok(())
}
