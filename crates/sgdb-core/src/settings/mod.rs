//! Persisted user settings. **One of only three modules allowed to write files.**
//!
//! Lives at `%APPDATA%\<AppName>\settings.json`, written atomically.
//!
//! # The API key is never stored in plaintext
//!
//! [`crate::sgdb::ApiKey`] deliberately implements no `Serialize`, so it *cannot* be written
//! into this file by accident — the only route in is [`Settings::set_api_key`], which wraps it
//! with DPAPI first and stores base64 of the ciphertext. That is the design working as
//! intended: the encryption is not something a future edit can forget, because the plaintext
//! type will not serialise at all.
//!
//! # Two failure modes, handled differently
//!
//! | Situation | What happens |
//! |---|---|
//! | file missing | defaults, no error — a first run is not a failure |
//! | file corrupt | **the bad file is kept**, renamed aside, and defaults are returned |
//! | key present but undecryptable | settings still load; only the key is dropped |
//!
//! That last row matters. The key is decrypted lazily rather than during load, so a settings
//! file written by a *different Windows account* still yields all the user's tab preferences
//! and filters — they just have to re-enter the key. Failing the whole load would look like
//! every setting had been lost.
//!
//! # Forward compatibility
//!
//! Every field is `#[serde(default)]`, so a file written by an older build loads cleanly and a
//! file written by a *newer* one loses only the fields this build does not know. A settings
//! file that fails to parse is the fastest way to make someone reconfigure everything, and it
//! is entirely avoidable.

pub mod dpapi;

use crate::sgdb::ApiKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// 🔵 **Placeholder.** The product name is undecided; this is a mechanical rename before
/// release, along with the crate names. See CLAUDE.md.
pub const APP_DIR_NAME: &str = "SteamGridDB Client";

/// Bumped only for a breaking schema change that needs a migration.
pub const SCHEMA_VERSION: u32 = 1;

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

    #[error("{path} is not valid settings JSON: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("serialising settings: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("could not locate %APPDATA%")]
    NoAppData,

    #[error(transparent)]
    Dpapi(#[from] dpapi::Error),

    #[error("the stored API key is not valid base64")]
    BadBase64,

    #[error("the decrypted API key is not valid text")]
    BadKeyText,
}

/// Where a Steam artwork slot's UI state is remembered, keyed by [`crate::grid::AssetType`]'s
/// label so the file stays readable.
pub type PerAssetType<T> = BTreeMap<String, T>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,

    /// DPAPI ciphertext, base64. **Never the key itself.**
    ///
    /// Private: the only way in or out is [`Settings::set_api_key`] / [`Settings::api_key`],
    /// which do the wrapping. A public field would be an invitation to assign a plaintext
    /// string to it.
    api_key_protected: Option<String>,

    /// Whether live apply over CDP is enabled. Off until the user opts in — turning it on is
    /// what creates the `.cef-enable-remote-debugging` sentinel.
    pub live_apply: bool,

    /// Tab order, hidden tabs, and which one opens first.
    pub tabs: TabSettings,

    /// `zoomlevel_<type>` in the Decky plugin.
    pub zoom: PerAssetType<f32>,

    /// `filters_<type>` in the Decky plugin.
    pub filters: PerAssetType<FilterState>,

    /// `nonsteam_<appid>`: a manual Steam-appid → SteamGridDB-game-id override, for when the
    /// automatic match is wrong or absent.
    pub game_overrides: BTreeMap<u32, u64>,

    /// Resolved Steam module map, keyed by the build it was resolved against.
    pub module_map: Option<ModuleMap>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: SCHEMA_VERSION,
            api_key_protected: None,
            live_apply: false,
            tabs: TabSettings::default(),
            zoom: PerAssetType::new(),
            filters: PerAssetType::new(),
            game_overrides: BTreeMap::new(),
            module_map: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabSettings {
    /// Asset-type labels in display order. Empty means "the built-in order".
    pub order: Vec<String>,
    pub hidden: Vec<String>,
    pub default_tab: Option<String>,
}

