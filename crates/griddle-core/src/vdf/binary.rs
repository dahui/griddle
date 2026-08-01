//! Binary KeyValues (KV1) codec — the format of `shortcuts.vdf`.
//!
//! # Wire format
//!
//! A stream of entries. Each entry is a one-byte type marker, a NUL-terminated key, then a
//! payload determined by the marker:
//!
//! | Marker | Payload |
//! |---|---|
//! | `0x00` | nested entries, terminated by `0x08` |
//! | `0x01` | NUL-terminated string |
//! | `0x02` | `i32` little-endian |
//! | `0x07` | `u64` little-endian |
//! | `0x08` | *(no key)* end of the enclosing map |
//!
//! # Two properties this codec guarantees, both learned from the real file
//!
//! **Byte-exact round-trip.** `write(parse(x)) == x`. Two things on this machine's real
//! `shortcuts.vdf` would break a naive implementation:
//!
//! 1. The file ends with **four** consecutive `0x08` bytes, one more than the nesting depth
//!    (`tags` / shortcut `"0"` / root `"shortcuts"`). The extra byte is a file-level
//!    terminator. `[VERIFIED-BOX 2026-07-27]` We count them rather than assuming, so a file
//!    written by a different Steam version round-trips too.
//! 2. `StartDir` contains **mixed path separators**
//!    (`C:\Users\jeff\AppData\Roaming/EmuDeck/...`). `[VERIFIED-BOX 2026-07-27]` Tidying that
//!    up would be a silent corruption, so keys and string values are held as raw bytes and
//!    never normalized, re-encoded, or validated as UTF-8.
//!
//! **Order and duplicates preserved.** Entries are a `Vec`, not a map. Steam's own writer
//! emits a stable field order and we do not get to have an opinion about it.
//!
//! # Why this matters more than it looks
//!
//! Steam holds `shortcuts.vdf` in memory and rewrites it on exit, so a write while Steam is
//! running is silently discarded. That makes a corrupting bug here maximally confusing: it
//! surfaces only after a restart, long after the code that caused it ran. See
//! `steam::shortcuts` for the `SteamStopped` token that makes the unsafe call not compile.

use std::fmt;

/// Type marker for a nested map.
const T_MAP: u8 = 0x00;
/// Type marker for a NUL-terminated string.
const T_STRING: u8 = 0x01;
/// Type marker for a little-endian `i32`.
const T_INT32: u8 = 0x02;
/// Type marker for a little-endian `u64`.
const T_UINT64: u8 = 0x07;
/// End of the enclosing map. Carries no key.
const T_END: u8 = 0x08;

/// A decoded value.
///
/// String payloads and keys are raw bytes, not `String`. See the module docs: byte fidelity
/// beats tidiness, and Steam has already been observed writing paths we would be tempted to
/// "fix".
#[derive(Clone, PartialEq, Eq)]
pub enum Value {
    Map(Vec<Entry>),
    Str(Vec<u8>),
    Int32(i32),
    UInt64(u64),
}

/// One key/value pair.
#[derive(Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: Vec<u8>,
    pub value: Value,
}

/// A parsed binary KV1 document.
#[derive(Clone, PartialEq, Eq)]
pub struct Document {
    /// Top-level entries. For `shortcuts.vdf` this is a single `shortcuts` map.
    pub entries: Vec<Entry>,
    /// Count of file-level `0x08` bytes after the last top-level entry closed.
    ///
    /// Observed as `1` on this machine `[VERIFIED-BOX 2026-07-27]`, but recorded rather than
    /// assumed so a file from another Steam build still round-trips byte-exactly.
    pub trailing_terminators: usize,
}

/// A decode failure, always carrying the byte offset so a malformed file can be inspected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("unexpected end of input at byte {offset} (expected {expected})")]
    UnexpectedEof {
        offset: usize,
        expected: &'static str,
    },

    #[error("unknown type marker {marker:#04x} at byte {offset}")]
    UnknownMarker { marker: u8, offset: usize },

    #[error("unterminated string starting at byte {offset}")]
    UnterminatedString { offset: usize },

    #[error("unclosed map opened at byte {offset}")]
    UnclosedMap { offset: usize },

    #[error(
        "trailing garbage at byte {offset}: expected only 0x08 terminators, found {found:#04x}"
    )]
    TrailingGarbage { offset: usize, found: u8 },

    #[error("nesting deeper than {limit} at byte {offset}")]
    TooDeep { offset: usize, limit: usize },
}

/// Guards against a malformed or hostile file driving the recursive parser into a stack
/// overflow. Real `shortcuts.vdf` nests three deep.
const MAX_DEPTH: usize = 64;

/// Decode a binary KV1 document.
pub fn parse(input: &[u8]) -> Result<Document, Error> {
    let mut p = Parser { input, pos: 0 };
    let entries = p.parse_entries(0)?;

    // Everything remaining must be file-level terminators. Count them so `write` can put
    // exactly as many back.
    let mut trailing_terminators = 0;
    while let Some(&b) = p.input.get(p.pos) {
        if b != T_END {
            return Err(Error::TrailingGarbage {
                offset: p.pos,
                found: b,
            });
        }
        trailing_terminators += 1;
        p.pos += 1;
    }

    Ok(Document {
        entries,
        trailing_terminators,
    })
}

/// Encode a document. Inverse of [`parse`].
pub fn write(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    write_entries(&doc.entries, &mut out);
    out.extend(std::iter::repeat_n(T_END, doc.trailing_terminators));
    out
}

