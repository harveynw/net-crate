use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use webrtc::{
    error::Result,
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfiguration,
        RTCConfigurationBuilder, SettingEngineBuilder,
    },
    runtime::TokioRuntime,
};

/// Shared settings for generating independent peer connections.
#[derive(Clone)]
pub struct RtcConfig {
    host: IpAddr,
    peer_configuration: RTCConfiguration,
}

impl RtcConfig {
    pub fn new(host: IpAddr) -> Self {
        assert!(
            !host.is_unspecified()
                && !host.is_multicast()
                && !matches!(host, IpAddr::V4(ip) if ip.is_broadcast())
                && !matches!(host, IpAddr::V6(ip) if ip.is_unicast_link_local()),
            "WebRTC host must be a concrete unicast IP without a scope ID"
        );
        Self {
            host,
            peer_configuration: RTCConfigurationBuilder::default()
                .with_ice_servers(vec![])
                .build(),
        }
    }

    pub async fn create_peer_connection(
        &self,
        handler: Arc<dyn PeerConnectionEventHandler>,
    ) -> Result<Arc<dyn PeerConnection>> {
        let peer = PeerConnectionBuilder::new()
            .with_configuration(self.peer_configuration.clone())
            .with_setting_engine(SettingEngineBuilder::default().with_lite(true).build())
            .with_udp_addrs(vec![SocketAddr::new(self.host, 0)])
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
    use webrtc::peer_connection::{
        RTCIceCandidate, RTCIceCandidateType, RTCIceGatheringState, RTCIceProtocol,
        RTCPeerConnectionIceEvent,
    };

    struct CandidateHandler(mpsc::UnboundedSender<Option<RTCIceCandidate>>);

    #[async_trait::async_trait]
    impl PeerConnectionEventHandler for CandidateHandler {
        async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
            let _ = self.0.send(Some(event.candidate));
        }

        async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
            if state == RTCIceGatheringState::Complete {
                let _ = self.0.send(None);
            }
        }
    }

    async fn gathered(
        rx: &mut mpsc::UnboundedReceiver<Option<RTCIceCandidate>>,
        host: IpAddr,
    ) -> SocketAddr {
        let mut addresses = Vec::new();
        while let Some(candidate) = rx.recv().await.expect("candidate stream ended") {
            assert_eq!(candidate.address.parse::<IpAddr>().unwrap(), host);
            assert_eq!(candidate.typ, RTCIceCandidateType::Host);
            assert_eq!(candidate.protocol, RTCIceProtocol::Udp);
            addresses.push(SocketAddr::new(host, candidate.port));
        }
        assert_eq!(addresses.len(), 1);
        addresses[0]
    }

    #[test]
    fn validates_hosts() {
        for host in [
            "0.0.0.0",
            "::",
            "224.0.0.1",
            "ff02::1",
            "255.255.255.255",
            "fe80::1",
        ] {
            assert!(std::panic::catch_unwind(|| RtcConfig::new(host.parse().unwrap())).is_err());
        }
        for host in ["127.0.0.1", "::1", "192.168.1.2", "fd00::1"] {
            RtcConfig::new(host.parse().unwrap());
        }
    }

    #[tokio::test]
    async fn ipv6_loopback_advertises_only_host_candidate() {
        let probe = match std::net::UdpSocket::bind("[::1]:0") {
            Ok(probe) => probe,
            Err(error) => {
                eprintln!("Skipping IPv6 loopback: {error}");
                return;
            }
        };
        drop(probe);
        timeout(Duration::from_secs(10), async {
            let host = "::1".parse().unwrap();
            let config = RtcConfig::new(host);
            let (tx, mut rx) = mpsc::unbounded_channel();
            let peer = config
                .create_peer_connection(Arc::new(CandidateHandler(tx)))
                .await
                .unwrap();
            let channel = peer.create_data_channel("ipv6", None).await.unwrap();
            let offer = peer.create_offer(None).await.unwrap();
            assert!(offer.sdp.lines().any(|line| line == "a=ice-lite"));
            peer.set_local_description(offer).await.unwrap();
            assert_ne!(gathered(&mut rx, host).await.port(), 0);
            peer.close().await.unwrap();
            while channel.poll().await.is_some() {}
        })
        .await
        .expect("IPv6 gathering should finish");
    }

    #[tokio::test]
    async fn cloned_config_creates_independent_ephemeral_peers() {
        timeout(Duration::from_secs(10), async {
            let config = RtcConfig::new(Ipv4Addr::LOCALHOST.into());
            assert!(config.peer_configuration.ice_servers().is_empty());
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
            let offer = a.create_offer(None).await.unwrap();
            assert!(offer.sdp.lines().any(|line| line == "a=ice-lite"));
            a.set_local_description(offer).await.unwrap();
            b.set_local_description(b.create_offer(None).await.unwrap())
                .await
                .unwrap();
            let address_a = gathered(&mut candidates_a, config.host).await;
            let address_b = gathered(&mut candidates_b, config.host).await;
            assert_eq!(address_a.ip(), config.host);
            assert_eq!(address_b.ip(), config.host);
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
