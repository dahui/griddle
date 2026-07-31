//! `appcache/appinfo.vdf` — Steam's metadata cache for every app it knows about.
//!
//! We read a handful of fields per app, all out of `common`: `type`, `name`, `clienticon`,
//! `icon`, `header_image` and `library_assets_full`. `type` is what distinguishes a game from a
//! redistributable, a soundtrack or a dedicated server — the difference between a library list
//! and a library list with junk in it.
//!
//! # 🔴 `library_assets_full` is the index into `librarycache`, and filenames are not
//!
//! `common/library_assets_full/<slot>/image/<lang>` holds the path **relative to
//! `appcache/librarycache/<appid>/`**, including a `<sha1>/` directory component when there is
//! one. `[VERIFIED-BOX 2026-07-30]`
//!
//! ```text
//! 620      library_capsule -> "library_600x900.jpg"
//! 1030300  library_capsule -> "93637c34351160eaa7d7ff0cce69cb4312abb819/library_capsule.jpg"
//! 1091500  library_capsule -> { english: "...", schinese: ".../library_capsule_schinese.jpg" }
//! ```
//!
//! The same slot is `library_600x900.jpg` for one app and `library_capsule.jpg` for another, so
//! **matching on the basename is not a durable predicate** — it works on whichever app you
//! happened to test and silently misses the rest. Read the path from here instead.
//!
//! Corroborated structurally: `library_assets_full` occurs exactly **once** in the file (it is a
//! string-table *key*) while `library_capsule` occurs **305×** (those are inline path *values*).
//!
//! Two consequences for anyone consuming this: the paths can run **ahead of disk** (Steam
//! downloads the cache lazily — 24-32 of them per slot had no file on this box), so every path
//! must be existence-checked; and the value is attacker-adjacent input read out of a 6 MB binary
//! we do not control, so joining it onto a directory needs an escape guard. Both live in
//! [`crate::steam::librarycache`].
//!
//! # 🔴 `icon` and `clienticon` are different fields
//!
//! `common/icon` is the sha1 of the small game icon, and the file is
//! `librarycache/<appid>/<icon>.jpg` — 628 of 630 matched on this box. `common/clienticon` is a
//! *different* sha1 (1030300: `b4a999c1…` vs `28f5a413…`) naming a `.ico` under
//! `Steam\steam\games\`. Conflating them yields a path that does not exist.
//!
//! # The format, measured on this box `[VERIFIED-BOX 2026-07-30]`
//!
//! ```text
//! u32  magic              0x07564429  ("29 44 56 07" on disk)
//! u32  universe           1
//! i64  string_table_off   6052322     (v29+ only)
//!
//! repeating, until appid == 0:
//!   u32  appid
//!   u32  size             bytes that follow, for this entry
//!   -- within those `size` bytes: --
//!   u32  info_state
//!   u32  last_updated
//!   u64  pics_token
//!   [20] sha1_text
//!   u32  change_number
//!   [20] sha1_data        (v28+ only)
//!   ..   binary KV blob
//!
//! at string_table_off (v29+):
//!   u32  count            9342
//!   ..   count NUL-terminated strings
//! ```
//!
//! # 🔴 In v29 the KV keys are u32 indices, not strings
//!
//! This is the one thing that makes `vdf::binary` unusable here. The first app's blob reads:
//!
//! ```text
//! 00 | 00 00 00 00                          map, key #0   -> "appinfo"
//!   02 | 01 00 00 00 | 05 00 00 00          i32, key #1   -> "appid"       = 5
//!   02 | 02 00 00 00 | 01 00 00 00          i32, key #2   -> "public_only" = 1
//! 08                                        end
//! ```
//! `[VERIFIED-BOX 2026-07-30]` Type markers are identical to `vdf::binary`; only the key
//! encoding differs. A parser that assumed NUL-terminated keys here would read four bytes of
//! index as the start of a string and produce confident garbage.
//!
//! # 🔴 String-table indices are per-file — resolve by content, never by number
//!
//! On this build `common` is #3, `type` #5, `name` #4 and `clienticon` #363. Those numbers are
//! a property of *this* file and change whenever Steam rewrites it. The finder predicate is
//! "the entry whose resolved key equals `type`", never "index 5".
//!
//! # Why a bad app cannot corrupt the read
//!
//! Each entry is length-prefixed, so the reader advances by `size` regardless of what the blob
//! contains and parses the blob inside that slice. **One unparseable app is skipped and
//! counted, not fatal** — the same discipline that keeps one corrupt `appmanifest` from
//! emptying the library. An unknown magic yields [`Error::UnsupportedVersion`] so the caller
//! can degrade to the blocklist rather than showing an empty library.

use std::collections::{BTreeMap, HashMap};

/// The language Steam falls back to, and the one nearly every asset is keyed by.
pub const DEFAULT_LANGUAGE: &str = "english";

/// v29 — string table, and `sha1_data` present. The version on this machine.
pub const MAGIC_V29: u32 = 0x0756_4429;
/// v28 — `sha1_data` present, keys still inline. `[INFERRED]`
pub const MAGIC_V28: u32 = 0x0756_4428;
/// v27 — no `sha1_data`, keys inline. `[INFERRED]`
pub const MAGIC_V27: u32 = 0x0756_4427;

