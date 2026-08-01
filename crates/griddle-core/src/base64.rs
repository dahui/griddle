//! Base64, standard alphabet with padding.
//!
//! Hand-rolled rather than pulling in a crate for ~40 lines. Two callers need it and neither
//! could reasonably duplicate it:
//!
//! - `settings` stores the DPAPI-wrapped API key as text in a JSON file.
//! - `cdp` hands image bytes to Steam's `SetCustomArtworkForApp`, which takes **bare** base64
//!   with no `data:` prefix — that is what Valve's own code does.
//!
//! Tested against the RFC 4648 vectors, not just against itself.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// True if every character is in the standard base64 alphabet (including padding).
///
/// Used by `cdp` before splicing a payload into a JavaScript string literal: the base64
/// alphabet contains no quote, backslash or newline, so a value that passes this check cannot
/// break out of the literal. That makes the check a real injection guarantee rather than a
/// tidiness test.
pub fn is_base64(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
}

pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn decode(input: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };

    let bytes: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }

    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|&&c| c == b'=').count();
        if pad > 2 {
            return None;
        }
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            let v = if c == b'=' {
                // Padding is only legal in the last two positions.
                if i < 2 {
                    return None;
                }
                0
            } else {
                value(c)?
            };
            n = (n << 6) | v;
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_rfc4648_vectors() {
        // A hand-rolled codec that only agrees with itself is not worth much.
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");

        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(decode("Zg==").unwrap(), b"f");
    }

    #[test]
    fn round_trips_at_every_padding_offset() {
        for len in 0..64usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
            let encoded = encode(&data);
            assert!(encoded.len().is_multiple_of(4), "len {len} was not padded");
            assert_eq!(decode(&encoded).unwrap(), data, "len {len}");
        }
    }

    #[test]
    fn round_trips_all_256_byte_values() {
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(decode(&encode(&all)).unwrap(), all);
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(decode("Zg="), None, "wrong length");
        assert_eq!(decode("Z!=="), None, "illegal character");
        assert_eq!(decode("=Zm8"), None, "padding at the front");
    }

    #[test]
    fn is_base64_accepts_real_output_and_rejects_anything_that_could_escape_a_js_literal() {
        assert!(is_base64(&encode(b"some image bytes")));
        assert!(is_base64("Zm9vYmFy"));
        assert!(is_base64("Zg=="));

        // The point of the check: none of these may ever reach a JS string literal.
        assert!(!is_base64(""), "empty is not a valid payload");
        assert!(!is_base64("abc\"def"), "a quote would close the literal");
        assert!(
            !is_base64("abc\\def"),
            "a backslash would escape the next char"
        );
        assert!(
            !is_base64("abc\ndef"),
            "a newline would break the statement"
        );
        assert!(!is_base64("data:image/png;base64,AAAA"), "the data: prefix");
        assert!(!is_base64("abc def"));
        assert!(!is_base64("</script>"));
    }
}
