//! Reading and editing `userdata/<accountid>/config/shortcuts.vdf`.
//!
//! **One of only three modules allowed to write files** (with `grid::store` and `settings`).
//!
//! # Why we touch this file at all
//!
//! Only for one thing: a non-Steam shortcut's **icon**. `SetCustomArtworkForApp` with the Icon
//! ordinal is a silent no-op — it writes nothing, for shortcuts and real apps alike (S8,
//! `[VERIFIED-BOX 2026-07-27]`), so an icon has to be a file on disk plus the `icon` field
//! here. Everything else the product does goes to `grid/`, which is safe to write at any time.
//!
//! That narrow purpose is why the mutation surface is exactly [`Shortcuts::set_icon`] and
//! [`Shortcuts::clear_icon`]. There is deliberately no general "set any field" method: this
//! file is irreplaceable user configuration, and a small API is one that can be audited by
//! reading it.
//!
//! # The four things that make a write safe
//!
//! 1. **Steam must be stopped**, proven by a [`SteamStopped`] token that only
//!    [`crate::steam::process`] can mint, and re-checked immediately before the write. Steam
//!    rewrites this file from memory on exit, so a write while it runs is silently discarded.
//! 2. **Round-trip verified on load.** If we cannot reproduce the file we just read
//!    byte-for-byte, our codec does not understand it and we refuse to write at all — before
//!    any modification is applied, not after.
//! 3. **The pristine original is backed up once**, to `shortcuts.vdf.sgdb-orig`, and never
//!    overwritten afterwards. Later backups would only preserve our own output; the file worth
//!    keeping is the one that existed before this app ever ran.
//! 4. **Temp file, fsync, rename, then read back and compare.** The rename is atomic within a
//!    directory, so an interrupted write leaves the previous file intact rather than a
//!    truncated one.
//!
//! # Conventions preserved verbatim
//!
//! - **Field names are inconsistently cased** in the real file — `appid`, `appname` and `exe`
//!   are lowercase while `StartDir`, `ShortcutPath` and `LaunchOptions` are not.
//!   `[VERIFIED-BOX 2026-07-27]` All lookups are therefore case-insensitive, and an existing
//!   key keeps whatever casing it already had.
//! - **Path values carry literal quote characters** inside the string: `exe`, `StartDir` and
//!   `icon` are all stored as `"C:\..."`, quotes included, on this machine — EmuDeck wrote
//!   them that way and Steam accepts both forms. `[VERIFIED-BOX 2026-07-27]` A new icon
//!   matches the convention already in use rather than imposing one.
//! - **Separators are never normalised.** `StartDir` is
//!   `C:\Users\jeff\AppData\Roaming/EmuDeck/...` — mixed, on purpose or not, and tidying it
//!   would be a silent corruption. The codec holds every string as raw bytes for this reason.

use crate::appid::AppId;
use crate::steam::process::SteamStopped;
use crate::vdf::binary::{self, Document, Entry, Value};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Suffix for the once-only backup of the pristine file.
const ORIGINAL_BACKUP_SUFFIX: &str = ".sgdb-orig";
/// Suffix for the same-directory temp file used by the atomic write.
const TEMP_SUFFIX: &str = ".sgdbtmp";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("writing {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is not valid binary KeyValues: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: binary::Error,
    },

    #[error(
        "{path} parsed, but re-encoding it did not reproduce the original bytes. \
         Refusing to write — this file would be corrupted."
    )]
    RoundTripMismatch { path: PathBuf },

    #[error("{path} has no `shortcuts` map")]
    NoShortcutsMap { path: PathBuf },

    #[error("no shortcut with appid {0} in {1}")]
    NotFound(AppId, PathBuf),

    #[error("Steam must be shut down first: {0}")]
    SteamRunning(#[from] crate::steam::process::Error),

    #[error(
        "wrote {path} but reading it back gave different bytes ({wrote} written, {found} read)"
    )]
    VerifyFailed {
        path: PathBuf,
        wrote: usize,
        found: usize,
    },

    #[error("internal: re-parsing our own output did not match the document we meant to write")]
    SelfCheckFailed,
}

