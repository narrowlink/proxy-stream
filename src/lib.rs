pub mod address;
mod error;
mod socks;

use address::ToSocketDestination;
use error::ProxyStreamError;
use socks::{AuthMethod, InterruptedSocks5, Socks5};
use tokio::io::{AsyncRead, AsyncWrite};

pub trait AsyncSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl<T> AsyncSocket for T where T: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

pub enum ProxyType {
    SOCKS5,
}
pub struct ProxyStream {
    proxy_type: ProxyType,
}

pub enum InterruptedStream {
    Socks5(InterruptedSocks5),
}

impl InterruptedStream {
    pub async fn serve(self, stream: impl AsyncSocket) -> Result<(), ProxyStreamError> {
        match self {
            InterruptedStream::Socks5(socks) => socks.serve(stream).await,
        }
    }
    pub fn addr(&self) -> &address::DestinationAddress {
        match self {
            InterruptedStream::Socks5(socks) => &socks.addr,
        }
    }
    pub async fn replay_error(self, replay: ReplayError) -> Result<(), ProxyStreamError> {
        match self {
            InterruptedStream::Socks5(socks) => socks.replay_error(replay.into()).await,
        }
    }
}

pub enum ReplayError {
    GeneralSocksServerFailure,
    ConnectionNotAllowedByRuleset,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TtlExpired,
    CommandNotSupported,
    AddressTypeNotSupported,
}

impl ProxyStream {
    pub fn new(proxy_type: ProxyType) -> Self {
        ProxyStream { proxy_type }
    }
    pub async fn accept(
        &self,
        stream: impl AsyncSocket,
    ) -> Result<InterruptedSocks5, ProxyStreamError> {
        match self.proxy_type {
            ProxyType::SOCKS5 => {
                let socks = Socks5::new(vec![AuthMethod::Noauth])?;
                socks.accept(stream).await
            }
        }
    }
    pub async fn connect(
        &self,
        stream: impl AsyncSocket,
        addr: impl ToSocketDestination,
    ) -> Result<impl AsyncSocket, ProxyStreamError> {
        match self.proxy_type {
            ProxyType::SOCKS5 => {
                let socks = Socks5::new(vec![AuthMethod::Noauth])?;
                socks.connect(stream, socks::Command::Connect, addr).await
            }
        }
    }
}
