//! `appcache/appinfo.vdf` — Steam's metadata cache for every app it knows about.
//!
//! We read a handful of fields per app, all out of `common`: `type`, `name`, `clienticon`,
//! `icon`, `header_image` and `library_assets_full`. `type` is what distinguishes a game from a
//! redistributable, a soundtrack or a dedicated server — the difference between a library list
//! and a library list with junk in it.
//!
//! # `library_assets_full` is the index into `librarycache`, and filenames are not
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
//! # `icon` and `clienticon` are different fields
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
//! # In v29 the KV keys are u32 indices, not strings
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
//! # String-table indices are per-file — resolve by content, never by number
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

mod cursor;
#[cfg(test)]
mod tests;

use cursor::{Cursor, Keys, read_key, read_string_table};
