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

use crate::base64;
use crate::sgdb::ApiKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The directory under `%APPDATA%` and `%LOCALAPPDATA%` that this app owns.
///
/// **The single definition.** `cache` imports it rather than keeping its own copy — the two used
/// to be separate constants kept in step by a hand-written comment, which is a drift hazard for
/// no benefit: a mismatch would split settings and cache across two directories, and nothing
/// would report it.
///
/// 🔴 Changing this **relocates the user's settings and API key**, with no migration path in the
/// code. That was acceptable exactly once, at the rename from the pre-release placeholder. Treat
/// any future change as a breaking one that needs a move written first.
pub const APP_DIR_NAME: &str = "Griddle";

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

    /// Tab order, hidden tabs, and which one opens first.
    pub tabs: TabSettings,

    /// `zoomlevel_<type>` in the Decky plugin.
    pub zoom: PerAssetType<f32>,

    /// The content filters, **shared by every asset type**.
    ///
    /// The Decky plugin keys these per type (`filters_<type>`) and so did this, until it became
    /// clear that having to re-pick "no adult content" five times is busywork rather than a
    /// feature. Values that only apply to some types — sizes especially — are held here as a
    /// union and clamped to the current type when the query is built, so switching tabs never
    /// discards a selection that the other tab could not show.
    ///
    /// `None` means the user has never changed them, which is *not* the same as
    /// [`FilterState::default`]: the app's defaults live in TypeScript, in one place.
    #[serde(default, deserialize_with = "filters_compat")]
    pub filters: Option<FilterState>,

    /// `nonsteam_<appid>`: a manual Steam appid → SteamGridDB game override, for when the
    /// automatic match is wrong or absent.
    #[serde(default, deserialize_with = "overrides_compat")]
    pub game_overrides: BTreeMap<u32, GameOverride>,

    /// Whether the library list shows only installed games, or everything Steam knows about.
    pub library_scope: LibraryScope,

    /// How the library list is ordered.
    pub library_sort: LibrarySort,
}

/// Which apps the library list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryScope {
    /// Apps with a fully-installed `appmanifest`. The default, because it is what you can
    /// actually play right now.
    #[default]
    Installed,
    /// Installed apps plus everything `localconfig.vdf` has a record for.
    ///
    /// 🔴 Not the same as "owned" — see [`crate::steam::localconfig`]. There is no offline
    /// ownership list, and this will both miss games you own but never launched and include
    /// ones whose license has lapsed.
    All,
}

/// How the library list is ordered.
///
/// The "all games" scope turns a 51-row list into a ~518-row one, at which point alphabetical
/// order stops being navigable — this is what makes that scope usable rather than merely large.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibrarySort {
    #[default]
    Name,
    RecentlyPlayed,
    MostPlayed,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: SCHEMA_VERSION,
            api_key_protected: None,
            tabs: TabSettings::default(),
            zoom: PerAssetType::new(),
            filters: None,
            game_overrides: BTreeMap::new(),
            library_scope: LibraryScope::default(),
            library_sort: LibrarySort::default(),
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

/// Which SteamGridDB game an appid pulls artwork from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameOverride {
    /// SteamGridDB's own game id — **not** a Steam appid.
    pub id: u64,
    /// The game's name as it read when the user chose it.
    ///
    /// Purely so the UI can say *"Cyberpunk 2077"* rather than *"SteamGridDB game #17830"*.
    /// Stored rather than looked up because the id is all the asset endpoints need, and this
    /// project does not ship an endpoint it has not probed against the live API. Optional so an
    /// override written before this field existed still loads.
    #[serde(default)]
    pub name: Option<String>,
}

/// Accept both the current shape and the pre-M4 per-asset-type map.
///
/// Filters used to be stored per type (`{"grid_p": {…}, "hero": {…}}`) and are one shared set
/// now. Simply changing the type would have serde read that old map as a `FilterState` with
/// every field missing — yielding all-`false`, which is **not** the app's defaults and would
/// look to the user like they had deliberately switched everything off. So the old shape is
/// recognised and the tab that opens first is carried across.
fn filters_compat<'de, D>(d: D) -> Result<Option<FilterState>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let Some(value) = Option::<serde_json::Value>::deserialize(d)? else {
        return Ok(None);
    };
    let Some(map) = value.as_object() else {
        return Ok(None);
    };
    // The old shape is the one whose every value is itself an object. A current `FilterState`
    // has booleans and arrays in it, so the two cannot be confused.
    if !map.is_empty() && map.values().all(serde_json::Value::is_object) {
        return Ok(map
            .get("grid_p")
            .or_else(|| map.values().next())
            .and_then(|v| serde_json::from_value(v.clone()).ok()));
    }
    Ok(serde_json::from_value(value).ok())
}

