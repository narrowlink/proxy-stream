use std::error::Error;

use proxy_stream::DestinationAddress;
use proxy_stream::Socks5;
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:1080").await?;
    loop {
        let stream = listener.accept().await?;
        let mut socks = Socks5::new_server(proxy_stream::SocksConfig::default(), stream.0);
        tokio::spawn(async move {
            let socks_stream = match socks.accept().await {
                Ok(socks_stream) => socks_stream,
                Err(e) => {
                    eprintln!("{}", e);
                    return;
                }
            };
            let Ok(socket) = (match socks_stream.addr() {
                DestinationAddress::Domain(host, port) => {
                    TcpStream::connect(format!("{}:{}", host, port)).await
                }
                DestinationAddress::Ip(addr) => TcpStream::connect(addr).await,
            }) else {
                return;
            };
            if let Err(e) = socks_stream.serve(socket).await {
                eprintln!("{}", e);
            };
        });
    }
}
