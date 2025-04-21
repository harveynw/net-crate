use log::info;
use std::sync::Arc;
use tokio::sync::mpsc;
use webrtc::{data_channel::RTCDataChannel, peer_connection::RTCPeerConnection};

use super::{signal, RTCEvent};

/// Configures the event handlers of an RTCDataChannel to log and send appropriate signals down the provided 'emit' channel.
pub fn configure_data_channel(data_channel: &Arc<RTCDataChannel>, emit: mpsc::Sender<RTCEvent>) {
    // Notify parent actor that connection is open
    {
        let emit = emit.clone();
        data_channel.on_open(Box::new(move || {
            info!("Data channel open");
            emit.try_send(RTCEvent::Opened).expect("Parent actor should be alive.");
            Box::pin(async {})
        }));
    }
    
    // Notify parent actor that connection is closed
    {
        let emit = emit.clone();
        data_channel.on_close(Box::new(move || {
            info!("Data channel close");
            let _ = emit.try_send(RTCEvent::Closed);
            Box::pin(async {})
        }));
    }

    // Forward received messages to parent actor
    {
        let emit = emit.clone();
        data_channel.on_message(Box::new(move |msg| {
            info!("Data channel message");
            emit.try_send(RTCEvent::MessageReceived(msg.data.to_vec())).expect("Parent actor should be alive.");
            Box::pin(async {})
        }));
    }
}

/// Configures the event handlers of an RTCPeerConnection to log and send appropriate signals down the provided 'emit' channel.
pub fn configure_peer_connection(peer_connection: &RTCPeerConnection, emit: mpsc::Sender<RTCEvent>) {
    // Handle ICE candidate challenges by sending them by another channel (actor emits them)
    peer_connection.on_ice_candidate(Box::new(move |candidate| {
        if let Some(candidate) = candidate {
            emit.try_send(
                RTCEvent::EmitSignallingMessage(signal::generate_ice_candidate_message(candidate))
            ).expect("Parent actor should be alive.");
        }
        Box::pin(async {})
    }));
}