/// The content filters for one asset type.
///
/// Mirrors the Decky plugin's per-type filter state. The tag→query *inversion* lives in
/// `packages/shared/src/filters.ts` and is not duplicated in Rust; this is storage only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterState {
    pub untagged: bool,
    pub adult: bool,
    pub humor: bool,
    pub epilepsy: bool,
    pub styles: Vec<String>,
    pub mimes: Vec<String>,
    pub animated: bool,
    pub statik: bool,
}

/// A cached resolution of Steam's webpack modules, valid only for one client build.
///
/// The point of storing `clstamp` alongside is the diff: when Steam updates, re-run every
/// finder and compare against this, so a break becomes *"9 of 11 components re-found;
/// AppContextMenu not found — use the F8 hotkey"* rather than a silent failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleMap {
    /// Steam's build stamp, readable from both `steamui/changelist.txt` and the live page.
    pub clstamp: String,
    /// Finder name → where it resolved.
    pub entries: BTreeMap<String, ModuleRef>,
    /// Finders that found nothing on this build.
    #[serde(default)]
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleRef {
    pub module_id: String,
    /// The mangled export key, e.g. `HR`. Absent when the module itself is the value.
    #[serde(default)]
    pub export_key: Option<String>,
}

impl ModuleMap {
    /// Whether this cache applies to the build now running.
    pub fn matches(&self, clstamp: &str) -> bool {
        self.clstamp == clstamp
    }
}

impl Settings {
    /// Store the key, DPAPI-wrapped.
    pub fn set_api_key(&mut self, key: &ApiKey) -> Result<(), Error> {
        let sealed = dpapi::protect(key.expose().as_bytes())?;
        self.api_key_protected = Some(base64_encode(&sealed));
        Ok(())
    }

    pub fn clear_api_key(&mut self) {
        self.api_key_protected = None;
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key_protected.is_some()
    }

    /// Decrypt and return the key.
    ///
    /// `Ok(None)` when none is stored. An error means one *is* stored but could not be
    /// decrypted — usually because the file came from another Windows account, which the
    /// [`dpapi::Error::Unprotect`] message says explicitly.
    pub fn api_key(&self) -> Result<Option<ApiKey>, Error> {
        let Some(encoded) = &self.api_key_protected else {
            return Ok(None);
        };
        let sealed = base64_decode(encoded).ok_or(Error::BadBase64)?;
        let plain = dpapi::unprotect(&sealed)?;
        let text = String::from_utf8(plain).map_err(|_| Error::BadKeyText)?;
        // Reuse the same validation as a freshly pasted key rather than trusting the file.
        ApiKey::new(&text).map(Some).map_err(|_| Error::BadKeyText)
    }
}

