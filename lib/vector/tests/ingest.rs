//! End-to-end input test. A Vector gRPC client sends a batch of events. The
//! server gives the batch to its single mpsc consumer. The server does not copy
//! the batch and does not use an Arc.

use std::sync::Arc;
use std::time::Duration;

use striem_common::{SysMessage, event::Event};
use striem_vector::{Client, Server};
use tokio::sync::broadcast;
use tokio_stream::wrappers::TcpListenerStream;

#[tokio::test]
async fn pushed_events_reach_the_consumer() {
    // Bind the port first. The port then accepts connections before the client
    // connects. This prevents a race condition.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mut server = Server::new();
    let mut consumer = server.subscribe().expect("receiver");
    let service = server.service().expect("service");

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service)
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Send events through the real Vector client (findings -> push_events).
    let findings = broadcast::channel::<Arc<Vec<Event>>>(4).0;
    let sys = broadcast::channel::<SysMessage>(1).0;
    let mut client = Client::new(
        &format!("http://{}", addr),
        findings.subscribe(),
        sys.subscribe(),
    )
    .await
    .expect("client connects");
    tokio::spawn(async move { client.run().await.unwrap() });

    let event = Event::from(serde_json::json!({ "hello": "world" }));
    findings.send(Arc::new(vec![event])).unwrap();

    let batch = tokio::time::timeout(Duration::from_secs(5), consumer.recv())
        .await
        .expect("consumer received a batch before timeout")
        .expect("channel open");

    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].data.get("hello").and_then(|v| v.as_str()), Some("world"));

    // Keep the senders alive until the assertion is complete.
    drop(findings);
    drop(sys);
}
