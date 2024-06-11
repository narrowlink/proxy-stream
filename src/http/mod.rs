pub mod config;
use std::{pin::Pin, str::FromStr, time::Duration};

pub use config::Config as HttpConfig;

use hyper::{
    body::{Body, Bytes, Incoming},
    header::HOST,
    service::Service,
    upgrade::Upgraded,
    Request, Response,
};
use hyper_util::rt::TokioIo;
use log::{debug, warn};
use resumable_io::ResumableIO;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::{
    address::ToSocketDestination,
    error::{http::HttpError, ProxyStreamError},
    AsyncSocket, DestinationAddress, ReplayStatus,
};

pub struct Http;

#[allow(dead_code)]
pub struct HttpServer {
    config: HttpConfig,
    receiver: UnboundedReceiver<ServerInterrupted>,
}

impl Http {
    pub fn new_server(config: HttpConfig, socket_stream: impl AsyncSocket) -> HttpServer {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let http = hyper::server::conn::http1::Builder::new();
        tokio::task::spawn(async move {
            if let Err(e) = http
                .serve_connection(
                    hyper_util::rt::tokio::TokioIo::new(socket_stream),
                    ServerService { sender },
                )
                .with_upgrades()
                .await
            {
                debug!("{:?}", e);
            };
        });
        HttpServer { config, receiver }
    }
    pub fn new_client(
        config: HttpConfig,
        socket_stream: impl AsyncSocket,
    ) -> HttpClient<impl AsyncSocket> {
        HttpClient {
            config,
            stream: Some(socket_stream),
        }
    }
}

pub enum ServerInterrupted {
    Connect(ServerInterruptedHttpStream),
    Request(ServerInterruptedHttpItem),
}

impl ServerInterrupted {
    pub fn addr(&self) -> &crate::address::DestinationAddress {
        match self {
            ServerInterrupted::Connect(stream) => stream.addr(),
            ServerInterrupted::Request(item) => &item.addr,
        }
    }
    pub async fn serve(self, socket_stream: impl AsyncSocket) -> Result<(), ProxyStreamError> {
        match self {
            ServerInterrupted::Connect(stream) => stream.serve(socket_stream).await,
            ServerInterrupted::Request(item) => item.serve(socket_stream).await,
        }
    }
}

impl HttpServer {
    pub async fn accept(&mut self) -> Result<ServerInterrupted, ProxyStreamError> {
        self.receiver.recv().await.ok_or(ProxyStreamError::Closed)
    }
}
#[allow(dead_code)]
pub struct HttpClient<T> {
    config: HttpConfig,
    stream: Option<T>,
}

impl<T: AsyncSocket> HttpClient<T> {
    pub async fn connect(
        &mut self,
        addr: impl ToSocketDestination,
    ) -> Result<impl AsyncSocket, ProxyStreamError> {
        let addr = addr.to_destination_address()?.to_string();
        let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
            .handshake(hyper_util::rt::TokioIo::new(
                self.stream.take().ok_or(ProxyStreamError::Closed)?,
            ))
            .await
            .or(Err(HttpError::BuildHttpReq))?;
        let req = hyper::Request::builder()
            .method("CONNECT")
            .uri(addr.clone())
            .header(HOST, addr)
            .header("Proxy-Connection", "keep-alive")
            .body(IncomingWrapper::new(None))
            .or(Err(HttpError::CreateHttpReq))?;

        tokio::task::spawn(async move {
            if let Err(err) = conn.with_upgrades().await {
                debug!("{:?}", err);
            }
        });

        let res = sender
            .send_request(req)
            .await
            .or(Err(HttpError::SendHttpReq))?;
        hyper::upgrade::on(res)
            .await
            .map(hyper_util::rt::tokio::TokioIo::new)
            .map_err(|e| HttpError::UpgradeHttpReq(e).into())
    }
}

pub struct ServerService {
    sender: tokio::sync::mpsc::UnboundedSender<ServerInterrupted>,
}

impl Service<hyper::Request<Incoming>> for ServerService {
    type Response = Response<IncomingWrapper>;

