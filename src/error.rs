use std::{error::Error, fmt::Display};
#[derive(Debug)]
pub enum ProxyStreamError {
    InvalidVersion,
    InvalidAddress,
    InvalidDomain,
    MethodNotProvided,
    MethodNotSupported,
    TooManyMethods,
    GeneralSocksServerFailure,
    ConnectionNotAllowedByRuleset,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TtlExpired,
    CommandNotSupported,
    AddressTypeNotSupported,
    UnexpetedError,
    IoError(std::io::Error),
}

impl Error for ProxyStreamError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ProxyStreamError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl Display for ProxyStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyStreamError::InvalidVersion => write!(f, "Invalid version"),
            ProxyStreamError::InvalidAddress => write!(f, "Invalid address"),
            ProxyStreamError::InvalidDomain => write!(f, "Invalid domain"),
            ProxyStreamError::MethodNotProvided => write!(f, "Method not provided"),
            ProxyStreamError::MethodNotSupported => write!(f, "Method not supported"),
            ProxyStreamError::TooManyMethods => write!(f, "Too many methods"),
            ProxyStreamError::GeneralSocksServerFailure => {
                write!(f, "General socks server failure")
            }
            ProxyStreamError::ConnectionNotAllowedByRuleset => {
                write!(f, "Connection not allowed by ruleset")
            }
            ProxyStreamError::NetworkUnreachable => write!(f, "Network unreachable"),
            ProxyStreamError::HostUnreachable => write!(f, "Host unreachable"),
            ProxyStreamError::ConnectionRefused => write!(f, "Connection refused"),
            ProxyStreamError::TtlExpired => write!(f, "TTL expired"),
            ProxyStreamError::CommandNotSupported => write!(f, "Command not supported"),
            ProxyStreamError::AddressTypeNotSupported => write!(f, "Address type not supported"),
            ProxyStreamError::UnexpetedError => write!(f, "Unexpected error"),
            ProxyStreamError::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl From<std::io::Error> for ProxyStreamError {
    fn from(e: std::io::Error) -> Self {
        ProxyStreamError::IoError(e)
    }
}
