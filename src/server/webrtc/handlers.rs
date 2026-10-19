use tokio::sync::mpsc;
use webrtc::peer_connection::{
    PeerConnectionEventHandler, RTCIceCandidate, RTCPeerConnectionIceEvent, RTCPeerConnectionState,
};

/// Events consumed by the connection actor, after its SDP offer has been queued.
pub enum PeerEvent {
    IceCandidate(RTCIceCandidate),
    Closed,
}

pub struct PeerHandler {
    emit: mpsc::UnboundedSender<PeerEvent>,
}

impl PeerHandler {
    pub fn new(emit: mpsc::UnboundedSender<PeerEvent>) -> Self {
        Self { emit }
    }
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for PeerHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        let _ = self.emit.send(PeerEvent::IceCandidate(event.candidate));
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        if matches!(
            state,
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
        ) {
            let _ = self.emit.send(PeerEvent::Closed);
        }
    }
}
