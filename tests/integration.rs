use std::{net::SocketAddr, sync::Arc, time::Duration};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use polaris::{Connection, ContentType, HttpProtocol, HttpResponse, Status};
use polaris::{Server, ServerConfig};

async fn spawn_test_server() -> SocketAddr {
    let config = ServerConfig::new()
        .max_clients(5)
        .buf_size(8192)
        .timeout(Duration::from_millis(100));

    let mut protocol = HttpProtocol::new();
    protocol.add_route("GET", "/", |_| HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::Plain, b"hello".to_vec())),
    });

    // Port 0 lets OS pick at random, to avoid conflict between tests
    let server = Server::new("127.0.0.1:0", config, protocol)
        .await
        .expect("Failed to create server");

    let addr = server.local_addr();

    let server = Arc::new(server);

    tokio::spawn(async move {
        loop {
            Arc::clone(&server).run().await.unwrap();
        }
    });

    addr
}

#[tokio::test]
async fn get_known_route_returns_200() {
    let addr = spawn_test_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("200 OK"));
    assert!(response.contains("hello"));
}

#[tokio::test]
async fn get_unknown_route_returns_404() {
    let addr = spawn_test_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET /fake HTTP/1.1\r\n\r\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("404 Not Found"));
}

#[tokio::test]
async fn no_delimiter_times_out() {
    let addr = spawn_test_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(b"WastingYourTime\r\n").await.unwrap();

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();

    assert_eq!(n, 0);
}

#[tokio::test]
async fn server_caps_clients() {
    let addr = spawn_test_server().await;

    let mut clients = Vec::new();

    for _ in 0..5 {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();

        let mut buf = vec![0u8; 1024];
        stream.read(&mut buf).await.unwrap();

        clients.push(stream);
    }

    let mut extra_client = TcpStream::connect(addr).await.unwrap();
    extra_client
        .write_all(b"GET / HTTP/1.1\r\n\r\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 1024];
    let n = extra_client.read(&mut buf).await.unwrap();

    assert_eq!(n, 0);
}

#[tokio::test]
async fn web_socket_upgrade() {
    let addr = spawn_test_server().await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(b"GET / HTTP/1.1\r\n\
        Host: localhost\r\n\
        Upgrade: websocket\r\n\
        Connection: Upgrade\r\n\
        Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
        Sec-WebSocket-Version: 13\r\n\
        \r\n"
    ).await.unwrap();

    let mut buf = vec![0u8; 1024];
    let n = stream.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(response.contains("Switching Protocols"));
    assert!(response.contains("Connection: Upgrade"));
    assert!(response.contains("Upgrade: WebSocket"));
    assert!(response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
}
