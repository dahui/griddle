//! What kind of thing an app is, and therefore whether it belongs in the library list.
//!
//! Reads `appcache/appinfo.vdf` through [`crate::vdf::appinfo`] and turns `common/type` into a
//! decision. Without it, "Steamworks Common Redistributables" and every Proton runtime sit in
//! the list looking like games you could put artwork on.
//!
//! # The failure direction is fixed: unknown means *show it*
//!
//! `appinfo.vdf` is an undocumented binary cache that Steam rewrites on its own schedule and
//! has already bumped the format of at least three times. So every uncertainty here resolves
//! toward showing the app:
//!
//! | Situation | Result |
//! |---|---|
//! | file missing, unreadable, or an unknown magic | **show everything** (blocklist only) |
//! | app absent from `appinfo.vdf` | **show it** |
//! | `common/type` absent or unrecognised | **show it** |
//! | `common/type` is a known non-game kind | hide it |
//!
//! A missing game is a bug the user reports as "your app is broken". A stray tool in the list
//! is a cosmetic annoyance. Those are not equally bad, and the code should not pretend they
//! are — which is why [`AppType::Other`] keeps the unrecognised string instead of collapsing
//! it to "not a game".
//!
//! The hardcoded blocklist in [`crate::steam::library`] stays as the floor, so the list is
//! still tolerable when `appinfo.vdf` cannot be read at all.

use crate::appid::AppId;
use crate::steam::library;
use crate::steam::locate::SteamInstall;
use crate::vdf::appinfo::{self, AppInfo};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: appinfo::Error,
    },
}

/// The values Steam puts in `common/type`.
///
/// Matched case-insensitively — Steam is not consistent about capitalisation across entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppType {
    Game,
    Application,
    Tool,
    Demo,
    Dlc,
    Music,
    Video,
    Config,
    Beta,
    Media,
    Series,
    Episode,
    Hardware,
    Mod,
    /// Something we have not seen. **Kept verbatim**, so a new Steam category shows up in
    /// diagnostics as itself rather than vanishing into a boolean.
    Other(String),
}

impl AppType {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "game" => AppType::Game,
            "application" => AppType::Application,
            "tool" => AppType::Tool,
            "demo" => AppType::Demo,
            "dlc" => AppType::Dlc,
            "music" => AppType::Music,
            "video" => AppType::Video,
            "config" => AppType::Config,
            "beta" => AppType::Beta,
            "media" => AppType::Media,
            "series" => AppType::Series,
            "episode" => AppType::Episode,
            "hardware" => AppType::Hardware,
            "mod" => AppType::Mod,
            _ => AppType::Other(raw.to_owned()),
        }
    }

    /// Whether a user would plausibly want to put artwork on this.
    ///
    /// `Other` is **true**: an unrecognised type is far more likely to be a new flavour of
    /// game than a new flavour of redistributable, and the cost of guessing wrong is
    /// asymmetric. See the module docs.
    pub fn belongs_in_library(&self) -> bool {
        match self {
            AppType::Game
            | AppType::Application
            | AppType::Demo
            | AppType::Beta
            | AppType::Mod
            | AppType::Other(_) => true,

            AppType::Tool
            | AppType::Dlc
            | AppType::Music
            | AppType::Video
            | AppType::Config
            | AppType::Media
            | AppType::Series
            | AppType::Episode
            | AppType::Hardware => false,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            AppType::Game => "Game",
            AppType::Application => "Application",
            AppType::Tool => "Tool",
            AppType::Demo => "Demo",
            AppType::Dlc => "DLC",
            AppType::Music => "Music",
            AppType::Video => "Video",
            AppType::Config => "Config",
            AppType::Beta => "Beta",
            AppType::Media => "Media",
            AppType::Series => "Series",
            AppType::Episode => "Episode",
            AppType::Hardware => "Hardware",
            AppType::Mod => "Mod",
            AppType::Other(s) => s,
        }
    }
}

/// A loaded `appinfo.vdf`, queried by appid.
#[derive(Debug, Clone)]
pub struct AppTypes {
    info: AppInfo,
    path: PathBuf,
}

