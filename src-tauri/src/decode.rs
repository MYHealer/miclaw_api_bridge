//! Incremental UTF-8 decoding for byte streams.
//!
//! Upstream SSE arrives as arbitrary network chunks; a multi-byte character
//! (e.g. a Chinese glyph in mimo's reasoning trace) can be split across two
//! chunks. Decoding each chunk independently with `String::from_utf8_lossy`
//! replaces the split bytes with U+FFFD (`�`) and corrupts the text. This
//! decoder buffers a trailing incomplete sequence until the rest arrives.

/// Stateful UTF-8 decoder that retains an incomplete trailing byte sequence
/// between `push` calls.
#[derive(Default)]
pub struct Utf8Stream {
    carry: Vec<u8>,
}

impl Utf8Stream {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `bytes` and return all text that can be fully decoded so far.
    /// A trailing partial multi-byte sequence is kept for the next call;
    /// genuinely invalid bytes are emitted as U+FFFD.
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.carry.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.carry) {
                Ok(s) => {
                    out.push_str(s);
                    self.carry.clear();
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    if valid > 0 {
                        // SAFETY: bytes [..valid] are valid UTF-8 per the error.
                        out.push_str(unsafe { std::str::from_utf8_unchecked(&self.carry[..valid]) });
                    }
                    match e.error_len() {
                        // Incomplete trailing sequence: keep it for next push.
                        None => {
                            self.carry.drain(..valid);
                            break;
                        }
                        // Invalid byte(s): emit replacement and skip past them.
                        Some(bad) => {
                            out.push('\u{FFFD}');
                            self.carry.drain(..valid + bad);
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reassembles_multibyte_split_across_chunks() {
        // "思考" = E6 80 9D  E8 80 83
        let full = "思考".as_bytes();
        let mut d = Utf8Stream::new();
        // split the first character (E6 80 | 9D ...)
        let mut got = String::new();
        got.push_str(&d.push(&full[..2]));
        got.push_str(&d.push(&full[2..4]));
        got.push_str(&d.push(&full[4..]));
        assert_eq!(got, "思考");
    }

    #[test]
    fn ascii_passthrough() {
        let mut d = Utf8Stream::new();
        assert_eq!(d.push(b"data: hi\n\n"), "data: hi\n\n");
    }

    #[test]
    fn invalid_byte_becomes_replacement() {
        let mut d = Utf8Stream::new();
        let out = d.push(&[0x41, 0xFF, 0x42]); // A <invalid> B
        assert_eq!(out, "A\u{FFFD}B");
    }

    #[test]
    fn trailing_partial_then_completed() {
        let mut d = Utf8Stream::new();
        assert_eq!(d.push(&[0xE6, 0x80]), ""); // nothing decodable yet
        assert_eq!(d.push(&[0x9D]), "思"); // completes the char
    }
}
