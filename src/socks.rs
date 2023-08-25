use std::net::SocketAddr;

use crate::{
    address::DestinationAddress, error::ProxyStreamError, AsyncSocket, ReplayError,
    ToSocketDestination,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    select,
};

#[derive(PartialEq, Clone, Copy)]
pub enum Version {
    V5 = 5,
}

impl Version {
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, ProxyStreamError> {
        match reader.read_u8().await? {
            5 => Ok(Version::V5),
            _ => Err(ProxyStreamError::InvalidVersion),
        }
    }
}
#[derive(PartialEq, Debug, Clone)]
pub enum AuthMethod {
    Noauth,
    Gssapi,
    UsernamePassword,
    NoAcceptableMethod,
    Other(u8),
}

impl From<&AuthMethod> for u8 {
    fn from(v: &AuthMethod) -> Self {
        match *v {
            AuthMethod::Noauth => 0,
            AuthMethod::Gssapi => 1,
            AuthMethod::UsernamePassword => 2,
            AuthMethod::NoAcceptableMethod => 0xff,
            AuthMethod::Other(v) => v,
        }
    }
}

impl AuthMethod {
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, ProxyStreamError> {
        match reader.read_u8().await? {
            0 => Ok(AuthMethod::Noauth),
            1 => Ok(AuthMethod::Gssapi),
            2 => Ok(AuthMethod::UsernamePassword),
            0xff => Ok(AuthMethod::NoAcceptableMethod),
            v => Ok(AuthMethod::Other(v)),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Command {
    Connect = 1,
    Bind = 2,
    UdpAssociate = 3,
}

impl From<Command> for crate::Command {
    fn from(value: Command) -> Self {
        match value {
            Command::Connect => crate::Command::Connect,
            Command::Bind => crate::Command::Bind,
            Command::UdpAssociate => crate::Command::UdpAssociate,
        }
    }
}

impl Command {
    pub fn from(v: u8) -> Result<Self, ProxyStreamError> {
        match v {
            1 => Ok(Command::Connect),
            2 => Ok(Command::Bind),
            3 => Ok(Command::UdpAssociate),
            _ => Err(ProxyStreamError::CommandNotSupported),
        }
    }
}

#[derive(PartialEq)]
pub enum Replay {
    Succeeded,
    GeneralSocksServerFailure,
    ConnectionNotAllowedByRuleset,
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    TtlExpired,
    CommandNotSupported,
    AddressTypeNotSupported,
    Other(u8),
}

impl From<&Replay> for u8 {
    fn from(v: &Replay) -> Self {
        match *v {
            Replay::Succeeded => 0,
            Replay::GeneralSocksServerFailure => 1,
            Replay::ConnectionNotAllowedByRuleset => 2,
            Replay::NetworkUnreachable => 3,
            Replay::HostUnreachable => 4,
            Replay::ConnectionRefused => 5,
            Replay::TtlExpired => 6,
            Replay::CommandNotSupported => 7,
            Replay::AddressTypeNotSupported => 8,
            Replay::Other(v) => v,
        }
    }
}

impl From<u8> for Replay {
    fn from(v: u8) -> Self {
        match v {
            0 => Replay::Succeeded,
            1 => Replay::GeneralSocksServerFailure,
            2 => Replay::ConnectionNotAllowedByRuleset,
            3 => Replay::NetworkUnreachable,
            4 => Replay::HostUnreachable,
            5 => Replay::ConnectionRefused,
            6 => Replay::TtlExpired,
            7 => Replay::CommandNotSupported,
            8 => Replay::AddressTypeNotSupported,
            _ => Replay::Other(v),
        }
    }
}

pub struct InterruptedSocks5 {
    pub auth_method: AuthMethod,
    pub command: Command,
    pub addr: DestinationAddress,
    socket: Box<dyn AsyncSocket>,
}

impl InterruptedSocks5 {
    pub async fn serve(mut self, forward_socket: impl AsyncSocket) -> Result<(), ProxyStreamError> {
        match self.command {
            Command::Connect => {
                CommandResponse::new(Version::V5, Replay::Succeeded, self.addr)?
                    .write(&mut self.socket)
                    .await?;

                let (mut forward_socket_reader, mut forward_socket_writer) =
                    tokio::io::split(forward_socket);
                let (mut socket_reader, mut socket_writer) = tokio::io::split(self.socket);
                select! {
                    _ = tokio::io::copy(&mut forward_socket_reader, &mut socket_writer) => {
                        socket_writer.shutdown().await?;
                        forward_socket_writer.shutdown().await?;
                    }
                    _ = tokio::io::copy(&mut socket_reader, &mut forward_socket_writer) => {
                        socket_writer.shutdown().await?;
                        forward_socket_writer.shutdown().await?;
                }}
            }
            _ => {
                CommandResponse::new(
                    Version::V5,
                    Replay::CommandNotSupported,
                    DestinationAddress::default(),
                )?
                .write(&mut self.socket)
                .await?;
            }
        }
        Ok(())
    }
    pub async fn replay_error(mut self, replay: Replay) -> Result<(), ProxyStreamError> {
        self.socket
            .write_all(&[Version::V5 as u8, (&replay).into(), 0])
            .await?;
        Address::from(&DestinationAddress::default())
            .write(&mut self.socket)
            .await?;
        Ok(())
    }
    pub async fn connect(mut self) -> Result<impl AsyncSocket, ProxyStreamError> {
        match self.command {
            Command::Connect => {
                CommandResponse::new(Version::V5, Replay::Succeeded, self.addr)?
                    .write(&mut self.socket)
                    .await?;
                Ok(self.socket)
            }
            _ => Err(ProxyStreamError::CommandNotSupported),
        }
    }
}
pub struct Socks5 {
    auth_method: Vec<AuthMethod>,
}

impl Socks5 {
    pub fn new(auth_method: Vec<AuthMethod>) -> Result<Self, ProxyStreamError> {
        if auth_method.is_empty() {
            Err(ProxyStreamError::MethodNotProvided)
        } else if auth_method.len() > 255 {
            Err(ProxyStreamError::TooManyMethods)
        } else {
            Ok(Socks5 { auth_method })
        }
    }
    pub async fn accept(
        &self,
        mut socket: impl AsyncSocket,
    ) -> Result<InterruptedSocks5, ProxyStreamError> {
        let auth_request = AuthRequest::read(&mut socket).await?;
        if !auth_request.methods.contains(&AuthMethod::Noauth) {
            return Err(ProxyStreamError::MethodNotSupported);
        }
        AuthResponse::new(Version::V5, AuthMethod::Noauth)?
            .write(&mut socket)
            .await?;
        let request = CommandRequest::read(&mut socket).await?;

        Ok(InterruptedSocks5 {
            auth_method: AuthMethod::Noauth,
            command: request.command,
            addr: request.addr,
            socket: Box::new(socket),
        })
    }
    pub async fn connect(
        &self,
        mut socket: impl AsyncSocket,
        command: Command,
        addr: impl ToSocketDestination,
    ) -> Result<impl AsyncSocket, ProxyStreamError> {
        AuthRequest::new(Version::V5, self.auth_method.clone())?
            .write(&mut socket)
            .await?;
        AuthResponse::read(&mut socket).await?;
        CommandRequest::new(Version::V5, command, addr.to_destination_address()?)?
            .write(&mut socket)
            .await?;
        CommandResponse::read(&mut socket).await?;
        Ok(socket)
    }
}

struct AuthRequest {
    version: Version,
    methods: Vec<AuthMethod>,
}

impl AuthRequest {
    pub fn new(version: Version, methods: Vec<AuthMethod>) -> Result<Self, ProxyStreamError> {
        if methods.is_empty() {
            return Err(ProxyStreamError::MethodNotProvided);
        }
        if methods.len() > 255 {
            return Err(ProxyStreamError::TooManyMethods);
        }
        if version != Version::V5 {
            return Err(ProxyStreamError::InvalidVersion);
        }
        Ok(AuthRequest { version, methods })
    }
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, ProxyStreamError> {
        let version = Version::read(&mut reader).await?;
        if version != Version::V5 {
            return Err(ProxyStreamError::InvalidVersion);
        }
        let number_of_methods = reader.read_u8().await?;
        let mut methods = Vec::new();
        for _ in 0..number_of_methods {
            methods.push(AuthMethod::read(&mut reader).await?);
        }
        Ok(AuthRequest { version, methods })
    }
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), ProxyStreamError> {
        writer
            .write_all(
                &[
                    [self.version as u8].as_ref(),
                    [self.methods.len() as u8].as_ref(),
                    self.methods
                        .iter()
                        .map(|m| m.into())
                        .collect::<Vec<u8>>()
                        .as_ref(),
                ]
                .concat(),
            )
            .await
            .map_err(|e| e.into())
    }
}

pub struct AuthResponse {
    version: Version,
    method: AuthMethod,
}

impl AuthResponse {
    pub fn new(version: Version, method: AuthMethod) -> Result<Self, ProxyStreamError> {
        if version != Version::V5 {
            return Err(ProxyStreamError::InvalidVersion);
        }
        Ok(AuthResponse { version, method })
    }
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, ProxyStreamError> {
        let version = Version::read(&mut reader).await?;
        if version != Version::V5 {
            return Err(ProxyStreamError::InvalidVersion);
        }
        let method = AuthMethod::read(&mut reader).await?;
        Ok(AuthResponse { version, method })
    }
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), ProxyStreamError> {
        writer
            .write_all(&[self.version as u8, (&self.method).into()])
            .await
            .map_err(|e| e.into())
    }
}

pub struct CommandRequest {
    version: Version,
    pub command: Command,
    pub addr: DestinationAddress,
}
impl CommandRequest {
    pub fn new(
        version: Version,
        command: Command,
        addr: DestinationAddress,
    ) -> Result<Self, ProxyStreamError> {
        if version != Version::V5 {
            return Err(ProxyStreamError::InvalidVersion);
        }
        Ok(CommandRequest {
            version,
            command,
            addr,
        })
    }
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, ProxyStreamError> {
        let version = Version::read(&mut reader).await?;
        if version != Version::V5 {
            return Err(ProxyStreamError::InvalidVersion);
        }
        let command = Command::from(reader.read_u8().await?)?;
        reader.read_u8().await?;
        let addr = Address::read(&mut reader).await?;
        Ok(CommandRequest {
            version,
            command,
            addr: addr.addr,
        })
    }
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), ProxyStreamError> {
        writer
            .write_all([self.version as u8, self.command as u8, 0].as_ref())
            .await?;
        Address::from(&self.addr).write(&mut writer).await
    }
}

