//! Job
//! - Maintain a websocket connection
//! - Hold handle to webrtc connection actor
//! - Emit notification if failure of any of the above
//! - Handle sending messages
//! 
//! Some subtleties:
//! - Uses binary message types for application messages
//! - Uses utf8 text message types for webrtc signalling (ICE candidates etc.)

use log::{info, warn};
use tokio_tungstenite::WebSocketStream;
use tokio::{net::TcpStream, select, sync::mpsc};
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;

use crate::{event::Identifier, server::webrtc::RTCHandle};

use super::webrtc::RTCEvent;

/// Events emitted by the connection actor
#[derive(Debug)]
pub enum ConnectionEvent {
    /// Signals both websocket + webrtc connection is ready to send/receive messages.
    ConnectionEstablished, 
    ConnectionTerminated,
    MessageReceived(Vec<u8>),
}

/// Messages accepted by the connection actor
enum ConnectionHandleMessage {
    SendReliable(Vec<u8>),
    SendUnreliable(Vec<u8>),
    ReceiveApplicationMessage(Vec<u8>),
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
    pub fn new(id: Identifier, emit: mpsc::Sender<(Identifier, ConnectionEvent)>, stream: TcpStream) -> Self {
        let (sender, mut receiver) = mpsc::channel(1024);

        tokio::spawn(async move {
            // Perform handshake
            let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                Ok(stream) => stream,
                Err(err) => {
                    warn!("Failed websocket handshake: {}", err);
                    return;
                },
            };

            // Split ownership of sender and receiver
            let (ws_sink, mut ws_stream) = ws_stream.split();

            // Create channel for webrtc actor
            let (sender_rtc, mut receiver_rtc) = mpsc::channel(1024);

            // Create webrtc actor
            let actor_rtc = RTCHandle::new(sender_rtc);

            // Create actor
            let mut actor = Actor::new(id, emit, ws_sink, actor_rtc);

            info!("Began servicing connection with id={}", id);

            // Event loop
            loop {
                select! {
                    Some(message_ws) = ws_stream.next() => {
                        match message_ws {
                            Ok(message) => {
                                info!("Stream gave {:?}", message);
                                match message {
                                    WebSocketMessage::Binary(bytes) => actor.handle_message(ConnectionHandleMessage::ReceiveApplicationMessage(bytes.to_vec())),
                                    WebSocketMessage::Text(text) => actor.handle_message(ConnectionHandleMessage::ReceiveSignalling(text.to_string())),
                                    WebSocketMessage::Close(_) => {
                                        info!("Received web socket close frame from client");
                                        actor.handle_message(ConnectionHandleMessage::ReceiveWebSocketClose);
                                        break
                                    },
                                    _ => {} // Ping-pong ignored
                                }
                            },
                            Err(err) => {
                                warn!("Websocket stream error: {}", err);
                                actor.handle_message(ConnectionHandleMessage::ReceiveWebSocketClose);
                                break
                            },
                        }
                    },
                    Some(event) = receiver_rtc.recv() => {
                        info!("Got RTCEvent: {:?}", event);
                        actor.handle_message(ConnectionHandleMessage::HandleWebRTCEvent(event));
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

    pub fn send_reliable(&mut self, bytes: Vec<u8>) {
        self.sender.try_send(ConnectionHandleMessage::SendReliable(bytes)).expect("Actor should be alive.");
    }

    pub fn send_unreliable(&mut self, bytes: Vec<u8>) {
        self.sender.try_send(ConnectionHandleMessage::SendUnreliable(bytes)).expect("Actor should be alive.");
    }
}

type WsSink = SplitSink<WebSocketStream<TcpStream>, tokio_tungstenite::tungstenite::Message>;

struct Actor {
    id: Identifier,
    emit: mpsc::Sender<(Identifier, ConnectionEvent)>,
    send: mpsc::Sender<SinkMessage>,
    rtc: RTCHandle,
}

impl Actor {
    pub fn new(id: Identifier, emit: mpsc::Sender<(Identifier, ConnectionEvent)>, sink: WsSink, rtc: RTCHandle) -> Self {
        let send = start_sink_task(sink);

        Self { id, emit, send, rtc }
    }

    pub fn handle_message(&mut self, message: ConnectionHandleMessage) {
        match message {
            ConnectionHandleMessage::SendReliable(bytes) => {
                self.send.try_send(SinkMessage::Data(bytes)).expect("Sender task should be alive.");
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
                self.send.try_send(SinkMessage::Signalling(message)).expect("Sender task should be alive.");
            },
        }
    }
}

enum SinkMessage {
    Data(Vec<u8>),
    Signalling(String),
}

/// Spawns a task whose job is to forward messages into the provided sink, which is only possible in an async context.
/// 
/// Task finishes when returns when all senders are dropped.
fn start_sink_task(mut sink: WsSink) -> mpsc::Sender<SinkMessage> {
    let (sender, mut receiver) = mpsc::channel::<SinkMessage>(1024);

    tokio::spawn(async move {
        while let Some(message) = receiver.recv().await {
            let result = match message {
                SinkMessage::Data(bytes) => sink.send(WebSocketMessage::Binary(bytes::Bytes::copy_from_slice(&bytes))).await,
                SinkMessage::Signalling(message) => sink.send(WebSocketMessage::text(message)).await,
            };
                
            if let Err(err) = result {
                warn!("Error sending to websocket sink : {:?}", err);
            }
        }
    });

    sender
}


