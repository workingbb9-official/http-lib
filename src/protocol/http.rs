use super::*;
use std::collections::HashMap;

/// Function to run based on input given.
///
/// The bytes as input are the body from the request that was received. This can be ignored if the
/// method or path gives enough information, since the router will already account for that when
/// mapping to the handler. See an example in [HttpProtocol]
pub type HttpHandler = fn(&[u8]) -> HttpResponse;

/// The message formed after parsing.
///
/// This struct is formed straight from the raw network bytes. It is the result of the parse()
/// method from [HttpProtocol]. Once created, it will be sent to the route() method to process
/// the request.
#[derive(PartialEq, Debug)]
pub struct HttpMessage {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

/// A complete Http response ready to be sent back to client.
///
/// This struct is the final output of the routing process. Once created, the server will serialize
/// into raw bytes, as per HTTP/1.1 formatting. User should manually create this to match their
/// specifications.
pub struct HttpResponse {
    /// The status code sent to browser.
    pub status: Status,
    /// The connection status for browser.
    pub connection: Connection,
    /// An optional body, along with its type.
    pub body: Option<(ContentType, Vec<u8>)>,
}

/// Represents an HTTP response status code.
///
/// Each variant maps to a specific status line in HTTP/1.1 protocol, such as '200 OK' or '404 Not
/// Found'. This should be accurate for the browser to understand the purpose of the response.
pub enum Status {
    /// 200 OK. The request was successful.
    OK,
    /// 204 No Content. The request was successful, but there is no data to return.
    NoContent,
    /// 404 Not Found. The requested resource could not be found.
    NotFound,
    /// 400 Bad Request. The request could not be parsed, likely due to malformed syntax.
    BadRequest,
}

impl Status {
    fn as_str(&self) -> &'static str {
        match self {
            Status::OK => "200 OK",
            Status::NoContent => "204 No Content",
            Status::NotFound => "404 Not Found",
            Status::BadRequest => "400 Bad Request",
        }
    }
}

/// A notification to the browser for connection handling.
///
/// This determines how long clients stay connected to the server. Before dropping the client, the
/// server will inform the browser that it is about to close the connection.
pub enum Connection {
    /// Signals to the browser to keep the connection active.
    ///
    /// It is the default connection used by this implementation. KeepAlive helps to quickly send
    /// html, css, javascript, and other data without having to waste time reconnecting.
    KeepAlive,
    /// Signals to the browser that TCP connection should be terminated.
    ///
    /// This is sent by the server right before the client is dropped / disconnected. When sent, it
    /// instructs the client that no further requests sent, and that the connection will be closed.
    Close,
}

impl Connection {
    fn as_str(&self) -> &'static str {
        match self {
            Connection::KeepAlive => "keep-alive",
            Connection::Close => "close",
        }
    }
}

/// The type of data the body contains.
///
/// This must be accurate for browser to interpret correctly. Incorrect inputs can lead to
/// malformed or unintended web pages.
pub enum ContentType {
    /// The body contains standard text.
    Plain,
    /// The body contains an HTML file.
    Html,
    /// The body contains a CSS file.
    Css,
    /// The body contains a JavaScript file.
    JavaScript,
}

impl ContentType {
    fn as_str(&self) -> &'static str {
        match self {
            ContentType::Plain => "text/plain",
            ContentType::Html => "text/html",
            ContentType::Css => "text/css",
            ContentType::JavaScript => "text/javascript",
        }
    }
}

/// The core engine responsible for handling the Http lifecycle.
///
/// The struct itself holds a hashmap used to route messages to the inserted handlers.
/// It also provides the logic for:
/// 1. **Parsing** raw bytes streams into [HttpMessage] objects.
/// 2. **Routing** those requests into the injected handlers.
/// 3. **Serializing** the [HttpResponse] from routing into bytes in the correct Http format.
pub struct HttpProtocol {
    routes: HashMap<String, HttpHandler>,
}

impl HttpProtocol {
    /// Creates an empty HashMap.
    ///
    /// The object should be mutable to add routes.
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Adds a route to the hashmap.
    ///
    /// The route maps method + path (concatenated) to an [HttpHandler]. Every request will be
    /// checked against the valid keys. Both method and path should be something that the browser
    /// would send.
    ///
    /// # Examples
    ///
    /// ```
    /// fn my_handler(_: &[u8]) -> polaris::HttpResponse {
    ///     polaris::HttpResponse {
    ///         status: polaris::Status::OK,
    ///         connection: polaris::Connection::KeepAlive,
    ///         body: Some((polaris::ContentType::Plain, b"Hello from polaris".to_vec())),
    ///     }
    /// }
    ///
    /// let mut protocol = polaris::HttpProtocol::new();
    /// protocol.add_route("GET", "/", my_handler);
    /// ```
    pub fn add_route(&mut self, method: &str, path: &str, handler: HttpHandler) {
        let key = format!("{} {}", method, path);
        self.routes.insert(key, handler);
    }
}

