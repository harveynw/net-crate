mod support;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use net::Event;
use support::{Client, IO_TIMEOUT, TestResult, TestServer};
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as WsError, Message, error::ProtocolError},
};

/*
    A handful of tests for verifying 'in-between' behaviour when sending data (when connections are not ready or closed).
*/

#[tokio::test]
async fn sends_to_unknown_and_closed_connections_no_panic() -> TestResult {
    let mut server = TestServer::start()?;
    let mut client = Client::connect(server.address).await?;
    let id = server.opened().await?;
    let closing_client = Client::connect(server.address).await?;
    let closed_id = server.opened().await?;
    assert_ne!(id, closed_id);
    closing_client.close().await?;
    server.closed(closed_id).await?;

    for unavailable_id in [u32::MAX, closed_id] {
        server
            .send_reliable(unavailable_id, Bytes::from_static(b"drop reliable"))
            .await?;
        server
            .send_unreliable(unavailable_id, Bytes::from_static(b"drop unreliable"))
            .await?;

        // Processing these sends must not terminate the server actor or affect
        // an existing ready connection, on either transport.
        client
            .send_reliable(Bytes::from_static(b"reliable request"))
            .await?;
        server.received(id, b"reliable request").await?;
        server
            .send_reliable(id, Bytes::from_static(b"reliable response"))
            .await?;
        assert_eq!(client.receive_reliable().await?, b"reliable response"[..]);
        client.send_unreliable(b"unreliable request").await?;
        server.received(id, b"unreliable request").await?;
        server
            .send_unreliable(id, Bytes::from_static(b"unreliable response"))
            .await?;
        assert_eq!(
            client.receive_unreliable().await?,
            b"unreliable response"[..]
        );
    }

    client.close().await?;
    server.closed(id).await?;
    Ok(())
}

#[tokio::test]
async fn connecting_clients_are_skipped_by_sends_and_broadcasts() -> TestResult {
    let mut server = TestServer::start()?;
    let mut client = Client::connect(server.address).await?;
    let ready_id = server.opened().await?;
    assert!(!client.ordered().await?);

    // Complete only the WebSocket handshake. Without an SDP answer, this peer
    // cannot reach Ready. Its binary marker exposes its ID through the public API.
    let (mut pending, _) = timeout(
        IO_TIMEOUT,
        connect_async(format!("ws://{}", server.address)),
    )
    .await??;
    timeout(
        IO_TIMEOUT,
        pending.send(Message::Binary(Bytes::from_static(b"connecting"))),
    )
    .await??;
    let pending_id = match server.next_event().await? {
        Event::Received(id, bytes) => {
            assert_eq!(bytes, b"connecting"[..]);
            assert_ne!(id, ready_id);
            id
        }
        event => return Err(format!("Expected pending client's marker, got {event:?}").into()),
    };

    server
        .send_reliable(pending_id, Bytes::from_static(b"drop reliable"))
        .await?;
    server
        .send_unreliable(pending_id, Bytes::from_static(b"drop unreliable"))
        .await?;
    server.broadcast(Bytes::from_static(b"ready only")).await?;
    assert_eq!(client.receive_reliable().await?, b"ready only"[..]);

    // Draining through disconnection checks that no queued application frame
    // reached the connecting peer, without relying on a short silence timeout.
    timeout(IO_TIMEOUT, pending.close(None)).await??;
    timeout(IO_TIMEOUT, async {
        while let Some(message) = pending.next().await {
            match message {
                Ok(Message::Binary(_)) => {
                    return Err("Connecting client received application data".into());
                }
                Ok(Message::Close(_))
                | Err(WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake)) => break,
                Err(error) => return Err(error.into()),
                _ => {}
            }
        }
        TestResult::Ok(())
    })
    .await??;
    server.closed(pending_id).await?;

    client.send_unreliable(b"still ready").await?;
    server.received(ready_id, b"still ready").await?;
    server
        .send_unreliable(ready_id, Bytes::from_static(b"still usable"))
        .await?;
    assert_eq!(client.receive_unreliable().await?, b"still usable"[..]);
    client.close().await?;
    server.closed(ready_id).await?;
    Ok(())
}