    type Error = hyper::Error;

    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn call(&self, req: hyper::Request<Incoming>) -> Self::Future {
        let sender = self.sender.clone();
        Box::pin(async move {
            if req.method() == hyper::Method::CONNECT {
                let host = req.headers().get("host").and_then(|s| {
                    s.to_str().ok().map(|s| {
                        if s.contains(':') {
                            s.to_string()
                        } else {
                            format!("{}:80", s)
                        }
                    })
                });
                let Some(addr) = req
                    .uri()
                    .authority()
                    .map(|a| a.as_str())
                    .filter(|a| if let Some(h) = host { a == &h } else { true })
                    .and_then(|a| DestinationAddress::from_str(a).ok())
                else {
                    let mut response = hyper::Response::new(IncomingWrapper::new(None));
                    *response.status_mut() = hyper::StatusCode::BAD_REQUEST;
                    return Ok::<_, hyper::Error>(response);
                };

                let (stream, mut stream_controller) =
                    ResumableIO::<TokioIo<Upgraded>>::new(None, Duration::from_secs(10));
                let (status_sender, status_receiver) =
                    tokio::sync::oneshot::channel::<ReplayStatus>();
                if sender
                    .send(ServerInterrupted::Connect(ServerInterruptedHttpStream {
                        addr: addr.clone(),
                        status_sender,
                        stream,
                    }))
                    .is_err()
                {
                    let mut response = hyper::Response::new(IncomingWrapper::new(None));
                    *response.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                    return Ok::<_, hyper::Error>(response);
                }
                match status_receiver.await {
                    Ok(status) => match status {
                        ReplayStatus::Succeeded => {
                            let mut response = hyper::Response::new(IncomingWrapper::new(None));

                            let intrupted = match stream_controller.recv().await {
                                Some(i) => i,
                                None => {
                                    let mut response =
                                        hyper::Response::new(IncomingWrapper::new(None));
                                    *response.status_mut() =
                                        hyper::StatusCode::INTERNAL_SERVER_ERROR;
                                    return Ok::<_, hyper::Error>(response);
                                }
                            };
                            tokio::spawn(async move {
                                let upgraded = match hyper::upgrade::on(req)
                                    .await
                                    .map(hyper_util::rt::tokio::TokioIo::new)
                                {
                                    Ok(upgraded) => upgraded,
                                    Err(e) => {
                                        warn!("{:?}", e);
                                        return;
                                    }
                                };

                                if let Err(e) = intrupted.send_new_io(Some(upgraded)) {
                                    debug!("{:?}", e);
                                }
                            });

                            *response.status_mut() = hyper::StatusCode::OK;
                            Ok(response)
                        }
                        _ => {
                            let mut response = hyper::Response::new(IncomingWrapper::new(None));
                            *response.status_mut() = status.to_status_code();
                            Ok(response)
                        }
                    },
                    Err(e) => {
                        warn!("{:?}", e);
                        let mut response = hyper::Response::new(IncomingWrapper::new(None));
                        *response.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                        Ok(response)
                    }
                }
            } else {
                let host = match req
                    .headers()
                    .get("host")
                    .and_then(|s| {
                        s.to_str().ok().map(|s| {
                            if s.contains(':') {
                                s.to_string()
                            } else {
                                format!("{}:80", s)
                            }
                        })
                    })
                    .and_then(|s| DestinationAddress::from_str(&s).ok())
                {
                    Some(host) => host,
                    None => {
                        let mut response = hyper::Response::new(IncomingWrapper::new(None));
                        *response.status_mut() = hyper::StatusCode::BAD_REQUEST;
                        return Ok(response);
                    }
                };
                let (res_sender, res_receiver) = tokio::sync::oneshot::channel();
                if let Err(e) = sender.send(ServerInterrupted::Request(ServerInterruptedHttpItem {
                    addr: host,
                    req,
                    res: res_sender,
                })) {
                    warn!("{:?}", e);
                    let mut response = hyper::Response::new(IncomingWrapper::new(None));
                    *response.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                    return Ok(response);
                };
                let res = res_receiver.await.map(|res| {
                    let (parts, body) = res.into_parts();
                    Response::from_parts(parts, IncomingWrapper::new(body))
                });
                let res = match res {
                    Ok(res) => res,
                    Err(e) => {
                        warn!("{:?}", e);
                        let mut res = hyper::Response::new(IncomingWrapper::new(None));
                        *res.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                        res
                    }
                };

                Ok(res)
            }
        })
    }
}