/// Accept both the current shape and the pre-M4 bare id.
///
/// Overrides used to be `{"620": 17830}`; they now carry the game's name alongside.
fn overrides_compat<'de, D>(d: D) -> Result<BTreeMap<u32, GameOverride>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = BTreeMap::<u32, serde_json::Value>::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|(app_id, value)| {
            if let Some(id) = value.as_u64() {
                return Some((app_id, GameOverride { id, name: None }));
            }
            serde_json::from_value::<GameOverride>(value)
                .ok()
                .map(|g| (app_id, g))
        })
        .collect())
}

/// The content filters, shared by every asset type.
///
/// The tag→query *inversion* lives in `packages/shared/src/filters.ts` and is not duplicated in
/// Rust; this is storage only.
///
/// 🔴 **`Default` here is not the app's default filter set.** Every boolean defaults to `false`,
/// whereas `defaultFilters()` in TypeScript turns most of them on. That is deliberate: the
/// defaults have one implementation, in the place that already tests them, and `Settings` stores
/// only what the user actually chose. Reimplementing them here would be a second source of truth
/// for the most error-prone rule in the product.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterState {
    pub untagged: bool,
    pub adult: bool,
    pub humor: bool,
    pub epilepsy: bool,
    pub styles: Vec<String>,
    /// Selected dimension filters, e.g. `600x900`.
    ///
    /// Was missing entirely until M4, so persisting a filter set silently dropped the user's
    /// dimension choice and the tab quietly reverted to its defaults on reload.
    pub dimensions: Vec<String>,
    pub mimes: Vec<String>,
    pub animated: bool,
    /// `static` on the wire — the TypeScript `Filters` interface calls it that, and it is a
    /// Rust keyword, so the field name and the JSON key cannot match.
    #[serde(rename = "static")]
    pub statik: bool,
}

impl Settings {
    /// Store the key, DPAPI-wrapped.
    pub fn set_api_key(&mut self, key: &ApiKey) -> Result<(), Error> {
        let sealed = dpapi::protect(key.expose().as_bytes())?;
        self.api_key_protected = Some(base64::encode(&sealed));
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
        let sealed = base64::decode(encoded).ok_or(Error::BadBase64)?;
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

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    const FAKE_KEY: &str = "0123456789abcdef0123456789abcdef";

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
        // `live_apply` is here on purpose: it was a real field until live apply stopped being
        // optional, so any existing settings file still has it. A removed field must be as
        // harmless as one from the future.
        let s: Settings = serde_json::from_str(
            r#"{"live_apply": true, "library_scope": "all", "some_future_field": [1,2,3]}"#,
        )
        .unwrap();
        assert_eq!(s.library_scope, LibraryScope::All);
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
            library_scope: LibraryScope::All,
            library_sort: LibrarySort::RecentlyPlayed,
            ..Default::default()
        };
        let _ = s.zoom.insert("Capsule".into(), 1.75);
        let _ = s.game_overrides.insert(
            620,
            GameOverride {
                id: 17830,
                name: Some("Portal 2".into()),
            },
        );
        s.filters = Some(FilterState {
            untagged: true,
            humor: true,
            styles: vec!["alternate".into()],
            dimensions: vec!["600x900".into()],
            animated: true,
            statik: true,
            ..Default::default()
        });
        s.tabs.order = vec!["Hero".into(), "Capsule".into()];

