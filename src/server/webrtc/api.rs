use std::sync::Arc;

use log::info;
use tokio::{net::UdpSocket, sync::{mpsc, oneshot}};
use webrtc::{api::{setting_engine::SettingEngine, APIBuilder, API}, ice::{udp_mux::{UDPMuxDefault, UDPMuxParams}, udp_network::UDPNetwork}, peer_connection::RTCPeerConnection};

/// Request for a new RTCPeerConnection
struct Request {
    respond_to: oneshot::Sender<Arc<RTCPeerConnection>>
}

/// Handle for new connections to use
#[derive(Clone)]
pub struct RtcApiHandle {
    sender: mpsc::Sender<Request>
}

impl RtcApiHandle {
    pub fn new(listen_addr: &str) -> Self {
        let (sender, mut receiver) = mpsc::channel::<Request>(1024);

        let listen_addr = listen_addr.to_string();
        tokio::spawn(async move {
            let api = create_api(&listen_addr).await;

            while let Some(request) = receiver.recv().await {
                let peer_connection = api
                    .new_peer_connection(Default::default())
                    .await
                    .expect("Should have been created.");

                request.respond_to.send(Arc::new(peer_connection)).expect("Should have been sent.");
            }
        });

        Self { sender }
    }

    pub async fn new_peer_connection(&mut self) -> Arc<RTCPeerConnection> {
        let (respond_to, receiver) = oneshot::channel();

        self.sender.try_send(Request { respond_to  }).expect("Actor should be alive");

        receiver.await.expect("Actor should have responded")
    }
}

/// Creates a new API instance from the WebRTC crate
async fn create_api(listen_addr: &str) -> API {
    let mut s = SettingEngine::default();

    // Create a UDP socket to receive inbound packets
    let socket = UdpSocket::bind(listen_addr).await.expect("Opening UDP socket should have succeeded");

    s.set_udp_network(UDPNetwork::Muxed(UDPMuxDefault::new(
        UDPMuxParams::new(socket)
    )));

    let api = APIBuilder::new()
        .with_setting_engine(s)
        .build();

    info!("WebRTC API initialised");

    api
}

#[cfg(test)]
mod tests {
    use crate::server::webrtc::api::create_api;

    #[tokio::test]
    async fn api_builds() {
        let _ = create_api("0.0.0.0:3001").await;
    }
}