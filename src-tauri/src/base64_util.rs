use base64::{engine::general_purpose::STANDARD as B64, Engine as _};

/// Encode a byte slice into a base64 string for the frontend's xterm.js write().
pub fn base64_encode(bytes: &[u8]) -> String { B64.encode(bytes) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn encode_roundtrips_arbitrary_bytes() {
        let bytes: Vec<u8> = (0u8..=255).collect();
        let decoded = B64.decode(base64_encode(&bytes)).unwrap();
        assert_eq!(decoded, bytes);
    }
}