impl AppTypes {
    /// Read and parse `appcache/appinfo.vdf`.
    ///
    /// Callers that want the degrade-on-failure behaviour should use [`AppTypes::load_or_none`]
    /// rather than handling the error themselves — it is easy to accidentally turn "could not
    /// read the cache" into "the library is empty".
    pub fn load(install: &SteamInstall) -> Result<Self, Error> {
        let path = install.appinfo_vdf();
        let data = std::fs::read(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        let info = appinfo::parse(&data).map_err(|source| Error::Parse {
            path: path.clone(),
            source,
        })?;
        tracing::info!(
            apps = info.apps.len(),
            skipped = info.skipped,
            version = ?info.version,
            "loaded appinfo.vdf"
        );
        Ok(AppTypes { info, path })
    }

    /// [`AppTypes::load`], reduced to `None` on any failure, with the reason logged.
    ///
    /// This is the shape every caller actually wants: `Option<&AppTypes>` threads straight into
    /// [`include_in_library`], which already treats `None` as "show everything".
    pub fn load_or_none(install: &SteamInstall) -> Option<Self> {
        match Self::load(install) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::warn!(error = %e, "appinfo.vdf unavailable; falling back to the blocklist");
                None
            }
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn version(&self) -> appinfo::Version {
        self.info.version
    }

    pub fn len(&self) -> usize {
        self.info.apps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.info.apps.is_empty()
    }

    /// Entries whose blob did not parse. Worth showing in diagnostics: a number that jumps
    /// after a Steam update is the earliest signal that the format moved.
    pub fn skipped(&self) -> usize {
        self.info.skipped
    }

    /// False when the entry list did not end where the string table begins, meaning apps were
    /// probably missed. Also belongs in diagnostics — it is the difference between "you own
    /// 50 games" and "we lost our place after 12".
    pub fn aligned(&self) -> bool {
        self.info.aligned
    }

    pub fn app_type(&self, app: AppId) -> Option<AppType> {
        self.info
            .apps
            .get(&app.get())?
            .common
            .app_type
            .as_deref()
            .map(AppType::parse)
    }

    /// The name Steam holds for this app. Not the same as the `appmanifest` name, which is
    /// what was current when the app was installed.
    pub fn name(&self, app: AppId) -> Option<&str> {
        self.info.apps.get(&app.get())?.common.name.as_deref()
    }

    /// sha1 of the client icon — the `.ico` under `Steam\steam\games\`.
    ///
    /// **Not** the librarycache icon. That is [`AppTypes::icon_sha1`], which is a different
    /// sha1 on the same app.
    pub fn client_icon(&self, app: AppId) -> Option<&str> {
        self.info
            .apps
            .get(&app.get())?
            .common
            .client_icon
            .as_deref()
            .filter(|s| !s.is_empty())
    }

    /// sha1 of the small icon at `librarycache/<appid>/<icon>.jpg`.
    pub fn icon_sha1(&self, app: AppId) -> Option<&str> {
        self.info
            .apps
            .get(&app.get())?
            .common
            .icon
            .as_deref()
            .filter(|s| !s.is_empty())
    }

    /// `common/header_image` — the store header's filename, nearly always `header.jpg`.
    pub fn header_image(&self, app: AppId, lang: &str) -> Option<&str> {
        self.info
            .apps
            .get(&app.get())?
            .common
            .header_image_for(lang)
            .filter(|s| !s.is_empty())
    }

    /// A `library_assets_full` slot's path, **relative to `librarycache/<appid>/`**.
    ///
    /// Untrusted and possibly stale: join it only through [`crate::steam::librarycache`], which
    /// guards the join and checks the file exists.
    pub fn library_asset(&self, app: AppId, slot: &str, lang: &str) -> Option<&str> {
        self.info
            .apps
            .get(&app.get())?
            .common
            .library_asset(slot, lang)
            .filter(|s| !s.is_empty())
    }
}

/// Should this app appear in the library list?
///
/// `types` is `None` when `appinfo.vdf` could not be read — in which case only the hardcoded
/// blocklist applies and everything else is shown. See the module docs for why every unknown
/// resolves toward showing.
pub fn include_in_library(types: Option<&AppTypes>, app: AppId) -> bool {
    if library::is_known_non_game(app) {
        return false;
    }
    match types.and_then(|t| t.app_type(app)) {
        Some(t) => t.belongs_in_library(),
        None => true,
    }
}

#[cfg(test)]
#[path = "apptype_tests.rs"]
mod tests;