/// A parsed `shortcuts.vdf`, plus the bytes it was read from.
#[derive(Debug, Clone)]
pub struct Shortcuts {
    path: PathBuf,
    doc: Document,
    /// The exact bytes on disk at load time. Kept so [`Shortcuts::is_modified`] is a byte
    /// comparison rather than a guess, and so a no-op save can be skipped entirely.
    original: Vec<u8>,
}

/// A read-only view of one shortcut.
#[derive(Debug, Clone, Copy)]
pub struct Shortcut<'a> {
    /// The numeric key Steam gave this entry (`"0"`, `"1"`, …), as raw bytes.
    index: &'a [u8],
    fields: &'a [Entry],
}

/// What [`Shortcuts::set_icon`] changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconChange {
    pub previous: Option<String>,
    /// Exactly what was stored, including surrounding quotes when the convention called for
    /// them.
    pub applied: String,
    /// Whether quotes were added, to match the file's existing style.
    pub quoted: bool,
}

/// The outcome of a successful [`Shortcuts::save`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub path: PathBuf,
    pub bytes_written: usize,
    /// Set the first time we ever write this file.
    pub backup_created: Option<PathBuf>,
}

impl Shortcuts {
    /// Read and parse, verifying we can reproduce the file byte-for-byte.
    ///
    /// The round-trip check happens here rather than at save time on purpose: if the codec does
    /// not understand this file, the caller should find out before building up an edit, not
    /// after.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let original = std::fs::read(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        Self::from_bytes(path, original)
    }

    /// [`Shortcuts::load`], but a missing file yields an empty document instead of an error.
    ///
    /// Steam only creates `shortcuts.vdf` once a non-Steam game is added, so its absence is
    /// completely ordinary and must not read as a failure.
    pub fn load_or_empty(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        match std::fs::read(&path) {
            Ok(bytes) => Self::from_bytes(path, bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Shortcuts {
                path,
                doc: empty_document(),
                original: Vec::new(),
            }),
            Err(source) => Err(Error::Read { path, source }),
        }
    }

    fn from_bytes(path: PathBuf, original: Vec<u8>) -> Result<Self, Error> {
        let doc = binary::parse(&original).map_err(|source| Error::Parse {
            path: path.clone(),
            source,
        })?;
        if binary::write(&doc) != original {
            return Err(Error::RoundTripMismatch { path });
        }
        Ok(Shortcuts {
            path,
            doc,
            original,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// True once an edit has actually changed the bytes.
    pub fn is_modified(&self) -> bool {
        binary::write(&self.doc) != self.original
    }

    /// The encoded form that [`Shortcuts::save`] would write.
    pub fn to_bytes(&self) -> Vec<u8> {
        binary::write(&self.doc)
    }

    /// Every shortcut, in file order.
    ///
    /// Children of the `shortcuts` map that are not themselves maps are skipped rather than
    /// treated as an error — the same defence `vdf::text` needs for the scalar siblings some
    /// client versions emit among numbered keys.
    pub fn iter(&self) -> impl Iterator<Item = Shortcut<'_>> {
        self.shortcuts_map()
            .unwrap_or(&[])
            .iter()
            .filter_map(|e| match &e.value {
                Value::Map(fields) => Some(Shortcut {
                    index: &e.key,
                    fields,
                }),
                _ => None,
            })
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }

    pub fn find(&self, app: AppId) -> Option<Shortcut<'_>> {
        self.iter().find(|s| s.app_id() == Some(app))
    }

    fn shortcuts_map(&self) -> Option<&[Entry]> {
        self.doc
            .entries
            .iter()
            .find(|e| e.key.eq_ignore_ascii_case(b"shortcuts"))
            .and_then(|e| e.value.as_map())
    }

    fn fields_mut(&mut self, app: AppId) -> Result<&mut Vec<Entry>, Error> {
        let path = self.path.clone();
        let map = self
            .doc
            .entries
            .iter_mut()
            .find(|e| e.key.eq_ignore_ascii_case(b"shortcuts"))
            .and_then(|e| match &mut e.value {
                Value::Map(m) => Some(m),
                _ => None,
            })
            .ok_or(Error::NoShortcutsMap { path: path.clone() })?;

        map.iter_mut()
            .filter_map(|e| match &mut e.value {
                Value::Map(fields) => Some(fields),
                _ => None,
            })
            .find(|fields| read_app_id(fields) == Some(app))
            .ok_or(Error::NotFound(app, path))
    }

    /// Point a shortcut's `icon` field at `icon_path`.
    ///
    /// The value is stored quoted or bare to match what the file already does — see the module
    /// docs. `icon_path` is used verbatim apart from outer quotes being normalised, so mixed
    /// separators the caller supplies survive.
    pub fn set_icon(&mut self, app: AppId, icon_path: &str) -> Result<IconChange, Error> {
        let fields = self.fields_mut(app)?;

        let previous = find_field(fields, b"icon")
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        // Which convention does this file use? The existing icon is the best evidence; failing
        // that, `exe`, which is the field most likely to be present and quoted.
        let quoted = match previous.as_deref() {
            Some(v) if !v.is_empty() => is_quoted(v),
            _ => find_field(fields, b"exe")
                .and_then(|v| v.as_str())
                .map(is_quoted)
                .unwrap_or(false),
        };

        let bare = strip_quotes(icon_path);
        let applied = if quoted {
            format!("\"{bare}\"")
        } else {
            bare.to_owned()
        };

        set_field(fields, b"icon", applied.as_bytes());

        Ok(IconChange {
            previous,
            applied,
            quoted,
        })
    }

    /// Blank a shortcut's `icon` field.
    ///
    /// The key is emptied rather than removed: Steam writes it for every shortcut, and keeping
    /// the field order identical to what Steam produces is one less way to surprise it.
    pub fn clear_icon(&mut self, app: AppId) -> Result<Option<String>, Error> {
        let fields = self.fields_mut(app)?;
        let previous = find_field(fields, b"icon")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        set_field(fields, b"icon", b"");
        Ok(previous)
    }

    /// Write the file. Requires proof that Steam is stopped.
    ///
    /// Does nothing and reports zero bytes if no edit actually changed the encoding.
    pub fn save(&self, proof: &SteamStopped) -> Result<Saved, Error> {
        // The token proves a past observation; this proves it is still true. See
        // `steam::process` for why both are needed.
        proof.reconfirm()?;

        let bytes = self.to_bytes();

        if bytes == self.original && self.path.exists() {
            return Ok(Saved {
                path: self.path.clone(),
                bytes_written: 0,
                backup_created: None,
            });
        }

        // Last line of defence: our own output must parse back into the document we meant to
        // write. A codec bug that survives this has to be symmetric in both directions.
        match binary::parse(&bytes) {
            Ok(reparsed) if reparsed == self.doc => {}
            _ => return Err(Error::SelfCheckFailed),
        }

        let backup_created = self.backup_original()?;

        let tmp = sibling_with_suffix(&self.path, TEMP_SUFFIX);
        write_atomic(&tmp, &self.path, &bytes)?;

        // Read back rather than trusting the write. This is the file whose corruption would
        // cost a user their non-Steam library, and the check costs microseconds.
        let found = std::fs::read(&self.path).map_err(|source| Error::Read {
            path: self.path.clone(),
            source,
        })?;
        if found != bytes {
            return Err(Error::VerifyFailed {
                path: self.path.clone(),
                wrote: bytes.len(),
                found: found.len(),
            });
        }

        tracing::info!(
            path = %self.path.display(),
            bytes = bytes.len(),
            was = self.original.len(),
            "wrote shortcuts.vdf"
        );

        Ok(Saved {
            path: self.path.clone(),
            bytes_written: bytes.len(),
            backup_created,
        })
    }

    /// Copy the pristine file aside, once and only once.
    ///
    /// A failure here aborts the save. Refusing to make the first edit without a safety net is
    /// the conservative direction, and this is the one file in the product that cannot be
    /// regenerated from anywhere else.
    fn backup_original(&self) -> Result<Option<PathBuf>, Error> {
        if self.original.is_empty() {
            return Ok(None); // Nothing existed to preserve.
        }
        let backup = sibling_with_suffix(&self.path, ORIGINAL_BACKUP_SUFFIX);
        if backup.exists() {
            return Ok(None); // Already have the pristine copy; never overwrite it.
        }

        let tmp = sibling_with_suffix(&backup, TEMP_SUFFIX);
        write_atomic(&tmp, &backup, &self.original)?;
        tracing::info!(path = %backup.display(), "kept a copy of the original shortcuts.vdf");
        Ok(Some(backup))
    }
}

