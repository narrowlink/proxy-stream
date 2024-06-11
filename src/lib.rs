pub use address::DestinationAddress;
use tokio::io::{AsyncRead, AsyncWrite};

pub(crate) mod address;
pub(crate) mod error;
mod http;
mod socks5;

pub use http::{Http, HttpConfig};
pub use socks5::{Socks5, SocksConfig};
pub trait AsyncSocket: AsyncRead + AsyncWrite + Unpin + Send + 'static {}
impl<T> AsyncSocket for T where T: AsyncRead + AsyncWrite + Unpin + Send + 'static {}

pub enum ReplayStatus {
    Succeeded,
    GeneralSocksServerFailure,
    ConnectionNotAllowedByRuleset,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TtlExpired,
    CommandNotSupported,
    AddressTypeNotSupported,
}
