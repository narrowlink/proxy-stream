use std::error::Error;

use proxy_stream::{ProxyStream, ProxyType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let stream = tokio::net::TcpStream::connect("127.0.0.1:1080").await?;
    let mut stream = ProxyStream::new(ProxyType::SOCKS5)
        .connect(stream, "www.narrowlink.com:80")
        .await?;
    stream
        .write_all("GET / HTTP/1.0\r\n\r\n".as_bytes())
        .await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    println!("{}", String::from_utf8_lossy(&buf));
    Ok(())
}
