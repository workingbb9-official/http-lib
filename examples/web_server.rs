use log::warn;
use std::{fs, time::Duration};
use std::sync::{Arc, Mutex};

use http_lib::{Connection, ContentType, HttpProtocol, HttpResponse, Status};
use http_lib::{Server, ServerConfig};

#[tokio::main]
async fn main() {
    env_logger::init();
    let state = AppState {
        home_visitors: Mutex::new(0),
        about_visitors: Mutex::new(0),
    };

    let port = "127.0.0.1:8080";

    let config = ServerConfig::new()
        .max_clients(25000)
        .buf_size(8192)
        .timeout(Duration::from_secs(5));

    let mut protocol = HttpProtocol::new(Some(state));
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

    loop {
        let server_ptr = Arc::clone(&server);
        if let Err(e) = server_ptr.run().await {
            warn!("Failed to accept with error: {}", e);
        }
    }
}

struct AppState {
    home_visitors: Mutex<usize>,
    about_visitors: Mutex<usize>,
}

fn home_html(_: &[u8], state: Option<&Arc<AppState>>) -> HttpResponse {
    if let Some(s) = state {
        let mut guard = s.home_visitors.lock().unwrap();
        *guard += 1;
    }

    let bytes = fs::read("examples/static/index.html").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::Html, bytes)),
    }
}

fn home_css(_: &[u8], _: Option<&Arc<AppState>>) -> HttpResponse {
    let bytes = fs::read("examples/static/style.css").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::Css, bytes)),
    }
}

fn home_js(_: &[u8], _: Option<&Arc<AppState>>) -> HttpResponse {
    let bytes = fs::read("examples/static/script.js").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::JavaScript, bytes)),
    }
}

fn about_html(_: &[u8], state: Option<&Arc<AppState>>) -> HttpResponse {
    if let Some(s) = state {
        let mut guard = s.about_visitors.lock().unwrap();
        *guard += 1;
    }

    let bytes = fs::read("examples/static/about.html").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::Html, bytes)),
    }
}

fn about_js(_: &[u8], _: Option<&Arc<AppState>>) -> HttpResponse {
    let bytes = fs::read("examples/static/about.js").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::JavaScript, bytes)),
    }
}

fn post_html(_: &[u8], _: Option<&Arc<AppState>>) -> HttpResponse {
    let bytes = fs::read("examples/static/post.html").unwrap();
    HttpResponse {
        status: Status::OK,
        connection: Connection::KeepAlive,
        body: Some((ContentType::Html, bytes)),
    }
}

fn display_post(body: &[u8], _: Option<&Arc<AppState>>) -> HttpResponse {
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
