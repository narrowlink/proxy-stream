use std::error::Error;

use proxy_stream::Socks5;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let stream = tokio::net::TcpStream::connect("127.0.0.1:1080").await?;
    let mut socks = Socks5::new_client(proxy_stream::SocksConfig::default(), stream);
    let mut stream = socks
        .connect("www.narrowlink.com:80")
        .await?
        .proxied_stream()
        .await?;
    stream
        .write_all("GET / HTTP/1.0\r\nHost: www.narrowlink.com\r\n\r\n".as_bytes())
        .await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    println!("{}", String::from_utf8_lossy(&buf));
    Ok(())
}
