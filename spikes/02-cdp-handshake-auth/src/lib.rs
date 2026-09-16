pub mod broker;
pub mod client;
pub mod events;

pub use broker::{CdpBroker, BrokerError, NONCE_HEADER};
pub use client::{CdpClient, ClientError};
pub use events::{CdpEvent, CdpRequest, CdpResponse};

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[tokio::test]
    async fn test_non_loopback_binding_rejected() {
        let external_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 9222);
        let res = CdpBroker::bind_with_addr(external_addr, "test_nonce").await;
        assert!(matches!(res, Err(BrokerError::NonLoopbackBindingForbidden(_))));
    }

    #[tokio::test]
    async fn test_ephemeral_loopback_binding() {
        let (broker, addr) = CdpBroker::bind_ephemeral("test_nonce").await.unwrap();
        assert!(addr.ip().is_loopback());
        assert!(addr.port() > 0);
        broker.shutdown();
    }
}