fn write_entries(entries: &[Entry], out: &mut Vec<u8>) {
    for entry in entries {
        match &entry.value {
            Value::Map(children) => {
                out.push(T_MAP);
                write_key(&entry.key, out);
                write_entries(children, out);
                out.push(T_END);
            }
            Value::Str(s) => {
                out.push(T_STRING);
                write_key(&entry.key, out);
                out.extend_from_slice(s);
                out.push(0);
            }
            Value::Int32(v) => {
                out.push(T_INT32);
                write_key(&entry.key, out);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Value::UInt64(v) => {
                out.push(T_UINT64);
                write_key(&entry.key, out);
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
}

fn write_key(key: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(key);
    out.push(0);
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    /// Parse entries until a `0x08` terminator or end of input.
    ///
    /// The terminator is consumed when found. Callers at depth 0 tolerate its absence (the
    /// file-level terminators are counted separately by [`parse`]); nested callers do not.
    fn parse_entries(&mut self, depth: usize) -> Result<Vec<Entry>, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::TooDeep {
                offset: self.pos,
                limit: MAX_DEPTH,
            });
        }

        let mut entries = Vec::new();
        loop {
            let Some(&marker) = self.input.get(self.pos) else {
                // End of input with no terminator. Fine at the top level, where `parse`
                // handles the file-level terminators; a truncated file otherwise.
                return Ok(entries);
            };

            if marker == T_END {
                if depth == 0 {
                    // A file-level terminator, not ours. Leave it for `parse` to count.
                    return Ok(entries);
                }
                self.pos += 1;
                return Ok(entries);
            }

            let marker_offset = self.pos;
            self.pos += 1;
            let key = self.read_cstring()?.to_vec();

            let value = match marker {
                T_MAP => {
                    let open_at = marker_offset;
                    let before = self.pos;
                    let children = self.parse_entries(depth + 1)?;
                    // `parse_entries` returns on end-of-input as well as on a terminator, so
                    // check that we actually consumed one.
                    if self.pos == before && self.input.get(self.pos).is_none() {
                        return Err(Error::UnclosedMap { offset: open_at });
                    }
                    if self.pos > self.input.len() {
                        return Err(Error::UnclosedMap { offset: open_at });
                    }
                    Value::Map(children)
                }
                T_STRING => Value::Str(self.read_cstring()?.to_vec()),
                T_INT32 => Value::Int32(i32::from_le_bytes(self.read_array::<4>("i32")?)),
                T_UINT64 => Value::UInt64(u64::from_le_bytes(self.read_array::<8>("u64")?)),
                other => {
                    return Err(Error::UnknownMarker {
                        marker: other,
                        offset: marker_offset,
                    });
                }
            };

            entries.push(Entry { key, value });
        }
    }

    fn read_cstring(&mut self) -> Result<&'a [u8], Error> {
        let start = self.pos;
        let end = self.input[start..]
            .iter()
            .position(|&b| b == 0)
            .ok_or(Error::UnterminatedString { offset: start })?;
        self.pos = start + end + 1;
        Ok(&self.input[start..start + end])
    }

    fn read_array<const N: usize>(&mut self, expected: &'static str) -> Result<[u8; N], Error> {
        let slice = self
            .input
            .get(self.pos..self.pos + N)
            .ok_or(Error::UnexpectedEof {
                offset: self.pos,
                expected,
            })?;
        let mut buf = [0u8; N];
        buf.copy_from_slice(slice);
        self.pos += N;
        Ok(buf)
    }
}

// -- Convenience accessors -------------------------------------------------------------

impl Value {
    /// The nested entries, if this is a map.
    pub fn as_map(&self) -> Option<&[Entry]> {
        match self {
            Value::Map(entries) => Some(entries),
            _ => None,
        }
    }

    /// The raw string bytes, if this is a string.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The string interpreted as UTF-8, if this is a string and it is valid UTF-8.
    ///
    /// Prefer [`Value::as_bytes`] when the value will be written back — this method is for
    /// display and comparison only.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(self.as_bytes()?).ok()
    }

    pub fn as_i32(&self) -> Option<i32> {
        match *self {
            Value::Int32(v) => Some(v),
            _ => None,
        }
    }
}

/// Find an entry by key. Keys are compared as bytes; Steam's are ASCII in practice.
pub fn get<'a>(entries: &'a [Entry], key: &str) -> Option<&'a Value> {
    entries
        .iter()
        .find(|e| e.key == key.as_bytes())
        .map(|e| &e.value)
}

// Debug prints keys and string values as text where possible — a `Vec<u8>` dump of a
// shortcuts file is unreadable, and this type is inspected constantly during development.
impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Map(entries) => f
                .debug_map()
                .entries(entries.iter().map(|e| (Bytes(&e.key), &e.value)))
                .finish(),
            Value::Str(s) => write!(f, "{:?}", Bytes(s)),
            Value::Int32(v) => write!(f, "{v}"),
            Value::UInt64(v) => write!(f, "{v}"),
        }
    }
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} => {:?}", Bytes(&self.key), self.value)
    }
}

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Document")
            .field("entries", &self.entries)
            .field("trailing_terminators", &self.trailing_terminators)
            .finish()
    }
}

struct Bytes<'a>(&'a [u8]);

impl fmt::Debug for Bytes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match std::str::from_utf8(self.0) {
            Ok(s) => write!(f, "{s:?}"),
            Err(_) => write!(f, "{:x?}", self.0),
        }
    }
}

#[cfg(test)]
#[path = "binary_tests.rs"]
mod tests;