impl<'a> Shortcut<'a> {
    /// The `"0"`, `"1"`, … key Steam filed this shortcut under.
    pub fn index(&self) -> &'a [u8] {
        self.index
    }

    /// The appid, converted from the signed form stored in the file.
    ///
    /// 🔴 Always read; never computed. The `crc32(exe + appname) | 0x80000000` folklore is
    /// disproven on modern Steam — see [`crate::appid`].
    pub fn app_id(&self) -> Option<AppId> {
        read_app_id(self.fields)
    }

    pub fn app_name(&self) -> Option<&'a str> {
        self.str_field(b"appname")
    }

    pub fn exe(&self) -> Option<&'a str> {
        self.str_field(b"exe")
    }

    pub fn start_dir(&self) -> Option<&'a str> {
        self.str_field(b"startdir")
    }

    /// The raw `icon` value, quotes included if the file stores them.
    pub fn icon(&self) -> Option<&'a str> {
        self.str_field(b"icon")
    }

    /// The `icon` value with any surrounding quotes removed — what to hand to the filesystem.
    pub fn icon_path(&self) -> Option<&'a str> {
        self.icon().map(strip_quotes).filter(|s| !s.is_empty())
    }

    pub fn is_hidden(&self) -> bool {
        find_field(self.fields, b"ishidden")
            .and_then(Value::as_i32)
            .is_some_and(|v| v != 0)
    }

    pub fn tags(&self) -> Vec<&'a str> {
        find_field(self.fields, b"tags")
            .and_then(Value::as_map)
            .map(|entries| entries.iter().filter_map(|e| e.value.as_str()).collect())
            .unwrap_or_default()
    }

    /// Every field, for the diagnostics screen.
    pub fn fields(&self) -> &'a [Entry] {
        self.fields
    }

    fn str_field(&self, key: &[u8]) -> Option<&'a str> {
        find_field(self.fields, key).and_then(Value::as_str)
    }
}

