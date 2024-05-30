use thiserror::Error;
pub(crate) mod address;
pub(crate) mod http;
pub(crate) mod socks;

#[derive(Error, Debug)]
pub enum ProxyStreamError {
    #[error("AddressError: {0}")]
    Address(#[from] address::AddrError),
    #[error("HttpError: {0}")]
    Http(#[from] http::HttpError),
    #[error("SocksError: {0}")]
    Socks(#[from] socks::SocksError),
    #[error("IOError: {0}")]
    IO(#[from] std::io::Error),
}
