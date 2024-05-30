pub use address::DestinationAddress;
use address::ToSocketDestination;
use error::ProxyStreamError;
use tokio::io::{AsyncRead, AsyncWrite};

pub(crate) mod address;
pub(crate) mod error;
mod http;
mod socks5;

pub use socks5::{Socks5, SocksConfig};

pub trait AsyncSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl<T> AsyncSocket for T where T: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

pub trait ProxyStream<T, U> {
    fn accept(
        &self,
        socket_stream: impl AsyncSocket,
    ) -> impl std::future::Future<Output = Result<impl InterruptedStream<T>, ProxyStreamError>>;
    fn connect(
        &self,
        socket_stream: impl AsyncSocket,
        addr: impl ToSocketDestination,
    ) -> impl std::future::Future<Output = Result<impl InterruptedStream<T>, ProxyStreamError>>;
}

pub trait InterruptedStream<T> {
    fn proxied_stream(
        self,
    ) -> impl std::future::Future<Output = Result<impl AsyncSocket, ProxyStreamError>>;
    fn serve(
        self,
        mut socket_stream: impl AsyncSocket,
    ) -> impl std::future::Future<Output = Result<(), ProxyStreamError>>
    where
        Self: Sized,
    {
        async move {
            let mut s = self.proxied_stream().await?;
            _ = tokio::io::copy_bidirectional(&mut s, &mut socket_stream).await?;
            Ok(())
        }
    }
    fn replay_error(
        self,
        error: ReplayError,
    ) -> impl std::future::Future<Output = Result<(), ProxyStreamError>>;
    fn addr(&self) -> &address::DestinationAddress;
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
