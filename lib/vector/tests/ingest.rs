//! End-to-end ingestion test: a Vector gRPC client pushes an event batch, and
//! the server hands it to its single mpsc consumer with no cloning/Arc wrapping.

use std::sync::Arc;
use std::time::Duration;

use striem_common::{SysMessage, event::Event};
use striem_vector::{Client, Server};
use tokio::sync::broadcast;
use tokio_stream::wrappers::TcpListenerStream;

#[tokio::test]
async fn pushed_events_reach_the_consumer() {
    // Bind first so the port is accepting before we connect (avoids a race).
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

    // Drive events through the real Vector client (findings -> push_events).
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

    // Keep senders alive until the assertion completes.
    drop(findings);
    drop(sys);
}