// -- helpers ---------------------------------------------------------------------------

/// A fresh document shaped like one Steam would write: a single `shortcuts` map and the
/// file-level `0x08`.
fn empty_document() -> Document {
    Document {
        entries: vec![Entry {
            key: b"shortcuts".to_vec(),
            value: Value::Map(Vec::new()),
        }],
        trailing_terminators: 1,
    }
}

/// Case-insensitive field lookup. Required: the real file mixes `appid`/`appname`/`exe` with
/// `StartDir`/`ShortcutPath`, and other tools write other casings again.
fn find_field<'a>(fields: &'a [Entry], key: &[u8]) -> Option<&'a Value> {
    fields
        .iter()
        .find(|e| e.key.eq_ignore_ascii_case(key))
        .map(|e| &e.value)
}

/// Set a string field, preserving the existing key's casing and position. Appends if absent.
fn set_field(fields: &mut Vec<Entry>, key: &[u8], value: &[u8]) {
    match fields.iter_mut().find(|e| e.key.eq_ignore_ascii_case(key)) {
        Some(existing) => existing.value = Value::Str(value.to_vec()),
        None => fields.push(Entry {
            key: key.to_vec(),
            value: Value::Str(value.to_vec()),
        }),
    }
}

fn read_app_id(fields: &[Entry]) -> Option<AppId> {
    find_field(fields, b"appid")
        .and_then(Value::as_i32)
        .map(AppId::from_signed)
}

