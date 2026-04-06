use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};

pub(crate) struct Network {
    stream: TcpStream,
    buf: NetworkBuffer,
    timeout: Duration,
}

impl Network {
    pub(crate) fn new(stream: TcpStream, buf_size: usize, timeout: Duration) -> Self {
        Network {
            stream,
            buf: NetworkBuffer::new(buf_size),
            timeout,
        }
    }

    pub(crate) async fn read(&mut self) -> ReadResult {
        // Read from free buffer space, limit to timeout length.
        let n = match timeout(
            self.timeout,
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

struct NetworkBuffer {
    storage: Vec<u8>,
    filled: usize,
}

impl NetworkBuffer {
    fn new(size: usize) -> Self {
        Self {
            storage: vec![0u8; size],
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
        let mut buf = NetworkBuffer::new(8);
        buf.storage.copy_from_slice(b"abcdefgh");
        buf.filled = 8;

        buf.shift(3);
        assert_eq!(&buf.storage[..buf.filled], b"defgh");
    }

    #[test]
    fn shift_zero_does_nothing() {
        let mut buf = NetworkBuffer::new(4);
        buf.storage.copy_from_slice(b"abcd");
        buf.filled = 4;

        buf.shift(0);
        assert_eq!(&buf.storage[..buf.filled], b"abcd");
    }

    #[test]
    #[should_panic]
    fn shift_past_filled_panics() {
        let mut buf = NetworkBuffer::new(8);
        buf.storage.copy_from_slice(b"abcd");
        buf.filled = 4;

        buf.shift(5);
    }
}
