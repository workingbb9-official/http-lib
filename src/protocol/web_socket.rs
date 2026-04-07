use base64::{Engine, engine::general_purpose};
use sha1::{Digest, Sha1};

pub struct WebSocketProtocol;

impl WebSocketProtocol {
    #[allow(dead_code)]
    pub(crate) fn generate_accept_key(client_key: &str) -> String {
        const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

        let mut hasher = Sha1::new();
        hasher.update(client_key.as_bytes());
        hasher.update(GUID.as_bytes());

        let result = hasher.finalize();

        general_purpose::STANDARD.encode(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_generated_is_valid() {
        const CLIENT_KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
        const EXPECTED: &str = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";

        let result = WebSocketProtocol::generate_accept_key(CLIENT_KEY);

        assert_eq!(&result, EXPECTED);
    }
}
