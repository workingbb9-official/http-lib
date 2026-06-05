use log::{info, warn};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use crate::network::{Network, ReadResult};
use crate::protocol::Framing;
use crate::protocol::Protocol;

/// Configures connections for the server.
///
/// This struct is attached to the server and it will be used for each client. For now, it is
/// created once when the server is initialized, and cannot change. Eventually each client will
/// have their own configuration, allowing the user to modify it through the protocol.
pub struct ServerConfig {
    buf_size: usize,
    timeout: Duration,
    max_clients: usize,
}

impl ServerConfig {
    /// Create config with defaults.
    ///
    /// Max clients defaults to 100 clients.
    /// Buffer size defaults to 4096 bytes.
    /// Timeout defaults to 5 seconds.
    pub fn new() -> Self {
        Self {
            max_clients: 100,
            buf_size: 4096,
            timeout: Duration::from_secs(5),
        }
    }

    /// Sets buffer size for reading from network.
    ///
    /// The network module will allocate this much upfront for each client. Size is fixed for all
    /// clients, so ensure this is as large as the max payload the server is expected to receive.
    pub fn buf_size(mut self, n: usize) -> Self {
        self.buf_size = n;
        self
    }

    /// Sets amount of time to wait for bytes from client.
    ///
    /// This is used in conjunction with 'timeout()' function from Tokio. It can be in any unit
    /// supported by 'std::time::Duration'. If the user times out, the client will be dropped. This
    /// is important to prevent clients from staying connected while inactive.
    pub fn timeout(mut self, n: Duration) -> Self {
        self.timeout = n;
        self
    }

    /// Sets max number of clients server will stay connected to.
    ///
    /// The server will update the number of current clients whenever one connects or disconnects.
    /// If the count is at 'max_clients', no new clients will connect until someone else
    /// disconnects from the server.
    ///
    /// # Performance
    /// CPU time switching between tokio tasks will limit speed of service, with an increased time
    /// proportional to amount of clients. Bandwith is also an important limiter, as each client
    /// will take up a portion of total bandwidth.
    ///
    /// # Memory
    /// Each connection allocates 'buf_size' plus kernel TCP overhead (4-8KB depending on OS).
    /// Consider system RAM that can be set aside for the server when setting 'max_clients'.
    pub fn max_clients(mut self, n: usize) -> Self {
        self.max_clients = n;
        self
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            max_clients: 100,
            buf_size: 4096,
            timeout: Duration::from_secs(5),
        }
    }
}

/// Connects to clients and runs through event loop.
///
/// Protocol stays generic, but server calls the main functions.
/// Each connection loop: read, parse, route, serialize, send.
pub struct Server<P: Protocol> {
    listener: TcpListener,
    config: ServerConfig,
    protocol: P,
    clients: AtomicUsize,
}

