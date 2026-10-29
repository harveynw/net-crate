use super::*;
use std::{net::Ipv4Addr, time::Duration};
use tokio::time::timeout;
use webrtc::{
    data_channel::DataChannel,
    peer_connection::{
        PeerConnectionEventHandler, RTCPeerConnectionIceEvent, RTCSessionDescription,
    },
};

struct RemoteHandler {
    candidates: mpsc::UnboundedSender<String>,
    channels: mpsc::UnboundedSender<Arc<dyn DataChannel>>,
    events: mpsc::UnboundedSender<DataChannelEvent>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for RemoteHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let message = serde_json::json!({
            "type": "ice",
            "candidate": event.candidate.to_json().unwrap(),
        });
        let _ = self.candidates.send(message.to_string());
    }

    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        let _ = self.channels.send(channel.clone());
        let events = self.events.clone();
        tokio::spawn(async move {
            while let Some(event) = channel.poll().await {
                if events.send(event).is_err() {
                    break;
                }
            }
        });
    }
}

#[tokio::test]
async fn actor_negotiates_exchanges_binary_messages_and_closes() {
    timeout(Duration::from_secs(15), async {
        let config = RtcConfig::new(Ipv4Addr::LOCALHOST.into());
        let (candidate_tx, mut candidates) = mpsc::unbounded_channel();
        let (channel_tx, mut channels) = mpsc::unbounded_channel();
        let (event_tx, mut remote_events) = mpsc::unbounded_channel();
        let remote = webrtc::peer_connection::PeerConnectionBuilder::new()
            .with_configuration(webrtc::peer_connection::RTCConfigurationBuilder::default()
                .with_ice_servers(vec![]).build())
            .with_udp_addrs(vec![std::net::SocketAddr::from(([127, 0, 0, 1], 0))])
            .with_runtime(Arc::new(webrtc::runtime::TokioRuntime))
            .with_handler(Arc::new(RemoteHandler {
            candidates: candidate_tx,
            channels: channel_tx,
            events: event_tx,
        })).build().await.unwrap();

        let (emit, mut events) = mpsc::channel(1024);
        let mut actor = RTCHandle::new(emit, config);
        let RTCEvent::EmitSignallingMessage(offer) = events.recv().await.unwrap() else {
            panic!("The offer must precede every candidate");
        };
        let offer: serde_json::Value = serde_json::from_str(&offer).unwrap();
        assert!(offer["sdp"].as_str().unwrap().lines().any(|line| line == "a=ice-lite"));
        remote.set_remote_description(RTCSessionDescription::offer(
            offer["sdp"].as_str().unwrap().to_owned(),
        ).unwrap()).await.unwrap();
        let answer = remote.create_answer(None).await.unwrap();
        assert!(!answer.sdp.lines().any(|line| line == "a=ice-lite"));
        remote.set_local_description(answer.clone()).await.unwrap();

        // Exercise the case where an ICE candidate reaches the actor before the SDP answer.
        actor.receive_signalling_message(candidates.recv().await.unwrap());
        actor.receive_signalling_message(serde_json::json!({
            "type": "answer", "sdp": answer.sdp,
        }).to_string());

        let mut remote_channel: Option<Arc<dyn DataChannel>> = None;
        let (mut server_open, mut remote_open) = (false, false);
        let (mut server_received, mut remote_received) = (false, false);
        let mut sent = false;
        loop {
            if server_open && remote_open && !sent {
                if let Some(channel) = &remote_channel {
                    assert!(!channel.ordered().await.unwrap());
                    channel.send(BytesMut::from(&b"from-client"[..])).await.unwrap();
                    actor.send_message(Bytes::from_static(b"from-server"));
                    sent = true;
                }
            }
            if server_received && remote_received {
                break;
            }
            select! {
                event = events.recv() => match event.unwrap() {
                    RTCEvent::EmitSignallingMessage(message) => {
                        let message: serde_json::Value = serde_json::from_str(&message).unwrap();
                        let candidate = serde_json::from_value(message["candidate"].clone()).unwrap();
                        remote.add_ice_candidate(candidate).await.unwrap();
                    }
                    RTCEvent::Opened => server_open = true,
                    RTCEvent::ApplicationMessageReceived(message) => {
                        assert_eq!(message.as_ref(), b"from-client");
                        server_received = true;
                    }
                    RTCEvent::Closed => panic!("Connection closed before exchanging messages"),
                },
                Some(message) = candidates.recv() => actor.receive_signalling_message(message),
                Some(channel) = channels.recv() => remote_channel = Some(channel),
                Some(event) = remote_events.recv() => match event {
                    DataChannelEvent::OnOpen => remote_open = true,
                    DataChannelEvent::OnMessage(message) => {
                        assert!(!message.is_string);
                        assert_eq!(message.data.as_ref(), b"from-server");
                        remote_received = true;
                    }
                    DataChannelEvent::OnClose => panic!("Remote channel closed early"),
                    _ => {},
                },
            }
        }

        // Dropping the actor handle must shut down the peer, rather than leak its driver.
        drop(actor);
        loop {
            match events.recv().await.expect("Actor must emit Closed") {
                RTCEvent::Closed => break,
                _ => continue,
            }
        }
        assert!(events.recv().await.is_none());
        remote.close().await.unwrap();
    }).await.expect("WebRTC negotiation and shutdown should finish");
}
