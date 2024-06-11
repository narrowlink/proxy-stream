mod config;

use std::net::SocketAddr;

use crate::{address::ToSocketDestination, error::socks::SocksError, ReplayStatus};
pub use config::Config as SocksConfig;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{address::DestinationAddress, error::ProxyStreamError, AsyncSocket};

pub struct Socks5;

pub struct Socks5Client<T> {
    config: SocksConfig,
    socket_stream: Option<T>,
}

#[allow(dead_code)]
pub struct Socks5Server<T> {
    config: SocksConfig,
    socket_stream: Option<T>,
}

impl Socks5 {
    pub fn new_client(
        config: SocksConfig,
        socket_stream: impl AsyncSocket,
    ) -> Socks5Client<impl AsyncSocket> {
        Socks5Client {
            config,
            socket_stream: Some(socket_stream),
        }
    }
    pub fn new_server(
        config: SocksConfig,
        socket_stream: impl AsyncSocket,
    ) -> Socks5Server<impl AsyncSocket> {
        Socks5Server {
            config,
            socket_stream: Some(socket_stream),
        }
    }
}

impl<T: AsyncSocket> Socks5Server<T> {
    pub async fn accept(&mut self) -> Result<ServerInterruptedSocks5Stream<T>, ProxyStreamError> {
        let mut socket_stream = self.socket_stream.take().ok_or(ProxyStreamError::Closed)?;
        let auth_request = AuthRequest::read(&mut socket_stream).await?;
        if !auth_request.methods.contains(&AuthMethod::NoAuth) {
            Err(SocksError::MethodNotSupported)?;
        }
        AuthResponse::new(Version::V5, AuthMethod::NoAuth)?
            .write(&mut socket_stream)
            .await?;
        let request = CommandRequest::read(&mut socket_stream).await?;

        Ok(ServerInterruptedSocks5Stream {
            addr: request.addr,
            socket: socket_stream,
        })
    }
}

impl<T: AsyncSocket> Socks5Client<T> {
    pub async fn connect(
        &mut self,
        addr: impl ToSocketDestination,
    ) -> Result<ClientInterruptedSocks5Stream<T>, ProxyStreamError> {
        let mut socket_stream = self.socket_stream.take().ok_or(ProxyStreamError::Closed)?;
        AuthRequest::new(Version::V5, self.config.auth_method.clone())?
            .write(&mut socket_stream)
            .await?;
        AuthResponse::read(&mut socket_stream).await?;

        Ok(ClientInterruptedSocks5Stream {
            addr: addr.to_destination_address()?,
            socket: socket_stream,
        })
    }
}

pub struct ClientInterruptedSocks5Stream<T> {
    addr: DestinationAddress,
    socket: T,
}
pub struct ServerInterruptedSocks5Stream<T> {
    addr: DestinationAddress,
    socket: T,
}

impl<T: AsyncSocket> ClientInterruptedSocks5Stream<T> {
    pub async fn replay_error(
        mut self,
        error: crate::ReplayStatus,
    ) -> Result<(), ProxyStreamError> {
        self.socket
            .write_all(&[Version::V5 as u8, (&Replay::from(error)).into(), 0])
            .await?;
        Address::from(&DestinationAddress::default())
            .write(&mut self.socket)
            .await
            .map_err(|e| e.into())
    }

    pub async fn proxied_stream(
        mut self,
    ) -> Result<impl crate::AsyncSocket, crate::error::ProxyStreamError> {
        CommandRequest::new(Version::V5, Command::Connect, self.addr.to_owned())?
            .write(&mut self.socket)
            .await?;
        CommandResponse::read(&mut self.socket).await?;

        Ok(self.socket)
    }
    pub async fn serve(self, mut socket_stream: impl AsyncSocket) -> Result<(), ProxyStreamError>
    where
        Self: Sized,
    {
        let mut s = self.proxied_stream().await?;
        _ = tokio::io::copy_bidirectional(&mut s, &mut socket_stream).await?;
        Ok(())
    }
}

impl<T: AsyncSocket> ServerInterruptedSocks5Stream<T> {
    pub fn addr(&self) -> &crate::address::DestinationAddress {
        &self.addr
    }

    pub async fn replay_error(
        mut self,
        error: crate::ReplayStatus,
    ) -> Result<(), ProxyStreamError> {
        self.socket
            .write_all(&[Version::V5 as u8, (&Replay::from(error)).into(), 0])
            .await?;
        Address::from(&DestinationAddress::default())
            .write(&mut self.socket)
            .await
            .map_err(|e| e.into())
    }

