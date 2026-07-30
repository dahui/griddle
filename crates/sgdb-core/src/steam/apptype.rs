//! What kind of thing an app is, and therefore whether it belongs in the library list.
//!
//! Reads `appcache/appinfo.vdf` through [`crate::vdf::appinfo`] and turns `common/type` into a
//! decision. Without it, "Steamworks Common Redistributables" and every Proton runtime sit in
//! the list looking like games you could put artwork on.
//!
//! # 🔑 The failure direction is fixed: unknown means *show it*
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

    /// sha1 of the client icon, for locating it under `appcache/librarycache`.
    pub fn client_icon(&self, app: AppId) -> Option<&str> {
        self.info
            .apps
            .get(&app.get())?
            .common
            .client_icon
            .as_deref()
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
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    #[test]
    fn known_types_parse_case_insensitively() {
        assert_eq!(AppType::parse("Game"), AppType::Game);
        assert_eq!(AppType::parse("game"), AppType::Game);
        assert_eq!(AppType::parse("  TOOL  "), AppType::Tool);
        assert_eq!(AppType::parse("DLC"), AppType::Dlc);
    }

    #[test]
    fn an_unknown_type_keeps_its_text_and_is_still_shown() {
        // The asymmetry that matters: a Steam category we have never seen must not silently
        // remove a game from the user's library.
        let t = AppType::parse("Holodeck");
        assert_eq!(t, AppType::Other("Holodeck".into()));
        assert_eq!(t.label(), "Holodeck");
        assert!(
            t.belongs_in_library(),
            "unknown must resolve toward showing"
        );
    }

    #[test]
    fn tools_and_dlc_are_hidden_but_games_and_apps_are_not() {
        for t in ["Game", "Application", "Demo", "Beta", "Mod"] {
            assert!(AppType::parse(t).belongs_in_library(), "{t} should show");
        }
        for t in ["Tool", "DLC", "Music", "Video", "Config", "Hardware"] {
            assert!(!AppType::parse(t).belongs_in_library(), "{t} should hide");
        }
    }

    #[test]
    fn with_no_appinfo_at_all_everything_but_the_blocklist_is_shown() {
        // The degraded path: it must still hide the redistributables we know by id, and must
        // not hide anything else.
        assert!(include_in_library(None, AppId::new(620)));
        assert!(include_in_library(None, AppId::new(1_004_640)));
        assert!(!include_in_library(None, AppId::new(228_980)));
    }

    /// A minimal v29 file, built the same way `vdf::appinfo`'s own tests do.
    fn types_for(apps: &[(u32, &str)]) -> AppTypes {
        let strings = ["appinfo", "common", "type"];
        let idx = |s: &str| strings.iter().position(|x| *x == s).unwrap_or(0) as u32;

        let mut body = Vec::new();
        for (id, ty) in apps {
            let mut blob = Vec::new();
            blob.push(0x00);
            blob.extend(idx("appinfo").to_le_bytes());
            blob.push(0x00);
            blob.extend(idx("common").to_le_bytes());
            blob.push(0x01);
            blob.extend(idx("type").to_le_bytes());
            blob.extend(ty.as_bytes());
            blob.push(0);
            blob.push(0x08);
            blob.push(0x08);

            let mut payload = Vec::new();
            payload.extend(1u32.to_le_bytes());
            payload.extend(0u32.to_le_bytes());
            payload.extend(0u64.to_le_bytes());
            payload.extend([0u8; 20]);
            payload.extend(0u32.to_le_bytes());
            payload.extend([0u8; 20]);
            payload.extend(&blob);

            body.extend(id.to_le_bytes());
            body.extend((payload.len() as u32).to_le_bytes());
            body.extend(&payload);
        }
        body.extend(0u32.to_le_bytes());

        let mut table = Vec::new();
        table.extend((strings.len() as u32).to_le_bytes());
        for s in strings {
            table.extend(s.as_bytes());
            table.push(0);
        }

        let mut data = Vec::new();
        data.extend(appinfo::MAGIC_V29.to_le_bytes());
        data.extend(1u32.to_le_bytes());
        data.extend(((4 + 4 + 8 + body.len()) as i64).to_le_bytes());
        data.extend(&body);
        data.extend(&table);

        AppTypes {
            info: appinfo::parse(&data).unwrap(),
            path: PathBuf::from("appinfo.vdf"),
        }
    }

    #[test]
    fn a_tool_is_hidden_once_appinfo_is_available() {
        let t = types_for(&[(620, "Game"), (1234, "Tool")]);
        assert_eq!(t.app_type(AppId::new(620)), Some(AppType::Game));
        assert_eq!(t.app_type(AppId::new(1234)), Some(AppType::Tool));

        assert!(include_in_library(Some(&t), AppId::new(620)));
        assert!(
            !include_in_library(Some(&t), AppId::new(1234)),
            "appinfo must be able to hide a tool the blocklist has never heard of"
        );
    }

    #[test]
    fn an_app_missing_from_appinfo_is_still_shown() {
        let t = types_for(&[(620, "Game")]);
        assert_eq!(t.app_type(AppId::new(999_999)), None);
        assert!(
            include_in_library(Some(&t), AppId::new(999_999)),
            "absence from the cache must never hide an installed game"
        );
    }

    #[test]
    fn the_blocklist_still_applies_even_when_appinfo_calls_it_a_game() {
        // Belt and braces: if appinfo ever labels a redistributable "Game", the id-based
        // floor must still win.
        let t = types_for(&[(228_980, "Game")]);
        assert!(!include_in_library(Some(&t), AppId::new(228_980)));
    }
}
