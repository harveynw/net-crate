use std::sync::Arc;

use serde::{Serialize, Deserialize};
use serde_json::Value;
use webrtc::{ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit}, peer_connection::{sdp::session_description::RTCSessionDescription, RTCPeerConnection}};


/// Signalling struct to be forwarded to a client, who can easily inspect the contents in when in JSON form.
#[derive(Serialize, Deserialize)]
struct SignalingMessage {
    sdp: Option<String>,
    candidate: Option<RTCIceCandidateInit>,
}

/// Serializes an ICE candidate into a message, to be forwarded to a client via a websocket connection.
pub fn generate_ice_candidate_message(candidate: RTCIceCandidate) -> String {
    let msg = SignalingMessage {
        sdp: None,
        candidate: Some(candidate.to_json().unwrap()),
    };

    serde_json::to_string(&msg).expect("Should have been serialized")
}

/// Generates and serializes an SDP offer into a message, for a given peer connection, to be forwarded via a websocket connection.
pub async fn generate_sdp_offer_message(peer_connection: &Arc<RTCPeerConnection>) -> String {
    // Generate
    let offer = peer_connection.create_offer(None).await.expect("Offer should have been created.");
    peer_connection.set_local_description(offer.clone()).await.expect("Local description should have been set.");

    // Serialize
    let msg = SignalingMessage {
        sdp: Some(offer.sdp),
        candidate: None
    };

    serde_json::to_string(&msg).expect("Should have been serialized")
}

#[allow(clippy::large_enum_variant, clippy::upper_case_acronyms)]
pub enum IncomingSignal {
    SDP(RTCSessionDescription),
    ICECandidate(RTCIceCandidateInit)
}

/// Takes a string message, and interprets it as a webrtc signalling message (SDP or ICE Candidate)
/// 
/// Way to interpret string may change between different webrtc clients.
pub fn parse_signalling_message(message: String) -> IncomingSignal {
    // Treat whole string as an SDP 
    if message.starts_with("v=0") {
        let desc = RTCSessionDescription::answer(message).expect("Should be an answer");

        return IncomingSignal::SDP(desc);
    }

    // Treat as JSON 
    if message.starts_with("{") && message.ends_with("}") {
        let v: Value = serde_json::from_str(&message).expect("Should have parsed as JSON");

        let obj = if let Value::Object(obj) = v { obj } else {
            panic!("JSON should be an object with keys");
        };

        let t = if let Value::String(s) = obj.get("type").expect("Should contain type") { s } else {
            panic!("JSON should contain key type")
        };

        if t == "ice" {
            // ICE Candidate
            let candidate = obj.get("candidate").expect("Should have candidate key populated"); 
            let candidate_json = serde_json::to_string(candidate).expect("Should have serialized");
            let deserialized: RTCIceCandidateInit = serde_json::from_str(&candidate_json).expect("Should be representable as RTCIceCandidateInit");

            return IncomingSignal::ICECandidate(deserialized);
        } else if t == "answer" {
            // SDP Answer
            let sdp = obj.get("sdp").expect("Should have sdp key populated");
            let sdp_str = sdp.as_str().expect("'sdp' should have a string value").to_string();
            let desc = RTCSessionDescription::answer(sdp_str).expect("Should be an answer");

            return IncomingSignal::SDP(desc);
        } 
    }

    panic!("Couldn't interpret: {}", message);
}