    async fn proxied_stream(
        mut self,
    ) -> Result<impl crate::AsyncSocket, crate::error::ProxyStreamError> {
        CommandResponse::new(Version::V5, Replay::Succeeded, self.addr.to_owned())?
            .write(&mut self.socket)
            .await?;

        Ok(self.socket)
    }
    pub async fn serve(self, mut socket_stream: impl AsyncSocket) -> Result<(), ProxyStreamError>
    where
        Self: Sized,
    {
        let mut s = self.proxied_stream().await?;
        _ = tokio::io::copy_bidirectional(&mut s, &mut socket_stream).await?;
        Ok(())
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum Version {
    V5 = 5,
}

impl Version {
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, SocksError> {
        match reader.read_u8().await? {
            5 => Ok(Version::V5),
            _ => Err(SocksError::InvalidVersion),
        }
    }
}
#[derive(PartialEq, Debug, Clone, Default)]
pub enum AuthMethod {
    #[default]
    NoAuth,
    GssApi,
    UsernamePassword,
    NoAcceptableMethod,
    Other(u8),
}

impl From<&AuthMethod> for u8 {
    fn from(v: &AuthMethod) -> Self {
        match *v {
            AuthMethod::NoAuth => 0,
            AuthMethod::GssApi => 1,
            AuthMethod::UsernamePassword => 2,
            AuthMethod::NoAcceptableMethod => 0xff,
            AuthMethod::Other(v) => v,
        }
    }
}

impl AuthMethod {
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, SocksError> {
        match reader.read_u8().await? {
            0 => Ok(AuthMethod::NoAuth),
            1 => Ok(AuthMethod::GssApi),
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

// impl From<Command> for crate::Command {
//     fn from(value: Command) -> Self {
//         match value {
//             Command::Connect => crate::Command::Connect,
//             Command::Bind => crate::Command::Bind,
//             Command::UdpAssociate => crate::Command::UdpAssociate,
//         }
//     }
// }

impl Command {
    pub fn from(v: u8) -> Result<Self, SocksError> {
        match v {
            1 => Ok(Command::Connect),
            2 => Ok(Command::Bind),
            3 => Ok(Command::UdpAssociate),
            _ => Err(SocksError::CommandNotSupported),
        }
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
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, SocksError> {
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
            _ => Err(SocksError::InvalidAddress),
        }
    }
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), SocksError> {
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

struct AuthRequest {
    version: Version,
    methods: Vec<AuthMethod>,
}

impl AuthRequest {
    pub fn new(version: Version, methods: Vec<AuthMethod>) -> Result<Self, SocksError> {
        if methods.is_empty() {
            return Err(SocksError::MethodNotProvided);
        }
        if methods.len() > 255 {
            return Err(SocksError::TooManyMethods);
        }
        if version != Version::V5 {
            return Err(SocksError::InvalidVersion);
        }
        Ok(AuthRequest { version, methods })
    }
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, SocksError> {
        let version = Version::read(&mut reader).await?;
        if version != Version::V5 {
            return Err(SocksError::InvalidVersion);
        }
        let number_of_methods = reader.read_u8().await?;
        let mut methods = Vec::new();
        for _ in 0..number_of_methods {
            methods.push(AuthMethod::read(&mut reader).await?);
        }
        Ok(AuthRequest { version, methods })
    }
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), SocksError> {
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
    pub fn new(version: Version, method: AuthMethod) -> Result<Self, SocksError> {
        if version != Version::V5 {
            return Err(SocksError::InvalidVersion);
        }
        Ok(AuthResponse { version, method })
    }
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, SocksError> {
        let version = Version::read(&mut reader).await?;
        if version != Version::V5 {
            return Err(SocksError::InvalidVersion);
        }
        let method = AuthMethod::read(&mut reader).await?;
        Ok(AuthResponse { version, method })
    }
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), SocksError> {
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
    ) -> Result<Self, SocksError> {
        if version != Version::V5 {
            return Err(SocksError::InvalidVersion);
        }
        Ok(CommandRequest {
            version,
            command,
            addr,
        })
    }
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, SocksError> {
        let version = Version::read(&mut reader).await?;
        if version != Version::V5 {
            return Err(SocksError::InvalidVersion);
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
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), SocksError> {
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
    ) -> Result<Self, SocksError> {
        if version != Version::V5 {
            return Err(SocksError::InvalidVersion);
        }
        Ok(CommandResponse {
            version,
            replay,
            addr,
        })
    }
    pub async fn read(mut reader: impl AsyncRead + Unpin) -> Result<Self, SocksError> {
        let version = Version::read(&mut reader).await?;
        if version != Version::V5 {
            return Err(SocksError::InvalidVersion);
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
    pub async fn write(&self, mut writer: impl AsyncWrite + Unpin) -> Result<(), SocksError> {
        writer
            .write_all([self.version as u8, (&self.replay).into(), 0].as_ref())
            .await?;
        Address::from(&self.addr).write(&mut writer).await
    }
}

impl From<ReplayStatus> for Replay {
    fn from(val: ReplayStatus) -> Self {
        match val {
            ReplayStatus::Succeeded => Replay::Succeeded,
            ReplayStatus::GeneralSocksServerFailure => Replay::GeneralSocksServerFailure,
            ReplayStatus::ConnectionNotAllowedByRuleset => Replay::ConnectionNotAllowedByRuleset,
            ReplayStatus::NetworkUnreachable => Replay::NetworkUnreachable,
            ReplayStatus::HostUnreachable => Replay::HostUnreachable,
            ReplayStatus::ConnectionRefused => Replay::ConnectionRefused,
            ReplayStatus::TtlExpired => Replay::TtlExpired,
            ReplayStatus::CommandNotSupported => Replay::CommandNotSupported,
            ReplayStatus::AddressTypeNotSupported => Replay::AddressTypeNotSupported,
        }
    }
}
