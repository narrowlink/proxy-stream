use thiserror::Error;

use super::address;
#[derive(Error, Debug)]
pub enum SocksError {
    #[error("Invalid Version")]
    InvalidVersion,
    #[error("Command not supported")]
    CommandNotSupported,
    #[error("Method not supported")]
    MethodNotSupported,
    #[error("Method not provided")]
    MethodNotProvided,
    #[error("Too many methods provided")]
    TooManyMethods,
    #[error("Invalid Address")]
    InvalidAddress,
    #[error("IOError: {0}")]
    IOError(#[from] std::io::Error),
    #[error("AddressError: {0}")]
    AddressError(#[from] address::AddrError),
}
