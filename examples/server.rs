use std::error::Error;

use proxy_stream::{DestinationAddress, InterruptedStream};
use proxy_stream::{ProxyStream, Socks5};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:1080").await?;
    loop {
        let stream = listener.accept().await?;
        let socks = Socks5::default();
        let socks_stream = socks.accept(stream.0).await?;
        let Ok(socket) = (match socks_stream.addr() {
            DestinationAddress::Domain(host, port) => {
                TcpStream::connect(format!("{}:{}", host, port)).await
            }
            DestinationAddress::Ip(addr) => TcpStream::connect(addr).await,
        }) else {
            continue;
        };
        socks_stream.serve(socket).await?;
    }
}