/// Guards a corrupt header from driving a huge allocation. Real files nest ~4 deep.
const MAX_DEPTH: usize = 32;

// Type markers. Identical to `vdf::binary`; only the key encoding differs in v29.
const T_MAP: u8 = 0x00;
const T_STRING: u8 = 0x01;
const T_INT32: u8 = 0x02;
const T_FLOAT32: u8 = 0x03;
const T_POINTER: u8 = 0x04;
const T_WIDESTRING: u8 = 0x05;
const T_COLOR: u8 = 0x06;
const T_UINT64: u8 = 0x07;
const T_END: u8 = 0x08;
const T_INT64: u8 = 0x0A;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Not a magic we know. The caller should degrade, not fail: Steam has bumped this format
    /// before and will again.
    #[error("unsupported appinfo.vdf version {magic:#010x}")]
    UnsupportedVersion { magic: u32 },

    #[error("unexpected end of input at byte {offset} (wanted {expected})")]
    UnexpectedEof {
        offset: usize,
        expected: &'static str,
    },

    #[error("unknown type marker {marker:#04x} at byte {offset}")]
    UnknownMarker { marker: u8, offset: usize },

    #[error("string table offset {offset} is outside the file ({len} bytes)")]
    StringTableOutOfRange { offset: i64, len: usize },

    #[error("string table claims {count} strings, which cannot fit in the remaining {left} bytes")]
    StringTableTooLarge { count: usize, left: usize },

    #[error("key index {index} is past the end of the {count}-entry string table")]
    KeyIndexOutOfRange { index: u32, count: usize },

    #[error("unterminated string at byte {offset}")]
    UnterminatedString { offset: usize },

    #[error("nesting deeper than {MAX_DEPTH}")]
    TooDeep,
}

/// Which layout the file uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// `[VERIFIED-BOX 2026-07-30]`
    V29,
    /// `[INFERRED]` — not reproducible on this machine.
    V28,
    /// `[INFERRED]` — not reproducible on this machine.
    V27,
}

impl Version {
    fn from_magic(magic: u32) -> Option<Self> {
        match magic {
            MAGIC_V29 => Some(Version::V29),
            MAGIC_V28 => Some(Version::V28),
            MAGIC_V27 => Some(Version::V27),
            _ => None,
        }
    }

    /// v29 moved keys into a table at the end of the file.
    fn has_string_table(self) -> bool {
        matches!(self, Version::V29)
    }

    /// v28 added a second sha1 over the binary form.
    fn has_sha1_data(self) -> bool {
        matches!(self, Version::V29 | Version::V28)
    }
}

/// The `common` fields we care about. Everything else in the blob is skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Common {
    /// `Game`, `Tool`, `Application`, `Demo`, `DLC`, `Music`, `Config`, …
    pub app_type: Option<String>,
    pub name: Option<String>,
    /// `common/clienticon` — sha1 of the `.ico` under `Steam\steam\games\`.
    ///
    /// **Not** the librarycache icon: see [`Common::icon`], which is a different sha1 on the
    /// same app.
    pub client_icon: Option<String>,
    /// `common/icon` — sha1 of the small icon at `librarycache/<appid>/<icon>.jpg`.
    pub icon: Option<String>,
    /// `common/header_image`, language → filename (almost always `header.jpg`).
    pub header_image: BTreeMap<String, String>,
    /// `common/library_assets_full/<slot>/image`, slot → language → path **relative to
    /// `librarycache/<appid>/`**, which may contain a `<sha1>/` component.
    ///
    /// Slots seen on this box: `library_capsule`, `library_hero`, `library_hero_blur`,
    /// `library_logo`, `library_header`. `image2x` is deliberately not captured — none of those
    /// files exist on disk here.
    pub library_assets: BTreeMap<String, BTreeMap<String, String>>,
}

impl Common {
    /// `common/header_image` for a language, falling back to English and then to any entry.
    pub fn header_image_for(&self, lang: &str) -> Option<&str> {
        pick(&self.header_image, lang)
    }

    /// A `library_assets_full` slot's path for a language, with the same fallback.
    ///
    /// The result is *relative* and untrusted — join it only through
    /// [`crate::steam::librarycache`], which guards against escaping the app directory.
    pub fn library_asset(&self, slot: &str, lang: &str) -> Option<&str> {
        pick(self.library_assets.get(slot)?, lang)
    }
}

/// Language lookup: exact, then English, then whatever exists.
///
/// The last step matters — an app localized only into Simplified Chinese still has perfectly
/// good artwork, and returning nothing for it would show a blank tile instead.
fn pick<'m>(map: &'m BTreeMap<String, String>, lang: &str) -> Option<&'m str> {
    map.get(&lang.to_ascii_lowercase())
        .or_else(|| map.get(DEFAULT_LANGUAGE))
        .or_else(|| map.values().next())
        .map(String::as_str)
}

/// One app's entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    pub app_id: u32,
    pub last_updated: u32,
    pub change_number: u32,
    pub common: Common,
}

