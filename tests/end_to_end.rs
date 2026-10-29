mod support;

use bytes::Bytes;
use support::{Client, TestResult, TestServer};

#[tokio::test]
async fn reliable_messages_round_trip_over_websocket() -> TestResult {
    let mut server = TestServer::start()?;
    let mut client = Client::connect(server.address).await?;
    let id = server.opened().await?;

    // Include non-UTF-8 bytes to ensure application data travels as binary frames.
    for sequence in 0..3 {
        let request = Bytes::from(vec![0, 255, sequence, 128]);
        client.send_reliable(request.clone()).await?;
        server.received(id, &request).await?;

        let response = Bytes::from(vec![128, sequence, 255, 0]);
        server.send_reliable(id, response.clone()).await?;
        assert_eq!(client.receive_reliable().await?, response);
    }

    client.close().await?;
    server.closed(id).await?;
    Ok(())
}

#[tokio::test]
async fn unreliable_messages_round_trip_over_webrtc() -> TestResult {
    let mut server = TestServer::start()?;
    let mut client = Client::connect(server.address).await?;
    let id = server.opened().await?;
    assert!(!client.ordered().await?, "Game updates should be unordered");

    // This checks delivery on loopback, not a delivery guarantee under packet loss.
    for sequence in 0..3 {
        let request = Bytes::from(vec![0, 255, sequence, 128]);
        client.send_unreliable(&request).await?;
        server.received(id, &request).await?;

        let response = Bytes::from(vec![128, sequence, 255, 0]);
        server.send_unreliable(id, response.clone()).await?;
        assert_eq!(client.receive_unreliable().await?, response);
    }

    client.close().await?;
    server.closed(id).await?;
    Ok(())
}

#[tokio::test]
async fn multiple_clients_route_messages_and_receive_broadcasts() -> TestResult {
    let mut server = TestServer::start()?;
    let mut first = Client::connect(server.address).await?;
    let first_id = server.opened().await?;
    let mut second = Client::connect(server.address).await?;
    let second_id = server.opened().await?;
    assert_ne!(first_id, second_id);

    first
        .send_reliable(Bytes::from_static(b"first request"))
        .await?;
    server.received(first_id, b"first request").await?;
    second.send_unreliable(b"second request").await?;
    server.received(second_id, b"second request").await?;

    // Interleave transports and recipients so crossed connections fail assertions.
    server
        .send_reliable(second_id, Bytes::from_static(b"for second"))
        .await?;
    server
        .send_unreliable(first_id, Bytes::from_static(b"for first"))
        .await?;
    assert_eq!(first.receive_unreliable().await?, b"for first"[..]);
    assert_eq!(second.receive_reliable().await?, b"for second"[..]);

    server
        .broadcast(Bytes::from_static(b"for everyone"))
        .await?;
    assert_eq!(first.receive_reliable().await?, b"for everyone"[..]);
    assert_eq!(second.receive_reliable().await?, b"for everyone"[..]);

    first.close().await?;
    server.closed(first_id).await?;
    // The other connection must remain usable after one client disconnects.
    second
        .send_reliable(Bytes::from_static(b"still connected"))
        .await?;
    server.received(second_id, b"still connected").await?;
    second.close().await?;
    server.closed(second_id).await?;
    Ok(())
}

#[test]
fn invalid_rtc_host_is_rejected_synchronously() {
    assert!(std::panic::catch_unwind(|| net::Server::new("127.0.0.1:0", "0.0.0.0".parse().unwrap())).is_err());
}

#[tokio::test]
async fn wildcard_websocket_bind_uses_explicit_rtc_host() -> TestResult {
    let mut server = TestServer::start_with_bind_ip(std::net::Ipv4Addr::UNSPECIFIED)?;
    let mut client = Client::connect(server.address).await?;
    let id = server.opened().await?;
    client.send_unreliable(b"explicit-host").await?;
    server.received(id, b"explicit-host").await?;
    server.send_unreliable(id, Bytes::from_static(b"response")).await?;
    assert_eq!(client.receive_unreliable().await?.as_ref(), b"response");
    client.close().await?;
    server.closed(id).await?;
    Ok(())
}