pub struct CommandResponse {
    version: Version,
    replay: Replay,
    addr: DestinationAddress,
}

impl CommandResponse {
    pub fn new(
        version: Version,
        replay: Replay,
        addr: DestinationAddress,
    ) -> Result<Self, ProxyStreamError> {
        if version != Version::V5 {
            return Err(ProxyStreamError::InvalidVersion);
        }
        Ok(CommandResponse {
            version,
            replay,
            addr,
        })
    }
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, ProxyStreamError> {
        let version = Version::read(&mut reader).await?;
        if version != Version::V5 {
            return Err(ProxyStreamError::InvalidVersion);
        }
        let replay = Replay::from(reader.read_u8().await?);
        reader.read_u8().await?;
        let addr = Address::read(&mut reader).await?;
        Ok(CommandResponse {
            version,
            replay,
            addr: addr.addr,
        })
    }
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), ProxyStreamError> {
        writer
            .write_all([self.version as u8, (&self.replay).into(), 0].as_ref())
            .await?;
        Address::from(&self.addr).write(&mut writer).await
    }
}

pub struct Address {
    pub addr: DestinationAddress,
}

impl From<&DestinationAddress> for Address {
    fn from(addr: &DestinationAddress) -> Self {
        Address { addr: addr.clone() }
    }
}