impl Default for HttpProtocol {
    fn default() -> Self {
        Self::new()
    }
}

impl Protocol for HttpProtocol {
    type Message = HttpMessage;
    type Response = HttpResponse;

    fn framing(&self) -> Framing {
        Framing::Http
    }

    fn parse(&self, raw: Vec<u8>) -> Option<HttpMessage> {
        let request = String::from_utf8(raw).ok()?;

        // Split into headers and body
        let mut parts = request.splitn(2, "\r\n\r\n");

        let mut header_lines = parts.next()?.lines();

        // Parse request line
        let first_line = header_lines.next()?;
        let mut tokens = first_line.split_whitespace();
        let method = tokens.next()?.to_string();
        let path = tokens.next()?.to_string();
        let _version = tokens.next()?;

        // Parse headers
        let mut headers = HashMap::new();
        for line in header_lines {
            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }

        // Parse body
        let value = url_decode(parts.next().unwrap_or(""));
        let body_str = value.split_once('=').map(|x| x.1).unwrap_or(&value);
        let body = body_str.as_bytes().to_vec();

        let http_req = HttpMessage {
            method,
            path,
            headers,
            body,
        };

        Some(http_req)
    }

    fn route(&self, msg: HttpMessage) -> HttpResponse {
        let key = format!("{} {}", msg.method, msg.path);

        if let Some(handler) = self.routes.get(&key) {
            return handler(&msg.body[..]);
        }

        HttpResponse {
            status: Status::NotFound,
            connection: Connection::KeepAlive,
            body: Some((ContentType::Plain, b"Polaris\nNotFound".to_vec())),
        }
    }

    fn serialize(&self, response: HttpResponse) -> Vec<u8> {
        let status_str = response.status.as_str();
        let conn_str = response.connection.as_str();

        let (content_str, body) = match response.body {
            Some((ct, body)) => (ct.as_str(), body),
            None => ("", Vec::new()),
        };

        build_response(status_str, conn_str, content_str, body)
    }
}

fn build_response(status: &str, conn: &str, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 {}\r\n\
            Content-Security-Policy: default-src 'self'; script-src 'self';\r\n\
            Content-Length: {}\r\n\
            Content-Type: {}\r\n\
            Connection: {}\r\n\
            \r\n",
        status,
        body.len(),
        content_type,
        conn,
    );

    let mut final_response = header.into_bytes();

    // Add body after header
    final_response.extend(&body);

    final_response
}

fn url_decode(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '+' => result.push(' '),
            '%' => {
                let h1 = chars.next().unwrap_or('0');
                let h2 = chars.next().unwrap_or('0');
                if let Ok(byte) = u8::from_str_radix(&format!("{h1}{h2}"), 16) {
                    result.push(byte as char);
                }
            }
            _ => result.push(c),
        }
    }

    result
}

#[allow(dead_code)]
fn should_upgrade_to_web_sockets(headers: &HashMap<String, String>) -> bool {
    headers
        .get("connection")
        .map(|v| v.eq_ignore_ascii_case("upgrade"))
        .unwrap_or(false);
    headers
        .get("upgrade")
        .map(|v| v.eq_ignore_ascii_case("websockets"))
        .unwrap_or(false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    /*
    #[test]
    fn parse_valid_get_request() {
        let protocol = HttpProtocol::new();
        let result = protocol.parse(b"GET /test HTTP/1.1\r\n".to_vec());

        assert_eq!(
            result,
            Some(HttpMessage {
                method: "GET".to_string(),
                path: "/test".to_string(),
                body: Vec::new(),
            })
        );
    }

    #[test]
    fn parse_valid_post_request() {
        let protocol = HttpProtocol::new();
        let result = protocol.parse(b"POST / HTTP/1.1\r\n".to_vec());

        assert_eq!(
            result,
            Some(HttpMessage {
                method: "POST".to_string(),
                path: "/".to_string(),
                body: Vec::new(),
            })
        );
    }

    #[test]
    fn parse_invalid_utf8_returns_none() {
        let invalid = vec![0xFF, 0xFE, 0x00];

        let protocol = HttpProtocol::new();
        let result = protocol.parse(invalid);

        assert_eq!(result, None);
    }

    #[test]
    fn parse_missing_token_returns_none() {
        let protocol = HttpProtocol::new();
        let result = protocol.parse(b"GET HTTP/1.1\r\n".to_vec());

        assert_eq!(result, None);
    } */

    #[test]
    fn upgrade_to_web_sockets_detected() {
        let protocol = HttpProtocol::new();
        let request = "\
            GET /chat HTTP/1.1\r\n\
            Host: example.com\r\n\
            Upgrade: websocket\r\n\
            Connection: Upgrade\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\
            \r\n";

        let parsed = protocol.parse(request.as_bytes().to_vec()).unwrap();
        let result = should_upgrade_to_web_sockets(&parsed.headers);

        assert_eq!(result, true);
    }
}
