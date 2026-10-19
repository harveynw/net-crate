use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use webrtc::{
    error::Result,
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfiguration,
        RTCConfigurationBuilder,
    },
    runtime::TokioRuntime,
};

/// Shared settings for generating independent peer connections.
#[derive(Clone)]
pub struct RtcConfig {
    pub bind_ip: IpAddr,
    pub peer_configuration: RTCConfiguration,
}

impl RtcConfig {
    pub fn new(bind_ip: IpAddr) -> Self {
        Self {
            bind_ip,
            peer_configuration: RTCConfigurationBuilder::default().build(),
        }
    }

    pub async fn create_peer_connection(
        &self,
        handler: Arc<dyn PeerConnectionEventHandler>,
    ) -> Result<Arc<dyn PeerConnection>> {
        let peer = PeerConnectionBuilder::new()
            .with_configuration(self.peer_configuration.clone())
            .with_udp_addrs(vec![SocketAddr::new(self.bind_ip, 0)])
            .with_runtime(Arc::new(TokioRuntime))
            .with_handler(handler)
            .build()
            .await?;

        Ok(Arc::new(peer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::Ipv4Addr, time::Duration};
    use tokio::{sync::mpsc, time::timeout};
    use webrtc::peer_connection::RTCPeerConnectionIceEvent;

    struct CandidateHandler(mpsc::UnboundedSender<SocketAddr>);

    #[async_trait::async_trait]
    impl PeerConnectionEventHandler for CandidateHandler {
        async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
            let address = event.candidate.address.parse().unwrap();
            let _ = self.0.send(SocketAddr::new(address, event.candidate.port));
        }
    }

    #[tokio::test]
    async fn cloned_config_creates_independent_ephemeral_peers() {
        timeout(Duration::from_secs(10), async {
            let config = RtcConfig::new(Ipv4Addr::LOCALHOST.into());
            let cloned = config.clone();
            let (tx_a, mut candidates_a) = mpsc::unbounded_channel();
            let (tx_b, mut candidates_b) = mpsc::unbounded_channel();
            let (a, b) = tokio::join!(
                config.create_peer_connection(Arc::new(CandidateHandler(tx_a))),
                cloned.create_peer_connection(Arc::new(CandidateHandler(tx_b))),
            );
            let (a, b) = (a.unwrap(), b.unwrap());
            let channel_a = a.create_data_channel("a", None).await.unwrap();
            let channel_b = b.create_data_channel("b", None).await.unwrap();
            a.set_local_description(a.create_offer(None).await.unwrap())
                .await
                .unwrap();
            b.set_local_description(b.create_offer(None).await.unwrap())
                .await
                .unwrap();
            let address_a = candidates_a.recv().await.unwrap();
            let address_b = candidates_b.recv().await.unwrap();
            assert_eq!(address_a.ip(), config.bind_ip);
            assert_eq!(address_b.ip(), config.bind_ip);
            assert_ne!(address_a.port(), 0);
            assert_ne!(address_b.port(), 0);
            assert_ne!(address_a, address_b);
            a.close().await.unwrap();
            b.close().await.unwrap();
            while channel_a.poll().await.is_some() {}
            while channel_b.poll().await.is_some() {}
        })
        .await
        .expect("Peer creation should finish");
    }
}
