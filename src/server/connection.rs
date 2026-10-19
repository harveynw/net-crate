//! Job
//! - Maintain a websocket connection
//! - Hold handle to webrtc connection actor
//! - Emit notification if failure of any of the above
//! - Handle sending messages
//! 
//! Some subtleties:
//! - Uses binary message types for application messages
//! - Uses utf8 text message types for webrtc signalling (ICE candidates etc.)

use bytes::Bytes;
use log::{info, warn};
use tokio_tungstenite::WebSocketStream;
use tokio::{net::TcpStream, select, sync::mpsc};
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;

use crate::{event::Identifier, server::webrtc::RTCHandle};

use super::webrtc::{RTCEvent, RtcConfig};

/// Events emitted by the connection actor
#[derive(Debug)]
pub enum ConnectionEvent {
    /// Signals both websocket + webrtc connection is ready to send/receive messages.
    ConnectionEstablished, 
    ConnectionTerminated,
    MessageReceived(Bytes),
}

/// Messages accepted by the connection actor
enum ConnectionHandleMessage {
    SendReliable(Bytes),
    SendUnreliable(Bytes),
    ReceiveApplicationMessage(Bytes),
    ReceiveSignalling(String),
    ReceiveWebSocketClose,
    HandleWebRTCEvent(RTCEvent)
}

/// Handle to the connection
pub struct ConnectionHandle {
    sender: mpsc::Sender<ConnectionHandleMessage>
}

impl ConnectionHandle {

    /// Spawn a connection actor to service a TcpStream and establish a WebRTC data channel.
    pub fn new(id: Identifier, emit: mpsc::Sender<(Identifier, ConnectionEvent)>, stream: TcpStream, rtc_config: RtcConfig) -> Self {
        let (sender, mut receiver) = mpsc::channel(1024);

        tokio::spawn(async move {
            // Perform handshake
            let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                Ok(stream) => stream,
                Err(err) => {
                    warn!("Failed websocket handshake: {}", err);
                    let _ = emit.send((id, ConnectionEvent::ConnectionTerminated)).await;
                    return;
                },
            };

            // Split ownership of sender and receiver
            let (ws_sink, mut ws_stream) = ws_stream.split();

            // Create channel for webrtc actor
            let (sender_rtc, mut receiver_rtc) = mpsc::channel(1024);

            // Create webrtc actor
            let actor_rtc = RTCHandle::new(sender_rtc, rtc_config);

            // Create actor
            let mut actor = Actor::new(id, emit, ws_sink, actor_rtc);

            info!("Began servicing connection with id={}", id);

            // Event loop
            loop {
                select! {
                    message_ws = ws_stream.next() => {
                        match message_ws {
                            None => {
                                actor.handle_message(ConnectionHandleMessage::ReceiveWebSocketClose);
                                break;
                            }
                            Some(Ok(message)) => {
                                info!("Stream gave {:?}", message);
                                match message {
                                    WebSocketMessage::Binary(bytes) => actor.handle_message(ConnectionHandleMessage::ReceiveApplicationMessage(bytes)),
                                    WebSocketMessage::Text(text) => actor.handle_message(ConnectionHandleMessage::ReceiveSignalling(text.to_string())),
                                    WebSocketMessage::Close(_) => {
                                        info!("Received web socket close frame from client");
                                        actor.handle_message(ConnectionHandleMessage::ReceiveWebSocketClose);
                                        break
                                    },
                                    _ => {} // Ping-pong ignored
                                }
                            },
                            Some(Err(err)) => {
                                warn!("Websocket stream error: {}", err);
                                actor.handle_message(ConnectionHandleMessage::ReceiveWebSocketClose);
                                break
                            },
                        }
                    },
                    Some(event) = receiver_rtc.recv() => {
                        info!("Got RTCEvent: {:?}", event);
                        let closed = matches!(event, RTCEvent::Closed);
                        actor.handle_message(ConnectionHandleMessage::HandleWebRTCEvent(event));
                        if closed {
                            break;
                        }
                    }
                    message_handle = receiver.recv() => {
                        match message_handle {
                            Some(message) => actor.handle_message(message),
                            None => break,
                        }
                    },
                    else => {
                        warn!("Unexpected branch!");
                        break;
                    }
                }
            }

            info!("Finished servicing connection with id={}", id);
        });

