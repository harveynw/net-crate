use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{error::Error, fmt::Display};
use webrtc::peer_connection::{
    PeerConnection, RTCIceCandidate, RTCIceCandidateInit, RTCSessionDescription,
};

/// Signalling struct to be forwarded to a client, who can easily inspect the contents when in JSON form.
#[derive(Serialize, Deserialize)]
struct OutgoingSignallingMessage {
    sdp: Option<String>,
    candidate: Option<RTCIceCandidateInit>,
}

/// Serializes an ICE candidate into a message, to be forwarded to a client via a websocket connection.
pub fn generate_ice_candidate_message(candidate: RTCIceCandidate) -> webrtc::error::Result<String> {
    let msg = OutgoingSignallingMessage {
        sdp: None,
        candidate: Some(candidate.to_json()?),
    };

    Ok(serde_json::to_string(&msg).expect("Should have been serialized"))
}

/// Generates and serializes an SDP offer into a message, for a given peer connection, to be forwarded via a websocket connection.
pub async fn generate_sdp_offer_message(
    peer_connection: &dyn PeerConnection,
) -> webrtc::error::Result<String> {
    // Generate
    let offer = peer_connection.create_offer(None).await?;
    peer_connection.set_local_description(offer.clone()).await?;

    // Serialize
    let msg = OutgoingSignallingMessage {
        sdp: Some(offer.sdp),
        candidate: None,
    };

    Ok(serde_json::to_string(&msg).expect("Should have been serialized"))
}

#[allow(clippy::large_enum_variant, clippy::upper_case_acronyms)]
enum IncomingSignallingMessage {
    SDP(RTCSessionDescription),
    ICECandidate(RTCIceCandidateInit),
}

/// Serializes remote description updates and defers candidates received before an answer.
#[derive(Default)]
pub struct SignallingState {
    pending_candidates: Vec<RTCIceCandidateInit>,
}

impl SignallingState {
    pub async fn handle_message(&mut self, peer: &dyn PeerConnection, message: &str) {
        let signal = match parse_signalling_message(message) {
            Ok(signal) => signal,
            Err(error) => {
                warn!("Could not parse signalling message: {error}");
                return;
            }
        };

        match signal {
            IncomingSignallingMessage::SDP(description) => {
                if let Err(error) = peer.set_remote_description(description).await {
                    warn!("Could not apply remote description: {error}");
                    return;
                }
                for candidate in self.pending_candidates.drain(..) {
                    if let Err(error) = peer.add_ice_candidate(candidate).await {
                        warn!("Could not add buffered ICE candidate: {error}");
                    }
                }
            }
            IncomingSignallingMessage::ICECandidate(candidate) => {
                if peer.remote_description().await.is_none() {
                    self.pending_candidates.push(candidate);
                } else if let Err(error) = peer.add_ice_candidate(candidate).await {
                    warn!("Could not add ICE candidate: {error}");
                }
            }
        }
    }
}

/// Represents a failure to correctly parse a signalling message
#[derive(Debug, Clone)]
struct ParseError(String);

impl Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Signalling Parse Error, (perhaps check your client implementation?): {}",
            &self.0
        )
    }
}
impl Error for ParseError {}

/// Accepts a raw SDP answer, JSON `{type: "answer", sdp: ...}`, or
/// JSON `{type: "ice", candidate: ...}`. Invalid messages return a parse error.
fn parse_signalling_message(
    message: &str,
) -> Result<IncomingSignallingMessage, Box<dyn Error + Send + Sync>> {
    // Treat whole string as an SDP
    if message.starts_with("v=0") {
        return Ok(IncomingSignallingMessage::SDP(
            RTCSessionDescription::answer(message.to_string())?,
        ));
    }

    // Treat as JSON
    let v: Value = serde_json::from_str(message)?;

    let obj = if let Value::Object(obj) = v {
        obj
    } else {
        return Err(Box::new(ParseError("JSON object should be a map".into())));
    };

    let t = obj
        .get("type")
        .ok_or(ParseError("Map should contain key 'type'".into()))?;

    if t == "ice" {
        // ICE Candidate
        let candidate = obj.get("candidate").ok_or("Should have key 'candidate'")?;
        let candidate_json = serde_json::to_string(candidate)?;
        let deserialized: RTCIceCandidateInit = serde_json::from_str(&candidate_json)?;

        return Ok(IncomingSignallingMessage::ICECandidate(deserialized));
    } else if t == "answer" {
        // SDP Answer
        let sdp = obj.get("sdp").ok_or("Should have key 'sdp'")?;
        let sdp_str = sdp
            .as_str()
            .ok_or("Value of 'sdp' should be a string")?
            .to_string();
        let desc = RTCSessionDescription::answer(sdp_str)?;

        return Ok(IncomingSignallingMessage::SDP(desc));
    }

    Err(Box::new(ParseError(
        "Unrecognised signalling message format".into(),
    )))
}
