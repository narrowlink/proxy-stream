use std::error::Error;

use proxy_stream::{address::DestinationAddress, ProxyStream, ProxyType};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:1080")
        .await
        .unwrap();
    loop {
        let stream = listener.accept().await?;
        let x = ProxyStream::new(ProxyType::SOCKS5).accept(stream.0).await?;
        let socket = match &x.addr {
            DestinationAddress::Domain(host, port) => {
                TcpStream::connect(format!("{}:{}", host, port)).await
            }
            DestinationAddress::Ip(addr) => TcpStream::connect(addr).await,
        }
        .unwrap();
        x.serve(socket).await.unwrap();
        // dbg!(x.addr);
    }
    // Ok(())
}
