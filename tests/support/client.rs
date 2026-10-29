use std::{collections::VecDeque, future::pending, io::ErrorKind, net::SocketAddr, sync::Arc};

use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::{
    net::TcpStream,
    select,
    sync::mpsc,
    time::{sleep, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error as WsError, Message},
};
use webrtc::{
    data_channel::{DataChannel, DataChannelEvent},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceCandidateInit,
        RTCPeerConnectionIceEvent, RTCSessionDescription,
    },
    runtime::TokioRuntime,
};

use super::{IO_TIMEOUT, TestResult};

enum PeerEvent {
    Candidate(TestResult<RTCIceCandidateInit>),
    Channel(Arc<dyn DataChannel>),
}

struct Handler(mpsc::UnboundedSender<PeerEvent>);

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let candidate = event.candidate.to_json().map_err(Into::into);
        let _ = self.0.send(PeerEvent::Candidate(candidate));
    }

    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        let _ = self.0.send(PeerEvent::Channel(channel));
    }
}

#[derive(Deserialize)]
struct ServerSignal {
    sdp: Option<String>,
    candidate: Option<RTCIceCandidateInit>,
}

/// A browser-like client: WebSocket binary frames carry reliable data, text
/// frames carry signalling, and the server-created data channel carries UDP data.
/// Calling connect/receive drives I/O; the helper spawns no additional tasks.
pub struct Client {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    peer: Arc<dyn PeerConnection>,
    events: mpsc::UnboundedReceiver<PeerEvent>,
    channel: Option<Arc<dyn DataChannel>>,
    channel_open: bool,
    reliable: VecDeque<Bytes>,
    unreliable: VecDeque<Bytes>,
}

impl Client {
    pub async fn connect(address: SocketAddr) -> TestResult<Self> {
        let socket = timeout(IO_TIMEOUT, async {
            loop {
                match connect_async(format!("ws://{address}")).await {
                    Ok((socket, _)) => return Ok(socket),
                    Err(WsError::Io(error)) if error.kind() == ErrorKind::ConnectionRefused => {
                        sleep(std::time::Duration::from_millis(10)).await;
                    }
                    Err(error) => return Err(error),
                }
            }
        })
        .await
        .map_err(|_| "Server did not start listening")??;

        let (emit, events) = mpsc::unbounded_channel();
        let peer = PeerConnectionBuilder::new()
            .with_udp_addrs(vec![SocketAddr::new(address.ip(), 0)])
            .with_runtime(Arc::new(TokioRuntime))
            .with_handler(Arc::new(Handler(emit)))
            .build()
            .await?;
        let mut client = Self {
            socket,
            peer: Arc::new(peer),
            events,
            channel: None,
            channel_open: false,
            reliable: VecDeque::new(),
            unreliable: VecDeque::new(),
        };
        let result = timeout(IO_TIMEOUT, async {
            while !client.channel_open {
                client.drive_once().await?;
            }
            TestResult::Ok(())
        })
        .await;
        match result {
            Ok(Ok(())) => Ok(client),
            error => {
                let _ = client.close().await;
                match error {
                    Ok(Err(error)) => Err(error),
                    Err(_) => Err("Timed out negotiating WebRTC".into()),
                    _ => unreachable!(),
                }
            }
        }
    }

    pub async fn send_reliable(&mut self, bytes: Bytes) -> TestResult {
        timeout(IO_TIMEOUT, self.socket.send(Message::Binary(bytes))).await??;
        Ok(())
    }

    pub async fn send_unreliable(&self, bytes: &[u8]) -> TestResult {
        let channel = self.channel.as_ref().ok_or("Data channel has not opened")?;
        timeout(IO_TIMEOUT, channel.send(BytesMut::from(bytes))).await??;
        Ok(())
    }

    pub async fn receive_reliable(&mut self) -> TestResult<Bytes> {
        timeout(IO_TIMEOUT, async {
            loop {
                if let Some(bytes) = self.reliable.pop_front() {
                    return Ok(bytes);
                }
                self.drive_once().await?;
            }
        })
        .await
        .map_err(|_| "Timed out receiving WebSocket data")?
    }

    pub async fn receive_unreliable(&mut self) -> TestResult<Bytes> {
        timeout(IO_TIMEOUT, async {
            loop {
                if let Some(bytes) = self.unreliable.pop_front() {
                    return Ok(bytes);
                }
                self.drive_once().await?;
            }
        })
        .await
        .map_err(|_| "Timed out receiving data-channel data")?
    }

    pub async fn ordered(&self) -> TestResult<bool> {
        Ok(self
            .channel
            .as_ref()
            .ok_or("Data channel has not opened")?
            .ordered()
            .await?)
    }

    pub async fn close(mut self) -> TestResult {
        // Always close the peer, even if closing the WebSocket fails.
        let socket_result = timeout(IO_TIMEOUT, self.socket.close(None)).await;
        let peer_result = timeout(IO_TIMEOUT, self.peer.close()).await;
        peer_result??;
        socket_result??;
        Ok(())
    }

    async fn drive_once(&mut self) -> TestResult {
        let channel = self.channel.clone();
        select! {
            message = self.socket.next() => match message.ok_or("WebSocket ended")?? {
                Message::Binary(bytes) => self.reliable.push_back(bytes),
                Message::Text(text) => {
                    let signal: ServerSignal = serde_json::from_str(&text)?;
                    if let Some(sdp) = signal.sdp {
                        self.peer.set_remote_description(RTCSessionDescription::offer(sdp)?).await?;
                        let answer = self.peer.create_answer(None).await?;
                        self.peer.set_local_description(answer.clone()).await?;
                        self.socket.send(Message::text(serde_json::json!({
                            "type": "answer", "sdp": answer.sdp,
                        }).to_string())).await?;
                    } else if let Some(candidate) = signal.candidate {
                        let fields: Vec<_> = candidate.candidate.split_whitespace().collect();
                        assert_eq!(fields[4], "127.0.0.1");
                        assert_eq!(fields[6], "typ");
                        assert_eq!(fields[7], "host");
                        assert!(fields[2].eq_ignore_ascii_case("udp"));
                        self.peer.add_ice_candidate(candidate).await?;
                    } else {
                        return Err("Unrecognized server signalling message".into());
                    }
                }
                Message::Close(_) => return Err("Server closed the WebSocket".into()),
                _ => {},
            },
            event = self.events.recv() => match event.ok_or("Peer event stream ended")? {
                PeerEvent::Candidate(candidate) => {
                    self.socket.send(Message::text(serde_json::json!({
                        "type": "ice", "candidate": candidate?,
                    }).to_string())).await?;
                }
                PeerEvent::Channel(channel) => {
                    assert_eq!(channel.label().await?, "game");
                    self.channel = Some(channel);
                }
            },
            event = async move {
                match channel {
                    Some(channel) => channel.poll().await,
                    None => pending().await,
                }
            } => match event.ok_or("Data-channel event stream ended")? {
                DataChannelEvent::OnOpen => self.channel_open = true,
                DataChannelEvent::OnMessage(message) => {
                    assert!(!message.is_string, "Expected a binary data-channel message");
                    self.unreliable.push_back(message.data.freeze());
                }
                DataChannelEvent::OnClose => return Err("Server closed the data channel".into()),
                _ => {},
            },
        }
        Ok(())
    }
}
