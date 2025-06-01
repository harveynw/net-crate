use log::warn;
use serde_json::Value;
use serde::{Serialize, Deserialize};
use std::{error::Error, fmt::Display, sync::Arc};
use webrtc::{ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit}, peer_connection::{sdp::session_description::RTCSessionDescription, RTCPeerConnection}};


/// Signalling struct to be forwarded to a client, who can easily inspect the contents when in JSON form.
#[derive(Serialize, Deserialize)]
struct OutgoingSignallingMessage {
    sdp: Option<String>,
    candidate: Option<RTCIceCandidateInit>,
}

/// Serializes an ICE candidate into a message, to be forwarded to a client via a websocket connection.
pub fn generate_ice_candidate_message(candidate: RTCIceCandidate) -> String {
    let msg = OutgoingSignallingMessage {
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
    let msg = OutgoingSignallingMessage {
        sdp: Some(offer.sdp),
        candidate: None
    };

    serde_json::to_string(&msg).expect("Should have been serialized")
}

#[allow(clippy::large_enum_variant, clippy::upper_case_acronyms)]
enum IncomingSignallingMessage {
    SDP(RTCSessionDescription),
    ICECandidate(RTCIceCandidateInit)
}

/// Handles a string message, interpreted as a signalling message, by mutating its associated RTCPeerConnection.
/// 
/// Emits a warning if parsing or handling of the message fails.
pub async fn handle_signalling_message(peer_connection: Arc<RTCPeerConnection>, message: String) {
    // Parse
    let signal = match parse_signalling_message(&message) {
        Ok(signal) => signal,
        Err(err) => { warn!("Couldn't parse signalling message: {:?}", err); return; },
    };

    // Handle
    let result = match signal {
        IncomingSignallingMessage::SDP(desc) => {
            peer_connection.set_remote_description(desc).await
        },
        IncomingSignallingMessage::ICECandidate(ice) => {
            peer_connection.add_ice_candidate(ice.clone()).await
        },
    };

    // Logging
    if let Err(err) = result {
        warn!("Couldn't handle signalling message {:?}", err);
    }
}

/// Represents a failure to correctly parse a signalling message
#[derive(Debug, Clone)]
struct ParseError(String);

impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Signalling Parse Error: {}", &self.0)
    }
}
impl Error for ParseError {}

/// Takes a string message, and interprets it as a signalling message (SDP or ICE Candidate)
/// 
/// Supports either JSON or the case when the entire string is an SDP, returns None if parsing wasn't successful.
fn parse_signalling_message(message: &str) -> Result<IncomingSignallingMessage, Box<dyn Error>> {
    // Treat whole string as an SDP 
    if message.starts_with("v=0") {
        return Ok(IncomingSignallingMessage::SDP(RTCSessionDescription::answer(message.to_string())?));
    }

    // Treat as JSON 
    let v: Value = serde_json::from_str(message)?;

    let obj = if let Value::Object(obj) = v { obj } else {
        return Err(Box::new(ParseError("JSON object should be a map".into())));
    };

    let t = obj.get("type").ok_or(ParseError("Map should contain key 'type'".into()))?; 
       
    if t == "ice" {
        // ICE Candidate
        let candidate = obj.get("candidate").ok_or("Should have key 'candidate'")?;
        let candidate_json = serde_json::to_string(candidate)?;
        let deserialized: RTCIceCandidateInit = serde_json::from_str(&candidate_json)?;

        return Ok(IncomingSignallingMessage::ICECandidate(deserialized));
    } else if t == "answer" {
        // SDP Answer
        let sdp = obj.get("sdp").ok_or("Should have key 'sdp'")?;
        let sdp_str = sdp.as_str().ok_or("Value of 'sdp' should be a string")?.to_string();
        let desc = RTCSessionDescription::answer(sdp_str)?;

        return Ok(IncomingSignallingMessage::SDP(desc));
    } 

    Err(Box::new(ParseError("Unrecognised signalling message format".into())))
}