impl<P: Protocol + std::marker::Sync + 'static> Server<P> {
    /// Create a new server.
    ///
    /// This will create a tcp listener from the address string. Both [ServerConfig] and protocol
    /// are used for all connections.
    ///
    /// # Panics
    /// If addr is not able to be converted into a SocketAddr. This will print the panic message
    /// 'Invalid address'. Also panics if the buf_size of [ServerConfig] is zero.
    ///
    /// # Examples
    ///
    /// ```
    /// #[tokio::main]
    /// async fn main() {
    ///     use std::time::Duration;
    ///
    ///     let config = http_lib::ServerConfig::new()
    ///         .max_clients(300)
    ///         .buf_size(8192)
    ///         .timeout(Duration::from_millis(3500));
    ///
    ///     let protocol = http_lib::HttpProtocol::new(None::<()>);
    ///
    ///     // Use localhost (connect to same machine)
    ///     let server = http_lib::Server::new("127.0.0.1:0", config, protocol)
    ///         .await
    ///         .expect("Failed to create server");
    /// }
    /// ```
    pub async fn new(addr: &str, config: ServerConfig, protocol: P) -> tokio::io::Result<Self> {
        assert!(config.buf_size > 0, "Buffer size must be larger than zero");

        let sock: SocketAddr = addr.parse().expect("Invalid address");
        let listener = TcpListener::bind(sock).await?;

        Ok(Self {
            listener,
            config,
            protocol,
            clients: AtomicUsize::new(0),
        })
    }

    /// Connect to clients and spawn a task.
    ///
    /// This starts a loop of trying to connect to a client. Calls Arc::clone() on the server and
    /// moves the connection to the task. This keeps the server async and responsive. Will only
    /// accept a connection if the number of connected clients is less than 'max_clients'
    ///
    /// # Errors
    /// Propagates error from accepting on TcpListener.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::time::Duration;
    /// use std::sync::Arc;
    /// use log::warn;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let config = http_lib::ServerConfig::new()
    ///         .max_clients(300)
    ///         .buf_size(8192)
    ///         .timeout(Duration::from_millis(3500));
    ///
    ///     let protocol = http_lib::HttpProtocol::new(None::<()>);
    ///     let server = http_lib::Server::new("127.0.0.1:8080", config, protocol)
    ///         .await
    ///         .expect("Failed to create server");
    ///
    ///     let server = Arc::new(server);
    ///
    ///     loop {
    ///         let server_ptr = Arc::clone(&server);
    ///         if let Err(e) = server_ptr.run().await {
    ///             warn!("Failed to accept with error: {}", e);
    ///         }
    ///
    ///         // Reaches here if a client has connected.
    ///         println!("Client connected");
    ///     }
    ///
    ///     // Go to 127.0.0.1:8080 in browser to test it.
    /// }
    /// ```
    pub async fn run(self: Arc<Self>) -> tokio::io::Result<()> {
        let (mut stream, _) = self.listener.accept().await?;

        let accepted = self
            .clients
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |c| {
                if c < self.config.max_clients {
                    Some(c + 1)
                } else {
                    None
                }
            });

        if accepted.is_err() {
            info!("Max clients reached, rejecting connection");
            let _ = stream.shutdown().await;
            return Ok(());
        }

        info!(
            "Client connected ({}/{})",
            self.clients.load(Ordering::Relaxed),
            self.config.max_clients,
        );

        let server_ptr = Arc::clone(&self);
        tokio::spawn(async move {
            server_ptr.handle_connection(stream).await;
        });

        Ok(())
    }

    /// Access the SocketAddr the server is listening on.
    ///
    /// # Panics
    /// If accessing local address of TcpListener returns error. This could happen if socket is
    /// closed, or an OS-level error. If this happens, the server will be closed since its
    /// listener is corrupt.
    pub fn local_addr(&self) -> SocketAddr {
        self.listener.local_addr().unwrap()
    }

    async fn handle_connection(&self, stream: TcpStream) {
        let network = Network::new(stream, self.config.buf_size, self.config.timeout);

        self.connection_loop(network).await;

        self.clients.fetch_sub(1, Ordering::Relaxed);
        info!("Dropping connection");
    }

    async fn connection_loop(&self, mut network: Network) -> Option<()> {
        loop {
            let raw = self.net_read(&mut network).await?;
            let msg = match self.protocol.parse(raw) {
                Some(msg) => msg,
                None => todo!(),
            };
            let outcome = self.protocol.route(msg);
            let response = self.protocol.serialize(outcome);
            network.write(&response).await.ok()?;
        }
    }

    /// Reads from network and logs if message was not found.
    /// Will reset the network buffer if bytes were written.
    /// Mainly collects bytes and passes it to handle_frame().
    async fn net_read(&self, network: &mut Network) -> Option<Vec<u8>> {
        if let Framing::Http = self.protocol.framing() {
            return self.net_read_http(network).await;
        }

        loop {
            match network.read().await {
                ReadResult::NoData => {
                    info!("Received no data");
                    return None;
                }
                ReadResult::Timeout => {
                    info!("Connection timed out");
                    return None;
                }
                ReadResult::IoError => {
                    warn!("IO error when reading");
                    return None;
                }
                ReadResult::BufferFull => {
                    if let Some((vec, pos)) = handle_frame(&self.protocol.framing(), network.data())
                    {
                        network.reset(pos);
                        return Some(vec);
                    }

                    info!("Buffer full, frame not found");
                    return None;
                }
                ReadResult::Data => {
                    if let Some((vec, pos)) = handle_frame(&self.protocol.framing(), network.data())
                    {
                        network.reset(pos);
                        return Some(vec);
                    }
                }
            };
        }
    }

    /// Identical to net_read(), but extra loop for Http framing.
    /// Finds delimiter (\r\n\r\n), then content length.
    /// Using content length, reads the rest of the data.
    async fn net_read_http(&self, network: &mut Network) -> Option<Vec<u8>> {
        loop {
            match network.read().await {
                ReadResult::NoData => {
                    info!("Received no data");
                    return None;
                }
                ReadResult::Timeout => {
                    info!("Connection timed out");
                    return None;
                }
                ReadResult::IoError => {
                    warn!("IO error when reading");
                    return None;
                }
                ReadResult::BufferFull => {
                    if find_delimiter(network.data(), b"\r\n\r\n").is_some() {
                        break;
                    }

                    info!("Buffer full, frame not found");
                    return None;
                }
                ReadResult::Data => {
                    if find_delimiter(network.data(), b"\r\n\r\n").is_some() {
                        break;
                    }
                }
            }
        }

        let content_len = extract_content_length(network.data());
        let header_end = find_delimiter(network.data(), b"\r\n\r\n")?;
        let total = header_end + content_len;

        while network.data().len() < total {
            match network.read().await {
                ReadResult::NoData => return None,
                ReadResult::Timeout => {
                    info!("Connection timed out");
                    return None;
                }
                ReadResult::IoError => {
                    warn!("IO error when reading");
                    return None;
                }
                ReadResult::BufferFull => {
                    info!("Buffer full, body not complete");
                    return None;
                }
                ReadResult::Data => (),
            }
        }

        let msg = network.data()[..total].to_vec();
        network.reset(total);
        Some(msg)
    }
}

