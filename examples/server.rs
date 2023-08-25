use std::error::Error;

use proxy_stream::{address::DestinationAddress, ProxyStream, ProxyType};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:1080").await?;
    loop {
        let stream = listener.accept().await?;
        let interrupted_stream = ProxyStream::new(ProxyType::SOCKS5).accept(stream.0).await?;
        let Ok(socket) = (match interrupted_stream.addr() {
            DestinationAddress::Domain(host, port) => {
                TcpStream::connect(format!("{}:{}", host, port)).await
            }
            DestinationAddress::Ip(addr) => TcpStream::connect(addr).await,
        }) else {
            interrupted_stream
                .replay_error(proxy_stream::ReplayError::GeneralSocksServerFailure)
                .await?;
            continue;
        };
        interrupted_stream.serve(socket).await?;
    }
}
