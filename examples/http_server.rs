use std::error::Error;

use proxy_stream::Http;
use proxy_stream::{DestinationAddress, HttpConfig};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    loop {
        let stream = listener.accept().await?;
        let mut http = Http::new_server(HttpConfig::default(), stream.0);
        tokio::spawn(async move {
            let http_stream = match http.accept().await {
                Ok(http_stream) => http_stream,
                Err(e) => {
                    eprintln!("{}", e);
                    return;
                }
            };
            let Ok(socket) = (match http_stream.addr() {
                DestinationAddress::Domain(host, port) => {
                    TcpStream::connect(format!("{}:{}", host, port)).await
                }
                DestinationAddress::Ip(addr) => TcpStream::connect(addr).await,
            }) else {
                return;
            };
            if let Err(e) = http_stream.serve(socket).await {
                eprintln!("{}", e);
            };
        });
    }
}
