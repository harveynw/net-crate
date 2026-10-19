pub use config::RtcConfig;
use bytes::{Bytes, BytesMut};
use log::{info, warn};
use std::sync::Arc;
use tokio::{select, sync::mpsc};
use webrtc::{
    data_channel::{DataChannelEvent, RTCDataChannelInit},
    error::Result,
    peer_connection::PeerConnection,
};

use handlers::{PeerEvent, PeerHandler};

#[derive(Debug)]
pub enum RTCEvent {
    Opened,
    Closed,
    ApplicationMessageReceived(Bytes),
    EmitSignallingMessage(String),
}

enum RTCHandleMessage {
    Send(Bytes),
    ReceiveSignalling(String),
}

/// Shared WebRTC settings and peer connection construction.
mod config;
/// Forwards peer connection events to the connection actor.
mod handlers;
/// Handles serialization of ICE/SDP messages.
mod signal;

pub struct RTCHandle {
    sender: mpsc::Sender<RTCHandleMessage>,
}

impl RTCHandle {
    pub fn new(emit: mpsc::Sender<RTCEvent>, config: RtcConfig) -> Self {
        let (sender, receiver) = mpsc::channel(1024);

        tokio::spawn(async move {
            if let Err(error) = run_connection(&config, &emit, receiver).await {
                warn!("WebRTC connection failed: {error}");
            }
            let _ = emit.send(RTCEvent::Closed).await;
        });

        Self { sender }
    }

    pub fn send_message(&mut self, message: Bytes) {
        if let Err(error) = self.sender.try_send(RTCHandleMessage::Send(message)) {
            warn!("Could not queue WebRTC message: {error}");
        }
    }

    pub fn receive_signalling_message(&mut self, message: String) {
        if let Err(error) = self
            .sender
            .try_send(RTCHandleMessage::ReceiveSignalling(message))
        {
            warn!("Could not queue WebRTC signalling: {error}");
        }
    }
}

async fn run_connection(
    config: &RtcConfig,
    emit: &mpsc::Sender<RTCEvent>,
    receiver: mpsc::Receiver<RTCHandleMessage>,
) -> Result<()> {
    // The handler never awaits the application, so it cannot stall the RTC driver.
    let (peer_emit, peer_events) = mpsc::unbounded_channel();
    let handler = Arc::new(PeerHandler::new(peer_emit));
    let peer = config.create_peer_connection(handler).await?;

    let result = run_peer(peer.as_ref(), emit, receiver, peer_events).await;
    if let Err(error) = peer.close().await {
        warn!("Could not close WebRTC connection: {error}");
    }
    result
}

async fn run_peer(
    peer: &dyn PeerConnection,
    emit: &mpsc::Sender<RTCEvent>,
    mut receiver: mpsc::Receiver<RTCHandleMessage>,
    mut peer_events: mpsc::UnboundedReceiver<PeerEvent>,
) -> Result<()> {
    let options = RTCDataChannelInit {
        ordered: false,
        max_retransmits: Some(0),
        ..Default::default()
    };
    let data_channel = peer.create_data_channel("game", Some(options)).await?;
    let offer = signal::generate_sdp_offer_message(peer).await?;
    if emit
        .send(RTCEvent::EmitSignallingMessage(offer))
        .await
        .is_err()
    {
        return Ok(());
    }

    // Only drain local candidates after the offer is queued on the same emitter.
    let mut signalling = signal::SignallingState::default();
    loop {
        select! {
            message = receiver.recv() => {
                match message {
                    Some(RTCHandleMessage::Send(bytes)) => {
                        data_channel.send(BytesMut::from(bytes.as_ref())).await?;
                    }
                    Some(RTCHandleMessage::ReceiveSignalling(message)) => {
                        signalling.handle_message(peer, &message).await;
                    }
                    None => break,
                }
            }
            event = peer_events.recv() => {
                match event {
                    Some(PeerEvent::IceCandidate(candidate)) => {
                        match signal::generate_ice_candidate_message(candidate) {
                            Ok(message) => {
                                if emit.send(RTCEvent::EmitSignallingMessage(message)).await.is_err() {
                                    break;
                                }
                            }
                            Err(error) => warn!("Could not serialize ICE candidate: {error}"),
                        }
                    }
                    Some(PeerEvent::Closed) | None => break,
                }
            }
            event = data_channel.poll() => {
                let event = match event {
                    Some(DataChannelEvent::OnOpen) => {
                        info!("Data channel open");
                        RTCEvent::Opened
                    }
                    Some(DataChannelEvent::OnMessage(message)) => {
                        RTCEvent::ApplicationMessageReceived(message.data.freeze())
                    }
                    Some(DataChannelEvent::OnClose) | None => break,
                    _ => continue,
                };
                if emit.send(event).await.is_err() {
                    break;
                }
            }
            _ = emit.closed() => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