        store.save(&s).unwrap();
        assert_eq!(store.load().unwrap(), s);
        assert!(store.path().is_file(), "parent directories must be created");
    }

    #[test]
    fn filter_state_serialises_static_not_statik() {
        // The field cannot be called `static` in Rust, so the JSON key is a rename — and the
        // TypeScript side sends `static`. A mismatch here would not fail anything loudly: serde
        // would simply take the default and the user's static/animated choice would vanish on
        // every reload.
        let json = serde_json::to_string(&FilterState {
            statik: true,
            ..Default::default()
        })
        .unwrap();

        assert!(json.contains("\"static\":true"), "{json}");
        assert!(!json.contains("statik"), "{json}");

        // The other direction, which is the one that actually breaks: a payload written by the
        // frontend must deserialise.
        let back: FilterState =
            serde_json::from_str(r#"{"static":true,"animated":false}"#).unwrap();
        assert!(back.statik);
        assert!(!back.animated);
    }

    #[test]
    fn a_pre_m4_per_type_filter_map_is_carried_across_not_read_as_all_false() {
        // 🔴 The dangerous migration. Filters were stored per asset type and are one shared set
        // now. Without the shim serde reads the old map as a `FilterState` with every field
        // missing — all-`false`, which is not the app's defaults, and looks to the user like
        // they had switched every content filter off themselves.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));
        std::fs::write(
            store.path(),
            r#"{"version":1,"filters":{
                 "grid_p":{"untagged":true,"humor":true,"styles":["alternate"],"animated":true,"static":true},
                 "hero":{"untagged":false}
               }}"#,
        )
        .unwrap(); // boundary-ok: test fixture

        let loaded = store.load().unwrap();
        let Some(filters) = loaded.filters else {
            panic!("the old per-type map must carry across, not vanish");
        };

        // Premise and behaviour together: the carried values are grid_p's, and they are the
        // ones that would have been lost. All-false would satisfy none of these.
        assert!(filters.untagged);
        assert!(filters.humor);
        assert!(filters.statik);
        assert_eq!(filters.styles, vec!["alternate".to_owned()]);
    }

    #[test]
    fn the_current_flat_filter_shape_is_not_mistaken_for_the_old_map() {
        // The control for the shim's discriminator. A current `FilterState` has booleans and
        // arrays in it; the old shape had objects. Both must round-trip correctly, or the shim
        // would quietly eat every filter the user sets from now on.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));
        std::fs::write(
            store.path(),
            r#"{"version":1,"filters":{"untagged":false,"adult":true,"styles":["blurred"],"static":true}}"#,
        )
        .unwrap(); // boundary-ok: test fixture

        let filters = store.load().unwrap().filters.unwrap();
        assert!(!filters.untagged);
        assert!(filters.adult);
        assert!(filters.statik);
        assert_eq!(filters.styles, vec!["blurred".to_owned()]);
    }

    #[test]
    fn an_absent_filter_key_stays_none_rather_than_becoming_all_false() {
        // `None` means "never customised", which is what lets the frontend apply its own
        // defaults. Collapsing it to `FilterState::default()` here would silently turn every
        // content filter off for a first-run user.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));
        std::fs::write(store.path(), r#"{"version":1}"#).unwrap(); // boundary-ok: test fixture
        assert_eq!(store.load().unwrap().filters, None);
    }

    #[test]
    fn a_pre_m4_bare_override_id_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));
        std::fs::write(
            store.path(),
            r#"{"version":1,"game_overrides":{"620":17830,"440":{"id":123,"name":"TF2"}}}"#,
        )
        .unwrap(); // boundary-ok: test fixture

        let loaded = store.load().unwrap();
        // The old bare-id form keeps working, with no name to show.
        assert_eq!(
            loaded.game_overrides.get(&620),
            Some(&GameOverride {
                id: 17830,
                name: None
            }),
        );
        // The control: the current form parses too, so the shim is not swallowing everything.
        assert_eq!(
            loaded.game_overrides.get(&440),
            Some(&GameOverride {
                id: 123,
                name: Some("TF2".into())
            }),
        );
    }

    #[test]
    fn an_older_settings_file_without_the_m4_keys_still_loads() {
        // `#[serde(default)]` is what makes this true, and it is easy to lose. A settings file
        // written before the library scope existed must not fail to load.
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("settings.json"));
        std::fs::write(store.path(), r#"{"version":1,"zoom":{"Hero":2.0}}"#).unwrap(); // boundary-ok: test fixture

        let loaded = store.load().unwrap();
        assert_eq!(
            loaded.zoom.get("Hero"),
            Some(&2.0),
            "the keys that were present must survive"
        );
        assert_eq!(loaded.library_scope, LibraryScope::Installed);
        assert_eq!(loaded.library_sort, LibrarySort::Name);
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
            library_scope: LibraryScope::All,
            ..Default::default()
        };
        s.set_api_key(&ApiKey::new(FAKE_KEY).unwrap()).unwrap();
        let _ = s.zoom.insert("Hero".into(), 2.0);
        store.save(&s).unwrap();

        // Corrupt only the ciphertext.
        let mut damaged = store.load().unwrap();
        damaged.api_key_protected = Some(base64::encode(b"not a real dpapi blob"));
        store.save(&damaged).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(
            loaded.library_scope,
            LibraryScope::All,
            "settings must still load"
        );
        assert_eq!(loaded.zoom.get("Hero"), Some(&2.0));
        assert!(
            loaded.api_key().is_err(),
            "but the key itself must report a problem"
        );
    }
}