impl Address {
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, ProxyStreamError> {
        match reader.read_u8().await? {
            1 => {
                let mut buf = [0u8; 6];
                reader.read_exact(&mut buf).await?;
                Ok(Self {
                    addr: DestinationAddress::from_bytes(&buf, true)?,
                })
            }
            3 => {
                let mut buf = vec![0u8; reader.read_u8().await? as usize + 2];
                reader.read_exact(&mut buf).await?;
                Ok(Self {
                    addr: DestinationAddress::from_bytes(&buf, false)?,
                })
            }
            4 => {
                let mut buf = [0u8; 18];
                reader.read_exact(&mut buf).await?;
                Ok(Self {
                    addr: DestinationAddress::from_bytes(&buf, true)?,
                })
            }
            _ => Err(ProxyStreamError::InvalidAddress),
        }
    }
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), ProxyStreamError> {
        let dest_addr_type: u8 = match self.addr {
            DestinationAddress::Domain(_, _) => 3,
            DestinationAddress::Ip(SocketAddr::V4(_)) => 1,
            DestinationAddress::Ip(SocketAddr::V6(_)) => 4,
        };

        let addr = if dest_addr_type == 3 {
            let addr = self.addr.to_bytes();
            let mut addr_and_len = vec![addr.len() as u8 - 2];
            addr_and_len.extend_from_slice(&addr);
            addr_and_len
        } else {
            self.addr.to_bytes()
        };
        writer
            .write_all(&[[dest_addr_type].as_ref(), addr.as_ref()].concat())
            .await
            .map_err(|e| e.into())
    }
}

impl From<ReplayError> for Replay {
    fn from(val: ReplayError) -> Self {
        match val {
            ReplayError::GeneralSocksServerFailure => Replay::GeneralSocksServerFailure,
            ReplayError::ConnectionNotAllowedByRuleset => Replay::ConnectionNotAllowedByRuleset,
            ReplayError::NetworkUnreachable => Replay::NetworkUnreachable,
            ReplayError::HostUnreachable => Replay::HostUnreachable,
            ReplayError::ConnectionRefused => Replay::ConnectionRefused,
            ReplayError::TtlExpired => Replay::TtlExpired,
            ReplayError::CommandNotSupported => Replay::CommandNotSupported,
            ReplayError::AddressTypeNotSupported => Replay::AddressTypeNotSupported,
        }
    }
}
