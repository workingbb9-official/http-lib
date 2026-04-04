use log::warn;
use std::{fs, sync::Arc, time::Duration};

use polaris::{Connection, ContentType, HttpProtocol, HttpResponse, Status};
use polaris::{NetworkConfig, Server};

#[tokio::main]
async fn main() {
    env_logger::init();

    let port = "127.0.0.1:8080";

    let config = NetworkConfig::new(Duration::from_secs(5), 8192);

    let mut protocol = HttpProtocol::new();
    protocol.add_route("GET", "/", home_html);
    protocol.add_route("GET", "/style.css", home_css);
    protocol.add_route("GET", "/script.js", home_js);
    protocol.add_route("GET", "/about", about_html);
    protocol.add_route("GET", "/about.js", about_js);
    protocol.add_route("GET", "/post", post_html);
    protocol.add_route("POST", "/post", display_post);

    let server = Server::new(port, config, protocol)
        .await
        .expect("Failed to create server");

    let server = Arc::new(server);
    let mut count = 0;

    loop {
        let server_ptr = Arc::clone(&server);
        if let Err(e) = server_ptr.run().await {
            warn!("Failed to accept with error: {}", e);
        }

        count += 1;
        if count % 10 == 0 {
            println!("Total visits: {count}");
        }
    }
}

fn home_html(_: &[u8]) -> HttpResponse {
    let bytes = fs::read("examples/static/index.html").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::Html, bytes)),
    }
}

fn home_css(_: &[u8]) -> HttpResponse {
    let bytes = fs::read("examples/static/style.css").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::Css, bytes)),
    }
}

fn home_js(_: &[u8]) -> HttpResponse {
    let bytes = fs::read("examples/static/script.js").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::JavaScript, bytes)),
    }
}

fn about_html(_: &[u8]) -> HttpResponse {
    let bytes = fs::read("examples/static/about.html").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::Html, bytes)),
    }
}

fn about_js(_: &[u8]) -> HttpResponse {
    let bytes = fs::read("examples/static/about.js").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::JavaScript, bytes)),
    }
}

fn post_html(_: &[u8]) -> HttpResponse {
    let bytes = fs::read("examples/static/post.html").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::Html, bytes)),
    }
}

fn display_post(body: &[u8]) -> HttpResponse {
    let sanitized: String = body
        .iter()
        .map(|&b| if b.is_ascii_control() { '.' } else { b as char })
        .collect();
    println!("POST body received: {}", sanitized);

    HttpResponse {
        status: Status::NoContent,
        connection: Connection::KeepAlive,
        body: None,
    }
}
