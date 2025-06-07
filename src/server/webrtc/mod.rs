pub use api::RtcApiHandle;
use signal::handle_signalling_message;
use std::sync::Arc;

use tokio::sync::mpsc;
use webrtc::{data_channel::RTCDataChannel, peer_connection::RTCPeerConnection};


#[derive(Debug)]
pub enum RTCEvent {
    Opened,
    Closed,
    ApplicationMessageReceived(Vec<u8>),
    EmitSignallingMessage(String)
}

enum RTCHandleMessage {
    Send(Vec<u8>),
    ReceiveSignalling(String)
}

/// Handles serialization of ICE/SDP messages
mod signal;
/// Configures the RTCPeerConnection
mod handlers;
/// Handles the WebRTC API, which initialises new data channels over UDP
mod api;

pub struct RTCHandle {
    sender: mpsc::Sender<RTCHandleMessage>
}

impl RTCHandle {
    pub fn new(emit: mpsc::Sender<RTCEvent>, mut api: RtcApiHandle) -> Self {
        let (sender, mut receiver) = mpsc::channel(1024);

        tokio::spawn(async move {
            // Create a new RTCPeerConnection
            let peer_connection = api.new_peer_connection().await;

            // Create a data channel (only on the initiator side)
            let data_channel = peer_connection.create_data_channel("game", None).await.expect("Should have been created.");

            // Setup handlers 
            handlers::configure_data_channel(&data_channel, emit.clone());
            handlers::configure_peer_connection(&peer_connection, emit.clone());
                        
            // Create and send SDP offer
            emit.send(RTCEvent::EmitSignallingMessage(signal::generate_sdp_offer_message(&peer_connection).await)).await.expect("Parent actor should be alive");

            // Task to send messages via the data channel 
            let sender_data_channel = start_send_task(data_channel);
            
            // Create actor 
            let mut actor = Actor {
                sender_data_channel,
                peer_connection
            };

            // Event loop
            while let Some(message) = receiver.recv().await {
                actor.handle_message(message);
            }
        });

        Self { sender }
    }

    pub fn send_message(&mut self, message: Vec<u8>) {
        self.sender.try_send(RTCHandleMessage::Send(message)).expect("Actor should be alive");
    }

    pub fn receive_signalling_message(&mut self, message: String) {
        self.sender.try_send(RTCHandleMessage::ReceiveSignalling(message)).expect("Actor should be alive");
    }
}

struct Actor {
    sender_data_channel: mpsc::Sender<Vec<u8>>,
    peer_connection: Arc<RTCPeerConnection>,
}

impl Actor {
    pub fn handle_message(&mut self, message: RTCHandleMessage) {
        match message {
            RTCHandleMessage::Send(bytes) => {
                self.sender_data_channel.try_send(bytes).expect("Send task should be alive");
            },
            RTCHandleMessage::ReceiveSignalling(message) => {
                let peer_connection = Arc::clone(&self.peer_connection);

                tokio::spawn(async move {
                    handle_signalling_message(peer_connection, message).await;
                });
            },
        }
    }
}

/// Spawns a task whose job is to send messages through the provided datachannel, which is only possible in an async context.
/// 
/// Task finishes when returns when all senders are dropped.
fn start_send_task(data_channel: Arc<RTCDataChannel>) -> mpsc::Sender<Vec<u8>> {
    let (sender, mut receiver) = mpsc::channel::<Vec<u8>>(1024);

    tokio::spawn(async move {
        while let Some(message) = receiver.recv().await {
            data_channel.send(&bytes::Bytes::copy_from_slice(&message)).await.expect("Should have sent");
        }
    });

    sender
}