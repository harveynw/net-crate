use signal::handle_signalling_message;
use std::sync::{Arc, LazyLock};

use tokio::sync::mpsc;
use webrtc::{api::{APIBuilder, API}, data_channel::RTCDataChannel, peer_connection::RTCPeerConnection};

/// WebRTC server instance
static API: LazyLock<API> = LazyLock::new(|| {
    APIBuilder::new().build()
    /*
    let mut s = SettingEngine::default();

    let socket = Handle::current().block_on(async {
        tokio::net::UdpSocket::bind("0.0.0.0:3001").await.unwrap()
    });
    s.set_udp_network(UDPNetwork::Muxed(UDPMuxDefault::new(
        UDPMuxParams::new(socket)
    )));

    APIBuilder::new()
        .with_setting_engine(s)
        .build()
    */
});

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

pub struct RTCHandle {
    sender: mpsc::Sender<RTCHandleMessage>
}

impl RTCHandle {
    pub fn new(emit: mpsc::Sender<RTCEvent>) -> Self {
        let (sender, mut receiver) = mpsc::channel(1024);

        tokio::spawn(async move {
            // Create a new RTCPeerConnection
            let peer_connection = Arc::new(API.new_peer_connection(Default::default()).await.expect("Should have been created."));

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