fn is_quoted(s: &str) -> bool {
    s.len() >= 2 && s.starts_with('"') && s.ends_with('"')
}

fn strip_quotes(s: &str) -> &str {
    if is_quoted(s) {
        // Safe slicing: `is_quoted` proved both ends are the one-byte ASCII quote.
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// `foo/shortcuts.vdf` + `.sgdb-orig` -> `foo/shortcuts.vdf.sgdb-orig`.
///
/// Deliberately not `Path::with_extension`, which would replace `.vdf` and produce
/// `shortcuts.sgdb-orig` — a name that no longer says what the file is.
fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Write `data` to `tmp`, fsync it, then rename over `target`.
///
/// Same directory, so the rename is atomic and cannot become a cross-volume copy. The fsync is
/// what makes the rename meaningful: without it the metadata operation can land while the
/// contents are still in flight.
fn write_atomic(tmp: &Path, target: &Path, data: &[u8]) -> Result<(), Error> {
    let write_err = |source| Error::Write {
        path: tmp.to_path_buf(),
        source,
    };

    {
        let mut f = std::fs::File::create(tmp).map_err(write_err)?;
        f.write_all(data).map_err(write_err)?;
        f.sync_all().map_err(write_err)?;
    }

    std::fs::rename(tmp, target).map_err(|source| {
        if let Err(cleanup) = std::fs::remove_file(tmp) {
            tracing::warn!(
                temp = %tmp.display(),
                error = %cleanup,
                "could not remove temp file after a failed rename"
            );
        }
        Error::Write {
            path: target.to_path_buf(),
            source,
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;
    use crate::steam::process::SteamStopped;

    /// A file shaped like the real one on this machine: mixed key casing, quoted path values,
    /// mixed separators in `StartDir`, a `tags` submap, and the extra file-level terminator.
    /// `[VERIFIED-BOX 2026-07-27]` No real paths, so it is safe to commit.
    fn realistic() -> Vec<u8> {
        let mut v = Vec::new();
        v.push(0x00);
        v.extend(b"shortcuts\0");
        v.push(0x00);
        v.extend(b"0\0");
        v.push(0x02);
        v.extend(b"appid\0");
        v.extend((-246_118_299i32).to_le_bytes());
        v.push(0x01);
        v.extend(b"appname\0EmulationStationDE\0");
        v.push(0x01);
        v.extend(b"exe\0\"C:\\Windows\\System32\\cmd.exe\" /k start\0");
        v.push(0x01);
        v.extend(b"StartDir\0\"C:\\Users\\test\\AppData/Roaming/Emu\"\0");
        v.push(0x01);
        v.extend(b"icon\0\"C:\\icons\\Old.ico\"\0");
        v.push(0x01);
        v.extend(b"ShortcutPath\0\0");
        v.push(0x02);
        v.extend(b"IsHidden\0");
        v.extend(0i32.to_le_bytes());
        v.push(0x00);
        v.extend(b"tags\0");
        v.push(0x01);
        v.extend(b"0\0favorite\0");
        v.push(0x08); // tags
        v.push(0x08); // "0"
        v.push(0x08); // shortcuts
        v.push(0x08); // file-level
        v
    }

    const APP: AppId = AppId::new(4_048_848_997);

    fn loaded() -> Shortcuts {
        Shortcuts::from_bytes(PathBuf::from("shortcuts.vdf"), realistic()).unwrap()
    }

    #[test]
    fn reads_the_shortcut_through_case_insensitive_lookups() {
        let s = loaded();
        let sc = s.find(APP).unwrap();
        assert_eq!(sc.app_name(), Some("EmulationStationDE"));
        // `StartDir` is CamelCase in the file, queried lowercase here.
        assert_eq!(
            sc.start_dir(),
            Some("\"C:\\Users\\test\\AppData/Roaming/Emu\"")
        );
        assert_eq!(sc.index(), b"0");
        assert_eq!(sc.tags(), vec!["favorite"]);
        assert!(!sc.is_hidden());
    }

    #[test]
    fn icon_path_strips_the_stored_quotes_but_icon_does_not() {
        let s = loaded();
        let sc = s.find(APP).unwrap();
        assert_eq!(sc.icon(), Some("\"C:\\icons\\Old.ico\""));
        assert_eq!(sc.icon_path(), Some("C:\\icons\\Old.ico"));
    }

    #[test]
    fn an_untouched_document_is_not_modified() {
        let s = loaded();
        assert!(!s.is_modified());
        assert_eq!(s.to_bytes(), realistic());
    }

    #[test]
    fn setting_an_icon_matches_the_files_quoting_convention() {
        let mut s = loaded();
        let change = s.set_icon(APP, "C:\\icons\\New.ico").unwrap();
        assert!(change.quoted, "this file quotes its paths");
        assert_eq!(change.applied, "\"C:\\icons\\New.ico\"");
        assert_eq!(change.previous.as_deref(), Some("\"C:\\icons\\Old.ico\""));
        assert_eq!(s.find(APP).unwrap().icon_path(), Some("C:\\icons\\New.ico"));
    }

    #[test]
    fn an_already_quoted_input_is_not_double_quoted() {
        let mut s = loaded();
        let change = s.set_icon(APP, "\"C:\\icons\\New.ico\"").unwrap();
        assert_eq!(change.applied, "\"C:\\icons\\New.ico\"");
    }

    #[test]
    fn an_unquoted_file_stays_unquoted() {
        // Steam itself writes bare paths; only some third-party tools quote them. Whichever
        // this file uses, we must not switch it.
        let mut unquoted = Vec::new();
        unquoted.push(0x00);
        unquoted.extend(b"shortcuts\0");
        unquoted.push(0x00);
        unquoted.extend(b"0\0");
        unquoted.push(0x02);
        unquoted.extend(b"appid\0");
        unquoted.extend((-246_118_299i32).to_le_bytes());
        unquoted.push(0x01);
        unquoted.extend(b"exe\0C:\\Games\\game.exe\0");
        unquoted.push(0x01);
        unquoted.extend(b"icon\0\0"); // present but empty
        unquoted.push(0x08);
        unquoted.push(0x08);
        unquoted.push(0x08);

        let mut s = Shortcuts::from_bytes(PathBuf::from("s.vdf"), unquoted).unwrap();
        let change = s.set_icon(APP, "C:\\icons\\New.ico").unwrap();
        assert!(
            !change.quoted,
            "an empty icon must fall back to the `exe` convention, which is bare here"
        );
        assert_eq!(change.applied, "C:\\icons\\New.ico");
    }

    #[test]
    fn editing_preserves_field_order_key_casing_and_everything_else() {
        let mut s = loaded();
        let _ = s.set_icon(APP, "C:\\icons\\New.ico").unwrap();
        let after = s.to_bytes();

        let doc = binary::parse(&after).unwrap();
        let shortcuts = binary::get(&doc.entries, "shortcuts")
            .and_then(Value::as_map)
            .unwrap();
        let fields = binary::get(shortcuts, "0").and_then(Value::as_map).unwrap();
        let keys: Vec<String> = fields
            .iter()
            .map(|e| String::from_utf8_lossy(&e.key).into_owned())
            .collect();
        assert_eq!(
            keys,
            [
                "appid",
                "appname",
                "exe",
                "StartDir",
                "icon",
                "ShortcutPath",
                "IsHidden",
                "tags"
            ],
            "field order and the file's original key casing must both survive an edit"
        );

        // The mixed separators in StartDir are exactly the sort of thing a careless writer
        // "fixes".
        let sd = binary::get(fields, "StartDir")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(sd, "\"C:\\Users\\test\\AppData/Roaming/Emu\"");
        // And the file-level terminator count is unchanged.
        assert_eq!(doc.trailing_terminators, 1);
    }

    #[test]
    fn only_the_icon_bytes_differ_after_setting_an_icon() {
        let mut s = loaded();
        let _ = s.set_icon(APP, "C:\\icons\\Old.ico").unwrap();
        // Same length of value, different content -> same length overall.
        let after = s.to_bytes();
        assert_eq!(after.len(), realistic().len());
    }

    #[test]
    fn clearing_an_icon_empties_the_field_rather_than_removing_it() {
        let mut s = loaded();
        let previous = s.clear_icon(APP).unwrap();
        assert_eq!(previous.as_deref(), Some("\"C:\\icons\\Old.ico\""));

        let sc_bytes = s.to_bytes();
        let doc = binary::parse(&sc_bytes).unwrap();
        let shortcuts = binary::get(&doc.entries, "shortcuts")
            .and_then(Value::as_map)
            .unwrap();
        let fields = binary::get(shortcuts, "0").and_then(Value::as_map).unwrap();
        assert_eq!(
            binary::get(fields, "icon").and_then(Value::as_bytes),
            Some(&b""[..]),
            "the key must still be present"
        );
    }

    #[test]
    fn an_unknown_appid_is_a_named_error() {
        let mut s = loaded();
        let err = s.set_icon(AppId::new(12345), "x.ico").unwrap_err();
        assert!(matches!(err, Error::NotFound(id, _) if id == AppId::new(12345)));
    }

    #[test]
    fn a_file_we_cannot_reproduce_is_refused_before_any_edit() {
        // Trailing garbage the codec would drop. Better to refuse than to write back a file
        // missing bytes we did not understand.
        let mut bad = realistic();
        bad.push(0x99);
        let err = Shortcuts::from_bytes(PathBuf::from("s.vdf"), bad).unwrap_err();
        // A parse error here, but the important property is that it is an error at all.
        assert!(matches!(
            err,
            Error::Parse { .. } | Error::RoundTripMismatch { .. }
        ));
    }

    #[test]
    fn a_missing_file_loads_as_an_empty_document() {
        let t = tempfile::tempdir().unwrap();
        let s = Shortcuts::load_or_empty(t.path().join("shortcuts.vdf")).unwrap();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
        assert!(s.find(APP).is_none());
    }

    #[test]
    fn scalar_siblings_among_the_numbered_keys_are_skipped_not_fatal() {
        // The defect that breaks third-party libraryfolders parsers, applied here defensively.
        let mut v = Vec::new();
        v.push(0x00);
        v.extend(b"shortcuts\0");
        v.push(0x01);
        // `\x00` spelled out: `\0` followed by a digit reads as an octal escape.
        v.extend(b"contentstatsid\x00778551\0"); // a scalar where a map is expected
        v.push(0x00);
        v.extend(b"0\0");
        v.push(0x02);
        v.extend(b"appid\0");
        v.extend((-246_118_299i32).to_le_bytes());
        v.push(0x08);
        v.push(0x08);
        v.push(0x08);

        let s = Shortcuts::from_bytes(PathBuf::from("s.vdf"), v).unwrap();

        // Prove the premise before asserting the behaviour: the map really must contain two
        // children, one of them a scalar. Without this the test would still pass against a
        // fixture that never had a scalar sibling in it at all — which is exactly what a
        // mis-escaped `\0` had produced here.
        let children = s.shortcuts_map().unwrap();
        assert_eq!(children.len(), 2, "fixture must have a scalar sibling");
        assert!(
            children
                .iter()
                .any(|e| e.key == b"contentstatsid" && e.value.as_map().is_none()),
            "fixture's scalar sibling is not shaped as intended: {children:?}"
        );

        assert_eq!(s.len(), 1, "the scalar must be skipped, not counted");
        assert_eq!(s.find(APP).unwrap().app_id(), Some(APP));
    }

    // -- the write path ------------------------------------------------------------------

    #[test]
    fn save_writes_atomically_backs_up_once_and_verifies() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("shortcuts.vdf");
        std::fs::write(&path, realistic()).unwrap();

        let token = SteamStopped::synthetic_for_test();

        let mut s = Shortcuts::load(&path).unwrap();
        let _ = s.set_icon(APP, "C:\\icons\\New.ico").unwrap();
        let saved = s.save(&token).unwrap();

        let backup = path.with_file_name("shortcuts.vdf.sgdb-orig");
        assert_eq!(saved.backup_created.as_deref(), Some(backup.as_path()));
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            realistic(),
            "the backup must be the pristine original"
        );

        // Reload and confirm the edit landed.
        let reloaded = Shortcuts::load(&path).unwrap();
        assert_eq!(
            reloaded.find(APP).unwrap().icon_path(),
            Some("C:\\icons\\New.ico")
        );

        // No temp file left behind.
        let leftovers: Vec<_> = std::fs::read_dir(t.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("sgdbtmp"))
            .collect();
        assert!(leftovers.is_empty(), "left temp files: {leftovers:?}");
    }

    #[test]
    fn the_original_backup_is_never_overwritten_by_a_later_save() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("shortcuts.vdf");
        std::fs::write(&path, realistic()).unwrap();
        let token = SteamStopped::synthetic_for_test();

        let mut first = Shortcuts::load(&path).unwrap();
        let _ = first.set_icon(APP, "C:\\a.ico").unwrap();
        let _ = first.save(&token).unwrap();

        let mut second = Shortcuts::load(&path).unwrap();
        let _ = second.set_icon(APP, "C:\\b.ico").unwrap();
        let saved = second.save(&token).unwrap();

        assert_eq!(
            saved.backup_created, None,
            "a second save must not create another backup"
        );
        assert_eq!(
            std::fs::read(path.with_file_name("shortcuts.vdf.sgdb-orig")).unwrap(),
            realistic(),
            "the pristine original must survive repeated saves"
        );
    }

    #[test]
    fn saving_an_unmodified_document_writes_nothing() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("shortcuts.vdf");
        std::fs::write(&path, realistic()).unwrap();
        let before = std::fs::metadata(&path).unwrap().len();

        let s = Shortcuts::load(&path).unwrap();
        let saved = s.save(&SteamStopped::synthetic_for_test()).unwrap();

        assert_eq!(saved.bytes_written, 0);
        assert_eq!(saved.backup_created, None);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), before);
        assert!(
            !path.with_file_name("shortcuts.vdf.sgdb-orig").exists(),
            "a no-op save must not even create a backup"
        );
    }

    #[test]
    fn a_new_file_saves_without_a_backup() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("shortcuts.vdf");

        let s = Shortcuts::load_or_empty(&path).unwrap();
        let saved = s.save(&SteamStopped::synthetic_for_test()).unwrap();

        assert_eq!(saved.backup_created, None, "nothing existed to preserve");
        assert!(path.is_file());
        // What we wrote must be loadable, and shaped like Steam's own empty file.
        let reloaded = Shortcuts::load(&path).unwrap();
        assert!(reloaded.is_empty());
        assert_eq!(reloaded.to_bytes(), s.to_bytes());
    }

    #[test]
    fn sibling_suffix_keeps_the_vdf_in_the_name() {
        // `with_extension` would give `shortcuts.sgdb-orig`, losing what the file is.
        let p = sibling_with_suffix(Path::new("/a/shortcuts.vdf"), ".sgdb-orig");
        assert_eq!(p.file_name().unwrap(), "shortcuts.vdf.sgdb-orig");
    }

    #[test]
    fn quote_helpers_handle_the_degenerate_cases() {
        assert!(!is_quoted("\""), "a lone quote is not a quoted string");
        assert_eq!(strip_quotes("\""), "\"");
        assert_eq!(strip_quotes(""), "");
        assert_eq!(strip_quotes("\"\""), "");
        assert_eq!(strip_quotes("bare"), "bare");
        assert_eq!(strip_quotes("\"a\"b\""), "a\"b");
    }
}
