use std::num::NonZeroUsize;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};

pub(crate) struct Network {
    stream: TcpStream,
    buf: NetworkBuffer,
    config: NetworkConfig,
}

impl Network {
    pub(crate) fn new(stream: TcpStream, config: NetworkConfig) -> Self {
        Network {
            stream,
            buf: NetworkBuffer::new(config.buf_size),
            config,
        }
    }

    pub(crate) async fn read(&mut self) -> ReadResult {
        // Read from free buffer space, limit to timeout length.
        let n = match timeout(
            self.config.timeout,
            self.stream.read(&mut self.buf.storage[self.buf.filled..]),
        )
        .await
        {
            Ok(Err(_)) => return ReadResult::IoError,
            Err(_) => return ReadResult::Timeout,
            Ok(Ok(0)) => return ReadResult::NoData,
            Ok(Ok(n)) => n,
        };

        if self.buf.filled + n == self.buf.storage.len() {
            self.buf.filled += n;
            return ReadResult::BufferFull;
        }

        self.buf.filled += n;
        ReadResult::Data
    }

    pub(crate) async fn write(&mut self, buf: &[u8]) -> tokio::io::Result<()> {
        self.stream.write_all(buf).await?;
        self.stream.flush().await?;

        Ok(())
    }

    #[inline]
    pub(crate) fn data(&self) -> &[u8] {
        &self.buf.storage[..self.buf.filled]
    }

    #[inline]
    pub(crate) fn reset(&mut self, pos: usize) {
        self.buf.shift(pos);
    }
}

pub(crate) enum ReadResult {
    IoError,
    NoData,
    Timeout,
    BufferFull,
    Data,
}

#[derive(Copy, Clone)]
/// Determines read timeout and size of network buffer.
///
/// Both buffer size and timeout are placed upon every connection. They are initialized once and
/// fixed for the rest of the lifetime of the server.
///
/// # Timeout
/// If the client times out then the server will disconnect them and free up the memory for other
/// clients. The timeout is important to prevent attacks where a user sends data very slowly to
/// waste memory.
///
/// # Buffer size
/// The buffer size determines what the user can send, as if the buffer is too small then a large
/// message is not able to be formed. Make sure the buffer size is as large as the message you are
/// expecting to receive.
///
/// # Future Considerations
/// 1. Eventually, each client will have their own config, updated in real time based on context
/// 2. There will be more security features added, such as max retries or blocked IPs
pub struct NetworkConfig {
    timeout: Duration,
    buf_size: NonZeroUsize,
}

impl NetworkConfig {
    /// Initializes NetworkConfig.
    ///
    /// Timeout can be any chosen unit supported by std::time::Duration.
    /// The buffer size must be greater than 0.
    ///
    /// # Panics
    /// If buf_size is 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// let config = polaris::NetworkConfig::new(Duration::from_millis(2300), 8192);
    /// ```
    pub fn new(timeout: Duration, buf_size: usize) -> Self {
        NetworkConfig {
            timeout,
            buf_size: NonZeroUsize::new(buf_size).expect("buf_size must be non-zero"),
        }
    }
}

struct NetworkBuffer {
    storage: Vec<u8>,
    filled: usize,
}

impl NetworkBuffer {
    fn new(size: NonZeroUsize) -> Self {
        Self {
            storage: vec![0u8; size.into()],
            filled: 0,
        }
    }

    fn shift(&mut self, pos: usize) {
        assert!(pos <= self.filled, "pos exceeds filled bytes");
        self.storage.copy_within(pos.., 0);
        self.filled -= pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_moves_bytes_forward() {
        let mut buf = NetworkBuffer::new(NonZeroUsize::new(8).unwrap());
        buf.storage.copy_from_slice(b"abcdefgh");
        buf.filled = 8;

        buf.shift(3);
        assert_eq!(&buf.storage[..buf.filled], b"defgh");
    }

    #[test]
    fn shift_zero_does_nothing() {
        let mut buf = NetworkBuffer::new(NonZeroUsize::new(4).unwrap());
        buf.storage.copy_from_slice(b"abcd");
        buf.filled = 4;

        buf.shift(0);
        assert_eq!(&buf.storage[..buf.filled], b"abcd");
    }

    #[test]
    #[should_panic]
    fn shift_past_filled_panics() {
        let mut buf = NetworkBuffer::new(NonZeroUsize::new(4).unwrap());
        buf.storage.copy_from_slice(b"abcd");
        buf.filled = 4;

        buf.shift(5);
    }
}
