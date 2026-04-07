mod http;
mod web_sockets;
pub use http::HttpHandler;
pub use http::HttpMessage;
pub use http::HttpProtocol;
pub use http::HttpResponse;
pub use http::{Connection, ContentType, Status};

/// Custom protocol implementation.
///
/// Define your own Message (input) and Response (output).
/// Create event loop with parsing, routing, and serializing.
/// Server will collect bytes, implement protocol, and send bytes.
pub trait Protocol: Send {
    /// Format the client should send.
    type Message;
    /// Format the server should send.
    type Response;

    /// Access framing of the protocol.
    fn framing(&self) -> Framing;
    /// Turn raw bytes into a rigid Message object.
    /// Used to extract useful data from client.
    fn parse(&self, raw: Vec<u8>) -> Option<Self::Message>;
    /// Take message and match with action.
    /// Returns a Response to send back, based on input.
    fn route(&self, msg: Self::Message) -> Self::Response;
    /// Convert Response object back into raw bytes for sending.
    fn serialize(&self, response: Self::Response) -> Vec<u8>;
}

/// Used to determine what network should read until.
pub enum Framing {
    /// Read until this pattern is found.
    Delimiter(&'static [u8]),
    /// Read up to n bytes.
    ExactBytes(usize),
    /// Read until \r\n\r\n, find content length and read body.
    Http,
}