/// A parsed `appinfo.vdf`.
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub version: Version,
    pub universe: u32,
    pub apps: HashMap<u32, AppEntry>,
    /// Entries whose blob would not parse. Reported rather than hidden — a number that starts
    /// climbing after a Steam update is the signal that the format moved.
    pub skipped: usize,
    /// True when the entry list ended exactly where the string table begins.
    ///
    /// False means we lost our place walking the length-prefixed entries and the app list is
    /// probably incomplete. Always true on a healthy v29 file `[VERIFIED-BOX 2026-07-30]`, and
    /// trivially true for versions with no string table to check against.
    pub aligned: bool,
}

/// Parse the whole file.
///
/// Reads into memory rather than streaming: the file is ~6 MB, and a slice-based parser with
/// bounds checks on every read is markedly easier to make correct than a streaming one. The
/// cost is irrelevant on a desktop; the correctness is not.
pub fn parse(data: &[u8]) -> Result<AppInfo, Error> {
    let mut cur = Cursor::new(data);
    let magic = cur.u32()?;
    let version = Version::from_magic(magic).ok_or(Error::UnsupportedVersion { magic })?;
    let universe = cur.u32()?;

    let mut table_offset: Option<usize> = None;
    let table: Vec<&[u8]> = if version.has_string_table() {
        let offset = cur.i64()?;
        let parsed = read_string_table(data, offset)?;
        table_offset = usize::try_from(offset).ok();
        parsed
    } else {
        Vec::new()
    };
    let keys = Keys {
        table: &table,
        indexed: version.has_string_table(),
    };

    let mut apps = HashMap::new();
    let mut skipped = 0usize;

    loop {
        let app_id = cur.u32()?;
        if app_id == 0 {
            break; // Normal end of the app list.
        }
        let size = cur.u32()? as usize;
        // The length prefix is the resync point: whatever the blob turns out to contain, the
        // next entry starts exactly here. This is what contains a parse failure to one app.
        let body = cur.take(size, "app entry")?;

        match parse_entry(app_id, body, &keys, version) {
            Ok(entry) => {
                if apps.insert(app_id, entry).is_some() {
                    tracing::debug!(app_id, "duplicate appid in appinfo.vdf; later entry wins");
                }
            }
            Err(e) => {
                skipped += 1;
                tracing::debug!(app_id, error = %e, "skipping unparseable appinfo entry");
            }
        }
    }

    if skipped > 0 {
        tracing::warn!(
            skipped,
            parsed = apps.len(),
            "some appinfo.vdf entries did not parse"
        );
    }

    // Structural self-check. Having consumed the terminating appid, we should be sitting
    // exactly on the string table. Landing anywhere else means we mis-stepped through the
    // length-prefixed entry list — which would silently drop apps rather than fail, and would
    // look to the user like "some of my games are missing".
    //
    // Reported, not fatal: the entries parsed so far are still good, and losing them to be
    // strict would be the wrong trade for a library list.
    let aligned = match table_offset {
        Some(expected) => cur.pos == expected,
        None => true,
    };
    if !aligned {
        tracing::warn!(
            ended_at = cur.pos,
            string_table_at = ?table_offset,
            parsed = apps.len(),
            "appinfo.vdf entry list did not end where the string table begins; \
             apps may have been missed"
        );
    }

    Ok(AppInfo {
        version,
        universe,
        apps,
        skipped,
        aligned,
    })
}

fn parse_entry<'a>(
    app_id: u32,
    body: &'a [u8],
    keys: &Keys<'a>,
    version: Version,
) -> Result<AppEntry, Error> {
    let mut c = Cursor::new(body);
    let _info_state = c.u32()?;
    let last_updated = c.u32()?;
    let _pics_token = c.u64()?;
    let _sha1_text = c.take(20, "sha1_text")?;
    let change_number = c.u32()?;
    if version.has_sha1_data() {
        let _sha1_data = c.take(20, "sha1_data")?;
    }

    let mut common = Common::default();

    // The blob is a single root map, keyed "appinfo".
    //
    // An entry with *no* blob at all is legal and simply has no common fields. But a blob that
    // starts with anything other than a map marker is corruption, and must be reported rather
    // than quietly yielding an app with no type — `skipped` is the early-warning signal that
    // the format moved, so it has to actually fire.
    match c.u8() {
        Err(_) => {}
        Ok(T_MAP) => {
            let _root_key = read_key(&mut c, keys)?;
            let mut path: Vec<&[u8]> = Vec::new();
            read_map(&mut c, keys, 0, &mut path, &mut common)?;
        }
        Ok(other) => {
            return Err(Error::UnknownMarker {
                marker: other,
                offset: c.pos.saturating_sub(1),
            });
        }
    }

    Ok(AppEntry {
        app_id,
        last_updated,
        change_number,
        common,
    })
}