pub struct ServerInterruptedHttpStream {
    addr: DestinationAddress,
    status_sender: tokio::sync::oneshot::Sender<ReplayStatus>,
    stream: ResumableIO<TokioIo<Upgraded>>,
}

impl ServerInterruptedHttpStream {
    pub async fn proxied_stream(self) -> Result<impl AsyncSocket, ProxyStreamError> {
        self.status_sender
            .send(ReplayStatus::Succeeded)
            .map_err(|_| ProxyStreamError::Closed)?;
        Ok(self.stream)
    }

    pub async fn replay_error(self, error: crate::ReplayStatus) -> Result<(), ProxyStreamError> {
        self.status_sender
            .send(error)
            .map_err(|_| ProxyStreamError::Closed)?;
        Ok(())
    }

    pub fn addr(&self) -> &crate::address::DestinationAddress {
        &self.addr
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

pub struct ServerInterruptedHttpItem {
    addr: DestinationAddress,
    req: Request<Incoming>,
    res: tokio::sync::oneshot::Sender<Response<Incoming>>,
}

impl ServerInterruptedHttpItem {
    pub async fn proxied_item(&self) -> Result<&Request<Incoming>, ProxyStreamError> {
        Ok(&self.req)
    }

    pub async fn serve(self, socket_stream: impl AsyncSocket) -> Result<(), ProxyStreamError> {
        let req = self.req;

        let (mut sender, conn) =
            hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(socket_stream))
                .await
                .or(Err(HttpError::BuildHttpReq))?;
        tokio::task::spawn(async move {
            if let Err(err) = conn.await {
                debug!("{:?}", err);
            }
        });
        let res = sender
            .send_request(req)
            .await
            .or(Err(HttpError::SendHttpReq))?;

        self.res.send(res).or(Err(HttpError::SendHttpRes))?;

        Ok(())
    }
    pub async fn replay_error(self, error: crate::ReplayStatus) -> Result<(), ProxyStreamError> {
        let mut response = hyper::Response::new(self.req.into_body());
        *response.status_mut() = error.to_status_code();
        self.res
            .send(response)
            .map_err(|_| ProxyStreamError::Closed)?;
        Ok(())
    }

    pub fn addr(&self) -> &crate::address::DestinationAddress {
        &self.addr
    }
}

pub struct IncomingWrapper {
    body: Option<Incoming>,
}

impl IncomingWrapper {
    pub fn new(body: impl Into<Option<Incoming>>) -> Self {
        Self { body: body.into() }
    }
}

impl Body for IncomingWrapper {
    type Data = Bytes;

    type Error = hyper::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        match self.body {
            Some(ref mut body) => {
                let mut pinned = std::pin::pin!(body);
                pinned.as_mut().poll_frame(cx)
            }
            None => std::task::Poll::Ready(None),
        }
    }
}

impl ReplayStatus {
    pub fn to_status_code(&self) -> hyper::StatusCode {
        match self {
            ReplayStatus::Succeeded => hyper::StatusCode::OK,
            ReplayStatus::GeneralSocksServerFailure => hyper::StatusCode::INTERNAL_SERVER_ERROR,
            ReplayStatus::ConnectionNotAllowedByRuleset => hyper::StatusCode::FORBIDDEN,
            ReplayStatus::ConnectionRefused
            | ReplayStatus::NetworkUnreachable
            | ReplayStatus::HostUnreachable => hyper::StatusCode::BAD_GATEWAY,

            ReplayStatus::TtlExpired => hyper::StatusCode::GATEWAY_TIMEOUT,
            ReplayStatus::AddressTypeNotSupported | ReplayStatus::CommandNotSupported => {
                hyper::StatusCode::NOT_IMPLEMENTED
            }
        }
    }
}