        Self { sender }
    }

    pub fn send_reliable(&mut self, bytes: Bytes) {
        self.sender.try_send(ConnectionHandleMessage::SendReliable(bytes)).expect("Actor should be alive.");
    }

    pub fn send_unreliable(&mut self, bytes: Bytes) {
        self.sender.try_send(ConnectionHandleMessage::SendUnreliable(bytes)).expect("Actor should be alive.");
    }
}

type WsSink = SplitSink<WebSocketStream<TcpStream>, tokio_tungstenite::tungstenite::Message>;

struct Actor {
    /// Unique identifier for this peer.
    id: Identifier,
    /// Channel that provides a way to communicate with higher level actors that an event has occured on this connection. 
    emit: mpsc::Sender<(Identifier, ConnectionEvent)>,
    /// Channel that acts as a sink for sending messages to the other peer across the websocket connection. 
    ws_send: mpsc::Sender<SocketMessage>,
    /// Handle for the actor which services the corresponding WebRTC connection for this peer.
    rtc: RTCHandle,
}

impl Actor {
    pub fn new(id: Identifier, emit: mpsc::Sender<(Identifier, ConnectionEvent)>, sink: WsSink, rtc: RTCHandle) -> Self {
        let ws_send = start_sink_task(id, sink);

        Self { id, emit, ws_send, rtc }
    }

    pub fn handle_message(&mut self, message: ConnectionHandleMessage) {
        match message {
            ConnectionHandleMessage::SendReliable(bytes) => {
                self.ws_send.try_send(SocketMessage::Data(bytes)).expect("Sender task should be alive.");
            },
            ConnectionHandleMessage::SendUnreliable(bytes) => {
                self.rtc.send_message(bytes)
            },
            ConnectionHandleMessage::ReceiveSignalling(message) => {
                self.rtc.receive_signalling_message(message);
            },
            ConnectionHandleMessage::ReceiveApplicationMessage(bytes) => {
                self.emit.try_send((self.id, ConnectionEvent::MessageReceived(bytes))).expect("Parent actor should be alive.");
            },
            ConnectionHandleMessage::ReceiveWebSocketClose => {
                self.emit.try_send((self.id, ConnectionEvent::ConnectionTerminated)).expect("Parent actor should be alive.");
            }
            ConnectionHandleMessage::HandleWebRTCEvent(event) => {
                self.handle_webrtc_event(event);
            }
        }
    }

    pub fn handle_webrtc_event(&mut self, event: RTCEvent) {
        match event {
            RTCEvent::Opened => {
                self.emit.try_send((self.id, ConnectionEvent::ConnectionEstablished)).expect("Parent actor should be alive.");
            },
            RTCEvent::Closed => {
                self.emit.try_send((self.id, ConnectionEvent::ConnectionTerminated)).expect("Parent actor should be alive.");
            },
            RTCEvent::ApplicationMessageReceived(bytes) => {
                self.emit.try_send((self.id, ConnectionEvent::MessageReceived(bytes))).expect("Parent actor should be alive.");
            },
            RTCEvent::EmitSignallingMessage(message) => {
                self.ws_send.try_send(SocketMessage::Signalling(message)).expect("Sender task should be alive.");
            },
        }
    }
}

/// Type representing everything we need to send over the websocket connection
enum SocketMessage {
    /// Arbitrary bytes 
    Data(Bytes),
    /// WebRTC signalling message 
    Signalling(String),
}

/// Spawns a task whose job is to forward messages into the provided sink, which is only possible in an async context.
/// 
/// Task finishes when all senders are dropped.
fn start_sink_task(id: Identifier, mut sink: WsSink) -> mpsc::Sender<SocketMessage> {
    let (sender, mut receiver) = mpsc::channel::<SocketMessage>(1024);

    tokio::spawn(async move {
        while let Some(message) = receiver.recv().await {
            // Here we make the distinction between how application data and webrtc signalling is sent.
            let result = match message {
                SocketMessage::Data(bytes) => sink.send(WebSocketMessage::Binary(bytes::Bytes::copy_from_slice(&bytes))).await,
                SocketMessage::Signalling(message) => sink.send(WebSocketMessage::text(message)).await,
            };
                
            // Report any problems, but don't panic.
            if let Err(err) = result {
                warn!("Error sending to websocket sink : ({}), {:?}", id, err);
            }
        }
    });

    sender
}