/// Read a map's entries up to its `0x08`.
///
/// `path` is the chain of map keys descended into so far, relative to the entry's root map, and
/// it is what [`capture`] dispatches on. Matching the *whole* path rather than a "are we inside
/// `common`" flag is what keeps a nested map (`name_localized`, `associations`) from
/// overwriting the fields we want — that property now holds by construction rather than by a
/// flag someone has to remember to clear.
fn read_map<'a>(
    c: &mut Cursor<'a>,
    keys: &Keys<'a>,
    depth: usize,
    path: &mut Vec<&'a [u8]>,
    out: &mut Common,
) -> Result<(), Error> {
    if depth > MAX_DEPTH {
        return Err(Error::TooDeep);
    }

    loop {
        let marker = c.u8()?;
        if marker == T_END {
            return Ok(());
        }
        let key = read_key(c, keys)?;

        match marker {
            T_MAP => {
                // Push and pop around the recursion, propagating the error *after* the pop, so
                // the stack cannot desync on a truncated blob.
                path.push(key);
                let descended = read_map(c, keys, depth + 1, path, out);
                let _ = path.pop();
                descended?;
            }
            T_STRING => {
                let value = c.cstring()?;
                capture(path, key, value, out);
            }
            // Everything else is skipped by width. We never need these values, and decoding
            // them would only add ways to be wrong.
            T_INT32 | T_FLOAT32 | T_POINTER | T_COLOR => {
                let _ = c.take(4, "4-byte value")?;
            }
            T_UINT64 | T_INT64 => {
                let _ = c.take(8, "8-byte value")?;
            }
            T_WIDESTRING => {
                c.wstring()?;
            }
            other => {
                return Err(Error::UnknownMarker {
                    marker: other,
                    offset: c.pos.saturating_sub(1),
                });
            }
        }
    }
}

/// Store a string value if its full key path is one we want.
///
/// Dispatching on the path is what makes this safe: `common/name` and
/// `common/name_localized/english` are different paths, so the localized map cannot clobber the
/// real name no matter what order the file happens to list them in.
fn capture(path: &[&[u8]], key: &[u8], value: &[u8], out: &mut Common) {
    // Everything we want lives under `common`, so leave early for the great majority of the
    // blob rather than comparing key names inside `config`, `extended`, `depots` and the rest.
    let Some((root, rest)) = path.split_first() else {
        return;
    };
    if !root.eq_ignore_ascii_case(b"common") {
        return;
    }

    let text = || String::from_utf8_lossy(value).into_owned();
    let lang = || String::from_utf8_lossy(key).to_ascii_lowercase();

    match rest {
        // common/<key>
        [] => {
            if key.eq_ignore_ascii_case(b"type") {
                out.app_type = Some(text());
            } else if key.eq_ignore_ascii_case(b"name") {
                out.name = Some(text());
            } else if key.eq_ignore_ascii_case(b"clienticon") {
                out.client_icon = Some(text());
            } else if key.eq_ignore_ascii_case(b"icon") {
                out.icon = Some(text());
            }
        }
        // common/header_image/<lang>
        [section] if section.eq_ignore_ascii_case(b"header_image") => {
            let _ = out.header_image.insert(lang(), text());
        }
        // common/library_assets_full/<slot>/image/<lang>
        //
        // The `image` guard is also what excludes `image2x`: those files are on no disk here,
        // so capturing them would only produce paths that never resolve.
        [section, slot, leaf]
            if section.eq_ignore_ascii_case(b"library_assets_full")
                && leaf.eq_ignore_ascii_case(b"image") =>
        {
            let _ = out
                .library_assets
                .entry(String::from_utf8_lossy(slot).to_ascii_lowercase())
                .or_default()
                .insert(lang(), text());
        }
        _ => {}
    }
}

/// How keys are encoded: u32 indices into a table (v29) or inline NUL-terminated (v27/v28).
struct Keys<'a> {
    table: &'a [&'a [u8]],
    indexed: bool,
}

fn read_key<'a>(c: &mut Cursor<'a>, keys: &Keys<'a>) -> Result<&'a [u8], Error> {
    if keys.indexed {
        let index = c.u32()?;
        keys.table
            .get(index as usize)
            .copied()
            .ok_or(Error::KeyIndexOutOfRange {
                index,
                count: keys.table.len(),
            })
    } else {
        c.cstring()
    }
}