/// Reads and writes `settings.json`.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    /// The production location, `%APPDATA%\<AppName>\settings.json`.
    pub fn default_location() -> Result<Self, Error> {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or(Error::NoAppData)?;
        Ok(Store {
            path: base.join(APP_DIR_NAME).join("settings.json"),
        })
    }

    /// An explicit location. Used by tests, and by a future portable mode.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Store { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read, or defaults if the file does not exist.
    pub fn load(&self) -> Result<Settings, Error> {
        let raw = match std::fs::read(&self.path) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
            Err(source) => {
                return Err(Error::Read {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        serde_json::from_slice(&raw).map_err(|source| Error::Parse {
            path: self.path.clone(),
            source,
        })
    }

    /// [`Store::load`], but a corrupt file is preserved and defaults are returned.
    ///
    /// Overwriting an unparseable settings file would destroy whatever the user could have
    /// recovered by hand — including, potentially, a key they no longer have anywhere else.
    /// It is renamed to `settings.json.corrupt` instead.
    pub fn load_or_default(&self) -> Settings {
        match self.load() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "settings could not be read; starting from defaults");
                let aside = sibling_with_suffix(&self.path, ".corrupt");
                match std::fs::rename(&self.path, &aside) {
                    Ok(()) => tracing::warn!(path = %aside.display(), "kept the unreadable file"),
                    Err(e) => {
                        tracing::warn!(error = %e, "could not preserve the unreadable file")
                    }
                }
                Settings::default()
            }
        }
    }

    /// Write atomically: temp file in the same directory, fsync, rename.
    pub fn save(&self, settings: &Settings) -> Result<(), Error> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let json = serde_json::to_vec_pretty(settings)?;
        let tmp = sibling_with_suffix(&self.path, TEMP_SUFFIX);

        let write_err = |source| Error::Write {
            path: tmp.clone(),
            source,
        };
        {
            let mut f = std::fs::File::create(&tmp).map_err(write_err)?;
            f.write_all(&json).map_err(write_err)?;
            f.sync_all().map_err(write_err)?;
        }

        std::fs::rename(&tmp, &self.path).map_err(|source| {
            if let Err(cleanup) = std::fs::remove_file(&tmp) {
                tracing::warn!(
                    temp = %tmp.display(),
                    error = %cleanup,
                    "could not remove temp file after a failed rename"
                );
            }
            Error::Write {
                path: self.path.clone(),
                source,
            }
        })
    }
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