/// Returns index directly after delimiter.
fn find_delimiter(buf: &[u8], delimiter: &[u8]) -> Option<usize> {
    let len = delimiter.len();
    buf.windows(len)
        .position(|w| w == delimiter)
        .map(|i| i + len)
}

/// Returns full message and position where it ended.
/// Will return None if not enough bytes were read, or delimiter not found.
fn handle_frame(framing: &Framing, buf: &[u8]) -> Option<(Vec<u8>, usize)> {
    match framing {
        Framing::Delimiter(d) => {
            let idx = find_delimiter(buf, d)?;
            let len = idx.saturating_sub(d.len());
            Some((buf[..len].to_vec(), idx))
        }
        Framing::ExactBytes(n) => {
            if buf.len() < *n {
                return None;
            }

            Some((buf[..*n].to_vec(), *n))
        }
        Framing::Http => {
            warn!("handle_frame() used for Http");
            None
        }
    }
}

/// Returns 0 if content length not found.
fn extract_content_length(headers: &[u8]) -> usize {
    let key = b"Content-Length: ";
    let pos = match headers.windows(key.len()).position(|w| w == key) {
        Some(p) => p,
        None => return 0,
    };
    let start = pos + key.len();
    let end = match headers[start..].iter().position(|&b| b == b'\r') {
        Some(e) => e + start,
        None => return 0,
    };
    std::str::from_utf8(&headers[start..end])
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_delimiter_in_middle_returns_index() {
        let buf: &[u8] = b"find$%_delimiter_inthis^&";
        let result = find_delimiter(buf, b"delimiter");
        assert_eq!(result, Some(16));
    }

    #[test]
    fn find_delimiter_at_start_returns_index() {
        let buf: &[u8] = b"delimiter@$_start";
        let result = find_delimiter(buf, b"delimiter");
        assert_eq!(result, Some(9));
    }

    #[test]
    fn find_delimiter_at_end_returns_index() {
        let buf: &[u8] = b"@TheEnd$is_thedelimiter";
        let result = find_delimiter(buf, b"delimiter");
        assert_eq!(result, Some(23));
    }

    #[test]
    fn find_delimiter_not_found_returns_none() {
        let buf: &[u8] = b"$oDelimInThis*ne";
        let result = find_delimiter(buf, b"delimiter");
        assert_eq!(result, None);
    }

    #[test]
    fn find_delimiter_empty_buffer_returns_none() {
        let buf: &[u8] = b"";
        let result = find_delimiter(buf, b"delimiter");
        assert_eq!(result, None);
    }

    #[test]
    fn delimiter_framing_returns_pos() {
        let buf = b"HttpMessage\r\n\r\nMoreStuff";

        let result = handle_frame(&Framing::Delimiter(b"\r\n\r\n"), buf);
        assert_eq!(result, Some((buf[..11].to_vec(), 15)));
    }

    #[test]
    fn delimiter_framing_no_delimiter() {
        let buf = b"ThereIsNoDelimiter";

        let result = handle_frame(&Framing::Delimiter(b"\r\n\r\n"), buf);
        assert_eq!(result, None);
    }

    #[test]
    fn exact_bytes_framing_returns_bytes() {
        let buf = b"ThisIs17BytesLong";

        let result = handle_frame(&Framing::ExactBytes(13), buf);
        assert_eq!(result, Some((buf[..13].to_vec(), 13)));
    }

    #[test]
    fn exact_bytes_framing_buffer_too_short() {
        let buf = b"ShortString18Bytes";

        let result = handle_frame(&Framing::ExactBytes(20), buf);
        assert_eq!(result, None);
    }
}