fn read_string_table(data: &[u8], offset: i64) -> Result<Vec<&[u8]>, Error> {
    let start = usize::try_from(offset)
        .ok()
        .filter(|o| *o < data.len())
        .ok_or(Error::StringTableOutOfRange {
            offset,
            len: data.len(),
        })?;

    let mut c = Cursor { data, pos: start };
    let count = c.u32()? as usize;

    // Each string costs at least its NUL, so the count cannot exceed the bytes left. This
    // turns a corrupt offset into an error instead of a multi-gigabyte allocation.
    let left = data.len() - c.pos;
    if count > left {
        return Err(Error::StringTableTooLarge { count, left });
    }

    let mut table = Vec::with_capacity(count);
    for _ in 0..count {
        table.push(c.cstring()?);
    }
    Ok(table)
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    fn take(&mut self, n: usize, expected: &'static str) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::UnexpectedEof {
            offset: self.pos,
            expected,
        })?;
        let slice = self.data.get(self.pos..end).ok_or(Error::UnexpectedEof {
            offset: self.pos,
            expected,
        })?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1, "u8")?[0])
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let mut b = [0u8; 4];
        b.copy_from_slice(self.take(4, "u32")?);
        Ok(u32::from_le_bytes(b))
    }

    fn u64(&mut self) -> Result<u64, Error> {
        let mut b = [0u8; 8];
        b.copy_from_slice(self.take(8, "u64")?);
        Ok(u64::from_le_bytes(b))
    }

    fn i64(&mut self) -> Result<i64, Error> {
        let mut b = [0u8; 8];
        b.copy_from_slice(self.take(8, "i64")?);
        Ok(i64::from_le_bytes(b))
    }

    fn cstring(&mut self) -> Result<&'a [u8], Error> {
        let start = self.pos;
        let len = self.data[start..]
            .iter()
            .position(|&b| b == 0)
            .ok_or(Error::UnterminatedString { offset: start })?;
        self.pos = start + len + 1;
        Ok(&self.data[start..start + len])
    }

    /// UTF-16, NUL-terminated by a *pair* of zero bytes. Never seen in practice; handled so an
    /// exotic entry is skipped correctly rather than desyncing its blob.
    fn wstring(&mut self) -> Result<(), Error> {
        let start = self.pos;
        loop {
            let pair = self.take(2, "wide string")?;
            if pair == [0, 0] {
                return Ok(());
            }
            if self.pos <= start {
                return Err(Error::UnterminatedString { offset: start });
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    /// A KV tree, for fixtures whose shape is the point of the test.
    enum Node<'a> {
        Str(&'a str, &'a str),
        Map(&'a str, Vec<Node<'a>>),
    }

    /// Build a one-app v29 file from a declarative `common` tree.
    ///
    /// The string table is derived from the keys actually used, so a test can nest as deeply as
    /// it likes without hand-maintaining indices — which is what made the existing fixtures too
    /// laborious to extend, and is why the deeper paths went untested.
    fn build_tree(app_id: u32, common: Vec<Node<'_>>) -> Vec<u8> {
        fn collect<'a>(nodes: &[Node<'a>], into: &mut Vec<&'a str>) {
            for n in nodes {
                match n {
                    Node::Str(k, _) => into.push(k),
                    Node::Map(k, kids) => {
                        into.push(k);
                        collect(kids, into);
                    }
                }
            }
        }

        let mut strings: Vec<&str> = vec!["appinfo", "common"];
        collect(&common, &mut strings);
        strings.dedup();

        let idx = |s: &str| {
            strings
                .iter()
                .position(|x| *x == s)
                .map(|i| i as u32)
                .unwrap_or(0)
        };

        fn emit(nodes: &[Node<'_>], out: &mut Vec<u8>, idx: &dyn Fn(&str) -> u32) {
            for n in nodes {
                match n {
                    Node::Str(k, v) => {
                        out.push(T_STRING);
                        out.extend(idx(k).to_le_bytes());
                        out.extend(v.as_bytes());
                        out.push(0);
                    }
                    Node::Map(k, kids) => {
                        out.push(T_MAP);
                        out.extend(idx(k).to_le_bytes());
                        emit(kids, out, idx);
                        out.push(T_END);
                    }
                }
            }
        }

        let mut blob = Vec::new();
        blob.push(T_MAP);
        blob.extend(idx("appinfo").to_le_bytes());
        blob.push(T_MAP);
        blob.extend(idx("common").to_le_bytes());
        emit(&common, &mut blob, &idx);
        blob.push(T_END); // common
        blob.push(T_END); // appinfo

        let mut payload = Vec::new();
        payload.extend(1u32.to_le_bytes());
        payload.extend(0u32.to_le_bytes());
        payload.extend(0u64.to_le_bytes());
        payload.extend([0u8; 20]);
        payload.extend(0u32.to_le_bytes());
        payload.extend([0u8; 20]);
        payload.extend(&blob);

        let mut body = Vec::new();
        body.extend(app_id.to_le_bytes());
        body.extend((payload.len() as u32).to_le_bytes());
        body.extend(&payload);
        body.extend(0u32.to_le_bytes());

        let mut table = Vec::new();
        table.extend((strings.len() as u32).to_le_bytes());
        for s in &strings {
            table.extend(s.as_bytes());
            table.push(0);
        }

        let mut data = Vec::new();
        data.extend(MAGIC_V29.to_le_bytes());
        data.extend(1u32.to_le_bytes());
        data.extend(((4 + 4 + 8 + body.len()) as i64).to_le_bytes());
        data.extend(&body);
        data.extend(&table);
        data
    }

    /// `library_assets_full/<slot>/image/english = <path>`, the real nesting.
    fn asset_slot<'a>(slot: &'a str, path: &'a str) -> Node<'a> {
        Node::Map(
            slot,
            vec![Node::Map("image", vec![Node::Str("english", path)])],
        )
    }

    /// Build a v29 file with a string table, mirroring the real layout.
    fn build_v29(apps: &[(u32, &str, &str, &str)]) -> Vec<u8> {
        let strings: Vec<&str> = vec!["appinfo", "appid", "common", "type", "name", "clienticon"];
        let idx = |s: &str| {
            strings
                .iter()
                .position(|x| *x == s)
                .map(|i| i as u32)
                .unwrap_or(0)
        };

        let mut body = Vec::new();
        for (id, ty, name, icon) in apps {
            // The KV blob.
            let mut blob = Vec::new();
            blob.push(T_MAP);
            blob.extend(idx("appinfo").to_le_bytes());
            blob.push(T_INT32);
            blob.extend(idx("appid").to_le_bytes());
            blob.extend(id.to_le_bytes());
            blob.push(T_MAP);
            blob.extend(idx("common").to_le_bytes());
            blob.push(T_STRING);
            blob.extend(idx("type").to_le_bytes());
            blob.extend(ty.as_bytes());
            blob.push(0);
            blob.push(T_STRING);
            blob.extend(idx("name").to_le_bytes());
            blob.extend(name.as_bytes());
            blob.push(0);
            blob.push(T_STRING);
            blob.extend(idx("clienticon").to_le_bytes());
            blob.extend(icon.as_bytes());
            blob.push(0);
            blob.push(T_END); // common
            blob.push(T_END); // appinfo
            blob.push(T_END); // blob terminator, as the real file has

            let mut payload = Vec::new();
            payload.extend(1u32.to_le_bytes()); // info_state
            payload.extend(0x6a15_4cb4u32.to_le_bytes()); // last_updated
            payload.extend(0u64.to_le_bytes()); // pics_token
            payload.extend([0xAAu8; 20]); // sha1_text
            payload.extend(0x019d_8256u32.to_le_bytes()); // change_number
            payload.extend([0xBBu8; 20]); // sha1_data (v28+)
            payload.extend(&blob);

            body.extend(id.to_le_bytes());
            body.extend((payload.len() as u32).to_le_bytes());
            body.extend(&payload);
        }
        body.extend(0u32.to_le_bytes()); // appid 0 terminates

        let mut table = Vec::new();
        table.extend((strings.len() as u32).to_le_bytes());
        for s in &strings {
            table.extend(s.as_bytes());
            table.push(0);
        }

        let mut out = Vec::new();
        out.extend(MAGIC_V29.to_le_bytes());
        out.extend(1u32.to_le_bytes()); // universe
        let table_offset = (4 + 4 + 8 + body.len()) as i64;
        out.extend(table_offset.to_le_bytes());
        out.extend(&body);
        out.extend(&table);
        out
    }

    #[test]
    fn parses_a_v29_file_resolving_keys_through_the_string_table() {
        let data = build_v29(&[
            (620, "Game", "Portal 2", "abc123"),
            (228980, "Tool", "Steamworks Common Redistributables", ""),
        ]);
        let info = parse(&data).unwrap();

        assert_eq!(info.version, Version::V29);
        assert_eq!(info.universe, 1);
        assert_eq!(info.skipped, 0);
        assert_eq!(info.apps.len(), 2);
        assert!(
            info.aligned,
            "the entry list must end exactly where the string table begins"
        );

        let portal = info.apps.get(&620).unwrap();
        assert_eq!(portal.common.app_type.as_deref(), Some("Game"));
        assert_eq!(portal.common.name.as_deref(), Some("Portal 2"));
        assert_eq!(portal.common.client_icon.as_deref(), Some("abc123"));
        assert_eq!(portal.change_number, 0x019d_8256);

        let tool = info.apps.get(&228980).unwrap();
        assert_eq!(tool.common.app_type.as_deref(), Some("Tool"));
    }

    #[test]
    fn an_unknown_magic_is_a_named_error_so_callers_can_degrade() {
        let mut data = build_v29(&[(620, "Game", "Portal 2", "x")]);
        data[0..4].copy_from_slice(&0x0756_4499u32.to_le_bytes());
        assert_eq!(
            parse(&data).unwrap_err(),
            Error::UnsupportedVersion { magic: 0x0756_4499 }
        );
    }

    #[test]
    fn one_corrupt_entry_is_skipped_and_the_rest_still_parse() {
        // The property that matters: a length-prefixed entry lets the reader resync, so a bad
        // blob costs one app rather than the whole library.
        let mut data = build_v29(&[
            (620, "Game", "Portal 2", "a"),
            (440, "Game", "Team Fortress 2", "b"),
        ]);

        // Corrupt the *first* entry's blob with an unknown type marker. Its payload starts at
        // 16 (header) + 8 (appid+size) + 60 (fixed metadata) = 84.
        data[84] = 0x7F;

        let info = parse(&data).unwrap();
        assert_eq!(info.skipped, 1, "the damaged entry must be counted");
        assert_eq!(info.apps.len(), 1, "the healthy entry must survive");
        assert!(
            info.apps.contains_key(&440),
            "resync must land on the next entry exactly"
        );
    }

    #[test]
    fn a_nested_map_inside_common_cannot_overwrite_the_captured_fields() {
        // `common` really does contain sub-maps (`name_localized`) with a `name` key in them.
        // Capturing those would rename every app to its localised alias.
        let strings = ["appinfo", "common", "type", "name", "name_localized"];
        let idx = |s: &str| strings.iter().position(|x| *x == s).unwrap_or(0) as u32;

        let mut blob = Vec::new();
        blob.push(T_MAP);
        blob.extend(idx("appinfo").to_le_bytes());
        blob.push(T_MAP);
        blob.extend(idx("common").to_le_bytes());
        blob.push(T_STRING);
        blob.extend(idx("type").to_le_bytes());
        blob.extend(b"Game\0");
        blob.push(T_STRING);
        blob.extend(idx("name").to_le_bytes());
        blob.extend(b"Real Name\0");
        blob.push(T_MAP);
        blob.extend(idx("name_localized").to_le_bytes());
        blob.push(T_STRING);
        blob.extend(idx("name").to_le_bytes());
        blob.extend(b"Localised Name\0");
        blob.push(T_END); // name_localized
        blob.push(T_END); // common
        blob.push(T_END); // appinfo

        let mut payload = Vec::new();
        payload.extend(1u32.to_le_bytes());
        payload.extend(0u32.to_le_bytes());
        payload.extend(0u64.to_le_bytes());
        payload.extend([0u8; 20]);
        payload.extend(0u32.to_le_bytes());
        payload.extend([0u8; 20]);
        payload.extend(&blob);

        let mut body = Vec::new();
        body.extend(70u32.to_le_bytes());
        body.extend((payload.len() as u32).to_le_bytes());
        body.extend(&payload);
        body.extend(0u32.to_le_bytes());

        let mut table = Vec::new();
        table.extend((strings.len() as u32).to_le_bytes());
        for s in strings {
            table.extend(s.as_bytes());
            table.push(0);
        }

        let mut data = Vec::new();
        data.extend(MAGIC_V29.to_le_bytes());
        data.extend(1u32.to_le_bytes());
        data.extend(((4 + 4 + 8 + body.len()) as i64).to_le_bytes());
        data.extend(&body);
        data.extend(&table);

        let info = parse(&data).unwrap();
        let app = info.apps.get(&70).unwrap();
        assert_eq!(app.common.name.as_deref(), Some("Real Name"));
        assert_eq!(app.common.app_type.as_deref(), Some("Game"));

        // A control for the same property, from the other direction. The assertions above are
        // all negative — they pass just as well if nothing under `common` were captured at all,
        // which is exactly how a broken fixture hides. This half asserts that a *deeper* path
        // still gets captured while `name_localized/name` does not, so the two cannot both be
        // explained by "capture never ran".
        let data = build_tree(
            70,
            vec![
                Node::Str("name", "Real Name"),
                Node::Map("name_localized", vec![Node::Str("name", "Localised Name")]),
                Node::Map(
                    "library_assets_full",
                    vec![asset_slot("library_capsule", "library_600x900.jpg")],
                ),
            ],
        );
        let info = parse(&data).unwrap();
        let common = &info.apps.get(&70).unwrap().common;
        assert_eq!(common.name.as_deref(), Some("Real Name"));
        assert_eq!(
            common.library_asset("library_capsule", "english"),
            Some("library_600x900.jpg"),
            "control: capture really is running on nested paths",
        );
    }

    #[test]
    fn library_assets_full_survives_both_disk_shapes() {
        // The two shapes measured on this box: a bare filename for 1945 apps, and a path with a
        // sha1 directory component for 278. Losing the sha1 component would produce a path that
        // does not exist, which is indistinguishable from "this app has no art".
        const FLAT: &str = "library_600x900.jpg";
        const NESTED: &str = "93637c34351160eaa7d7ff0cce69cb4312abb819/library_capsule.jpg";

        // Premise: these fixtures really are the two different shapes. Without this the test
        // still passes if someone "tidies" NESTED into a bare filename.
        assert!(!FLAT.contains('/'), "the flat fixture must have no path");
        assert!(NESTED.contains('/'), "the nested fixture must have one");

        let flat = build_tree(
            620,
            vec![Node::Map(
                "library_assets_full",
                vec![asset_slot("library_capsule", FLAT)],
            )],
        );
        let nested = build_tree(
            1030300,
            vec![Node::Map(
                "library_assets_full",
                vec![
                    asset_slot("library_capsule", NESTED),
                    asset_slot("library_hero", "70d7e70a/library_hero.jpg"),
                ],
            )],
        );

        let a = parse(&flat).unwrap();
        let common = &a.apps.get(&620).unwrap().common;
        assert_eq!(
            common.library_asset("library_capsule", "english"),
            Some(FLAT)
        );

        let b = parse(&nested).unwrap();
        let common = &b.apps.get(&1030300).unwrap().common;
        assert_eq!(
            common.library_asset("library_capsule", "english"),
            Some(NESTED),
            "the sha1 directory component must survive verbatim"
        );
        assert_eq!(
            common.library_asset("library_hero", "english"),
            Some("70d7e70a/library_hero.jpg"),
            "slots must not overwrite each other"
        );
        assert_eq!(common.library_asset("library_logo", "english"), None);
    }

    #[test]
    fn every_language_is_kept_and_lookup_falls_back() {
        let data = build_tree(
            1091500,
            vec![Node::Map(
                "library_assets_full",
                vec![Node::Map(
                    "library_capsule",
                    vec![Node::Map(
                        "image",
                        vec![
                            Node::Str("english", "6399de/library_capsule.jpg"),
                            Node::Str("schinese", "e8cc29/library_capsule_schinese.jpg"),
                        ],
                    )],
                )],
            )],
        );
        let info = parse(&data).unwrap();
        let common = &info.apps.get(&1091500).unwrap().common;

        // Premise: both languages really were captured, so the fallback below is a fallback and
        // not the only entry there is.
        assert_eq!(
            common
                .library_assets
                .get("library_capsule")
                .map(BTreeMap::len),
            Some(2),
        );

        assert_eq!(
            common.library_asset("library_capsule", "schinese"),
            Some("e8cc29/library_capsule_schinese.jpg"),
            "an exact language match wins",
        );
        assert_eq!(
            common.library_asset("library_capsule", "klingon"),
            Some("6399de/library_capsule.jpg"),
            "an unknown language falls back to english",
        );
    }

    #[test]
    fn a_language_steam_has_no_english_for_still_resolves() {
        // The last fallback rung. An app localized only into one language still has perfectly
        // good artwork, and returning None would render a blank tile.
        let data = build_tree(
            777,
            vec![Node::Map(
                "library_assets_full",
                vec![Node::Map(
                    "library_capsule",
                    vec![Node::Map(
                        "image",
                        vec![Node::Str("schinese", "only/one.jpg")],
                    )],
                )],
            )],
        );
        let info = parse(&data).unwrap();
        let common = &info.apps.get(&777).unwrap().common;

        assert!(
            !common
                .library_assets
                .get("library_capsule")
                .unwrap()
                .contains_key("english"),
            "premise: there is deliberately no english entry",
        );
        assert_eq!(
            common.library_asset("library_capsule", "english"),
            Some("only/one.jpg"),
        );
    }

    #[test]
    fn icon_and_clienticon_are_captured_as_separate_fields() {
        // Measured on 1030300: these are genuinely different sha1s. `icon` names the
        // librarycache .jpg; `clienticon` names a .ico under Steam\steam\games. Aliasing them
        // yields a path that does not exist.
        const ICON: &str = "b4a999c1302e3ac123c041fd41bb8a34528c6ab5";
        const CLIENT: &str = "28f5a413a0f1f4b0a0f8b6ff30e1cbb0e5ba9a3d";
        assert_ne!(
            ICON, CLIENT,
            "premise: the fixture uses two distinct values"
        );

        let data = build_tree(
            1030300,
            vec![
                Node::Str("icon", ICON),
                Node::Str("clienticon", CLIENT),
                Node::Str("type", "Game"),
            ],
        );
        let info = parse(&data).unwrap();
        let common = &info.apps.get(&1030300).unwrap().common;

        assert_eq!(common.icon.as_deref(), Some(ICON));
        assert_eq!(common.client_icon.as_deref(), Some(CLIENT));
    }

    #[test]
    fn image2x_is_not_captured_but_image_beside_it_is() {
        // `image2x` sits next to `image` in the real file and none of those files exist on
        // disk here. The control is the `image` sibling: without it, a test that only asserts
        // image2x is absent would also pass if the whole slot failed to parse.
        let data = build_tree(
            620,
            vec![Node::Map(
                "library_assets_full",
                vec![Node::Map(
                    "library_capsule",
                    vec![
                        Node::Map("image", vec![Node::Str("english", "real.jpg")]),
                        Node::Map("image2x", vec![Node::Str("english", "retina.jpg")]),
                    ],
                )],
            )],
        );
        let info = parse(&data).unwrap();
        let common = &info.apps.get(&620).unwrap().common;

        assert_eq!(
            common.library_asset("library_capsule", "english"),
            Some("real.jpg"),
            "control: the `image` sibling really was captured",
        );
        let slot = common.library_assets.get("library_capsule").unwrap();
        assert_eq!(slot.len(), 1, "image2x must not have been merged in");
    }

    #[test]
    fn a_key_index_past_the_table_is_rejected_not_read_out_of_bounds() {
        let keys = Keys {
            table: &[b"appinfo"],
            indexed: true,
        };
        let bytes = 9999u32.to_le_bytes();
        let mut c = Cursor::new(&bytes);
        assert_eq!(
            read_key(&mut c, &keys).unwrap_err(),
            Error::KeyIndexOutOfRange {
                index: 9999,
                count: 1
            }
        );
    }

    #[test]
    fn an_absurd_string_count_is_refused_rather_than_allocated() {
        // A corrupt count must not become a multi-gigabyte Vec::with_capacity.
        let mut data = Vec::new();
        data.extend(u32::MAX.to_le_bytes());
        data.extend(b"short\0");
        let err = read_string_table(&data, 0).unwrap_err();
        assert!(matches!(err, Error::StringTableTooLarge { .. }), "{err:?}");
    }

    #[test]
    fn a_string_table_offset_past_the_end_is_an_error() {
        let data = [0u8; 16];
        assert!(matches!(
            read_string_table(&data, 9_999_999),
            Err(Error::StringTableOutOfRange { .. })
        ));
        assert!(matches!(
            read_string_table(&data, -1),
            Err(Error::StringTableOutOfRange { .. })
        ));
    }

    #[test]
    fn a_truncated_file_errors_instead_of_panicking() {
        let full = build_v29(&[(620, "Game", "Portal 2", "a"), (440, "Game", "TF2", "b")]);
        let complete = parse(&full).unwrap();

        // Every prefix must either error or return a subset — and above all must never panic
        // on an out-of-range slice. This file is 6 MB of binary we do not control.
        for cut in 0..full.len() {
            if let Ok(partial) = parse(&full[..cut]) {
                assert!(
                    partial.apps.len() <= complete.apps.len(),
                    "truncating at {cut} produced more apps than the whole file"
                );
            }
        }
    }

    #[test]
    fn an_empty_app_list_parses_to_nothing() {
        let data = build_v29(&[]);
        let info = parse(&data).unwrap();
        assert!(info.apps.is_empty());
        assert_eq!(info.skipped, 0);
    }
}