// -- base64 --------------------------------------------------------------------------------
//
// Hand-rolled rather than pulling in a crate for ~30 lines. Standard alphabet with padding,
// which is all the DPAPI blob needs.

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
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
                // Padding is only legal at the end.
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
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    const FAKE_KEY: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn base64_round_trips_including_every_padding_case() {
        for len in 0..40usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
            let encoded = base64_encode(&data);
            assert_eq!(encoded.len() % 4, 0, "len {len} produced unpadded output");
            assert_eq!(base64_decode(&encoded).unwrap(), data, "len {len}");
        }
    }

    #[test]
    fn base64_matches_known_vectors() {
        // RFC 4648 test vectors — a hand-rolled codec that only agrees with itself is not
        // worth much.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(base64_decode("Zg==").unwrap(), b"f");
    }

    #[test]
    fn base64_rejects_malformed_input() {
        assert_eq!(base64_decode("Zg="), None, "wrong length");
        assert_eq!(base64_decode("Z!=="), None, "illegal character");
        assert_eq!(base64_decode("=Zm8"), None, "padding at the front");
        assert_eq!(base64_decode("====").unwrap_or_default(), Vec::<u8>::new());
    }

    #[test]
    fn defaults_round_trip_through_json() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
    }

    #[test]
    fn an_empty_object_loads_as_defaults() {
        // Forward compatibility: a file from an older build, or a hand-edited one, must not
        // fail to parse and cost the user every setting they have.
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn unknown_fields_from_a_newer_build_are_ignored() {
        let s: Settings =
            serde_json::from_str(r#"{"live_apply": true, "some_future_field": [1,2,3]}"#).unwrap();
        assert!(s.live_apply);
    }

    #[test]
    fn a_missing_file_loads_as_defaults_without_creating_anything() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));
        assert_eq!(store.load().unwrap(), Settings::default());
        assert!(!store.path().exists(), "load must not create the file");
    }

    #[test]
    fn save_then_load_preserves_everything() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("nested").join("settings.json"));

        let mut s = Settings {
            live_apply: true,
            ..Default::default()
        };
        let _ = s.zoom.insert("Capsule".into(), 1.75);
        let _ = s.game_overrides.insert(620, 17830);
        s.tabs.order = vec!["Hero".into(), "Capsule".into()];
        s.module_map = Some(ModuleMap {
            clstamp: "10840511".into(),
            entries: BTreeMap::from([(
                "Focusable".into(),
                ModuleRef {
                    module_id: "28869".into(),
                    export_key: Some("HR".into()),
                },
            )]),
            failed: vec!["SliderField".into()],
        });

        store.save(&s).unwrap();
        assert_eq!(store.load().unwrap(), s);
        assert!(store.path().is_file(), "parent directories must be created");
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));
        store.save(&Settings::default()).unwrap();

        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("sgdbtmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn a_corrupt_file_is_preserved_rather_than_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{ this is not json").unwrap();
        let store = Store::at(&path);

        assert!(store.load().is_err(), "load must surface the problem");

        let recovered = store.load_or_default();
        assert_eq!(recovered, Settings::default());

        let aside = dir.path().join("settings.json.corrupt");
        assert!(aside.is_file(), "the unreadable file must be kept");
        assert_eq!(
            std::fs::read(&aside).unwrap(),
            b"{ this is not json",
            "and kept verbatim — it may be the only copy of the user's key"
        );
    }

    #[test]
    fn the_module_map_only_matches_its_own_build() {
        let m = ModuleMap {
            clstamp: "10840511".into(),
            entries: BTreeMap::new(),
            failed: Vec::new(),
        };
        assert!(m.matches("10840511"));
        assert!(!m.matches("10850000"), "a new Steam build invalidates it");
    }

    // -- key handling. DPAPI is Windows-only, so these are too. -------------------------

    #[cfg(windows)]
    #[test]
    fn the_api_key_is_never_written_in_plaintext() {
        // The single most important assertion in this module: read the bytes actually on disk
        // and confirm the secret is not among them.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));

        let mut s = Settings::default();
        s.set_api_key(&ApiKey::new(FAKE_KEY).unwrap()).unwrap();
        store.save(&s).unwrap();

        let on_disk = std::fs::read(store.path()).unwrap();
        assert!(
            on_disk
                .windows(FAKE_KEY.len())
                .all(|w| w != FAKE_KEY.as_bytes()),
            "the API key was written in plaintext"
        );
        // And it really did store something.
        assert!(String::from_utf8_lossy(&on_disk).contains("api_key_protected"));
    }

    #[cfg(windows)]
    #[test]
    fn a_stored_key_round_trips_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));

        let mut s = Settings::default();
        assert!(!s.has_api_key());
        assert_eq!(s.api_key().unwrap(), None);

        s.set_api_key(&ApiKey::new(FAKE_KEY).unwrap()).unwrap();
        assert!(s.has_api_key());
        store.save(&s).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.api_key().unwrap().unwrap().expose(), FAKE_KEY);
    }

    #[cfg(windows)]
    #[test]
    fn clearing_the_key_removes_it_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));

        let mut s = Settings::default();
        s.set_api_key(&ApiKey::new(FAKE_KEY).unwrap()).unwrap();
        s.clear_api_key();
        store.save(&s).unwrap();

        assert!(!store.load().unwrap().has_api_key());
        let text = String::from_utf8(std::fs::read(store.path()).unwrap()).unwrap();
        assert!(text.contains("\"api_key_protected\": null"), "{text}");
    }

    #[cfg(windows)]
    #[test]
    fn an_undecryptable_key_does_not_take_the_rest_of_the_settings_with_it() {
        // A settings file copied from another Windows account. The user should lose the key,
        // not every preference they have set.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));

        let mut s = Settings {
            live_apply: true,
            ..Default::default()
        };
        s.set_api_key(&ApiKey::new(FAKE_KEY).unwrap()).unwrap();
        let _ = s.zoom.insert("Hero".into(), 2.0);
        store.save(&s).unwrap();

        // Corrupt only the ciphertext.
        let mut damaged = store.load().unwrap();
        damaged.api_key_protected = Some(base64_encode(b"not a real dpapi blob"));
        store.save(&damaged).unwrap();

        let loaded = store.load().unwrap();
        assert!(loaded.live_apply, "settings must still load");
        assert_eq!(loaded.zoom.get("Hero"), Some(&2.0));
        assert!(
            loaded.api_key().is_err(),
            "but the key itself must report a problem"
        );
    }
}
