use std::net::SocketAddr;

use crate::error::ProxyStreamError;

#[derive(Debug, Clone)]
pub enum DestinationAddress {
    Domain(String, u16),
    Ip(SocketAddr),
}

impl DestinationAddress {
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            DestinationAddress::Domain(domain, port) => {
                [domain.as_bytes(), port.to_be_bytes().as_ref()].concat()
            }
            DestinationAddress::Ip(addr) => match addr {
                SocketAddr::V4(addr) => {
                    [&addr.ip().octets(), addr.port().to_be_bytes().as_ref()].concat()
                }
                SocketAddr::V6(addr) => {
                    [&addr.ip().octets(), addr.port().to_be_bytes().as_ref()].concat()
                }
            },
        }
    }
    pub fn from_bytes(buf: &[u8], ip: bool) -> Result<Self, ProxyStreamError> {
        if buf.len() < 3 {
            return Err(ProxyStreamError::InvalidAddress);
        }
        if ip {
            let port = u16::from_be_bytes([buf[buf.len() - 2], buf[buf.len() - 1]]);
            let ip = match buf.len() {
                6 => {
                    let mut octets = [0; 4];
                    octets.copy_from_slice(&buf[0..4]);
                    SocketAddr::V4(std::net::SocketAddrV4::new(
                        std::net::Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]),
                        port,
                    ))
                }
                18 => {
                    let mut octets = [0; 16];
                    octets.copy_from_slice(&buf[0..16]);
                    SocketAddr::V6(std::net::SocketAddrV6::new(
                        std::net::Ipv6Addr::new(
                            u16::from_be_bytes([octets[0], octets[1]]),
                            u16::from_be_bytes([octets[2], octets[3]]),
                            u16::from_be_bytes([octets[4], octets[5]]),
                            u16::from_be_bytes([octets[6], octets[7]]),
                            u16::from_be_bytes([octets[8], octets[9]]),
                            u16::from_be_bytes([octets[10], octets[11]]),
                            u16::from_be_bytes([octets[12], octets[13]]),
                            u16::from_be_bytes([octets[14], octets[15]]),
                        ),
                        port,
                        0,
                        0,
                    ))
                }
                _ => return Err(ProxyStreamError::InvalidAddress),
            };
            Ok(DestinationAddress::Ip(ip))
        } else {
            let port = u16::from_be_bytes([buf[buf.len() - 2], buf[buf.len() - 1]]);
            let domain = String::from_utf8_lossy(&buf[0..buf.len() - 2]).to_string();
            Ok(DestinationAddress::Domain(domain, port))
        }
    }
}

impl Default for DestinationAddress {
    fn default() -> Self {
        DestinationAddress::Ip(SocketAddr::from(([0, 0, 0, 0], 0)))
    }
}
pub trait ToSocketDestination {
    fn to_destination_address(&self) -> Result<DestinationAddress, ProxyStreamError>;
}

impl ToSocketDestination for SocketAddr {
    fn to_destination_address(&self) -> Result<DestinationAddress, ProxyStreamError> {
        Ok(DestinationAddress::Ip(*self))
    }
}

impl ToSocketDestination for &str {
    fn to_destination_address(&self) -> Result<DestinationAddress, ProxyStreamError> {
        if let Ok(ip) = self.parse::<SocketAddr>() {
            return Ok(DestinationAddress::Ip(ip));
        }
        self.rsplit_once(':')
            .and_then(|(domain, port)| {
                port.parse::<u16>()
                    .ok()
                    .map(|port| DestinationAddress::Domain(domain.to_string(), port))
            })
            .ok_or(ProxyStreamError::InvalidAddress)
    }
}
