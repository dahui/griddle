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
use crate::fsutil::{self, sibling_with_suffix};
use crate::sgdb::ApiKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The directory under `%APPDATA%` and `%LOCALAPPDATA%` that this app owns.
///
/// **The single definition.** `cache` imports it rather than keeping its own copy — the two used
/// to be separate constants kept in step by a hand-written comment, which is a drift hazard for
/// no benefit: a mismatch would split settings and cache across two directories, and nothing
/// would report it.
///
/// Changing this **relocates the user's settings and API key**, with no migration path in the
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

    /// Browsing tile width per asset type, in rem. `zoomlevel_<type>` in the Decky plugin.
    ///
    /// Keyed by the **wire** name — `grid_p`, `grid_l`, `hero`, `logo`, `icon` — not by
    /// [`crate::grid::AssetType`]'s display label, which is what this used to claim while nothing
    /// wrote to it. Every command that crosses the boundary speaks SteamGridDB's vocabulary, and
    /// a settings file mixing both would need a translation table to read.
    ///
    /// The **range lives in TypeScript**, in `ZOOM` in `@griddle/shared`, beside the stylesheet
    /// it describes; the frontend clamps on read. So an out-of-range value here is not corrupt,
    /// it is a choice made under different bounds, and it is kept rather than rewritten.
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
    /// Not the same as "owned" — see [`crate::steam::localconfig`]. There is no offline
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

/// Accept both the current shape and the older per-asset-type map.
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

/// Accept both the current shape and the older bare id.
///
/// An override was once just `{"620": 17830}`; it now carries the game's name alongside, so a
/// stored one can be labelled without a lookup.
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
/// **`Default` here is not the app's default filter set.** Every boolean defaults to `false`,
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
    /// Absent from an early version of this struct, which meant persisting a filter set
    /// silently dropped the user's size choice and the tab reverted to its defaults on reload.
    /// A field missing from a serialised struct fails this way — quietly, and only on reload.
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

        fsutil::write_atomic(&tmp, &self.path, &json).map_err(|e| Error::Write {
            path: e.path,
            source: e.source,
        })
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
