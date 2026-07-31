//! Application state, assembled once at startup.
//!
//! # Nothing here is allowed to prevent the window opening
//!
//! Steam may not be installed. `appinfo.vdf` may be a format we do not know. No API key may
//! have been entered yet. Every one of those is *ordinary* on a first run, and a shell that
//! refuses to start leaves the user with no way to find out what is wrong — the diagnostics
//! screen is part of the product precisely because most failures here are environmental.
//!
//! So startup resolves what it can, records what it could not, and hands the UI a report.

use griddle_core::cache::Cache;
use griddle_core::settings::{Settings, Store};
use griddle_core::sgdb;
use griddle_core::steam::{Account, AppTypes, SteamInstall, account, apptype, locate};
use tokio::sync::Mutex;

/// Everything about the local Steam installation, when it could be found.
pub struct SteamContext {
    pub install: SteamInstall,
    pub account: Account,
    /// `None` when `appinfo.vdf` could not be read. The library still lists games; the
    /// hardcoded blocklist does the filtering instead.
    pub app_types: Option<AppTypes>,
}

pub struct AppState {
    /// `Err` holds the reason, so the UI can show it rather than an empty library.
    pub steam: Result<SteamContext, String>,
    pub store: Store,
    pub settings: Mutex<Settings>,
    pub cache: Cache,
    pub http: reqwest::Client,
    /// Built lazily: there is no client until a key exists.
    pub sgdb: Mutex<Option<sgdb::Client>>,
    /// Steam appid → which SteamGridDB game it resolves to, for this session.
    ///
    /// Resolving can cost a name search (see `commands::resolve_game`), and the asset browser
    /// asks twice per game — once to label the "Wrong game?" button and once to fetch artwork.
    /// Caching makes that one request instead of two, and keeps paging through 700 assets from
    /// re-resolving on every page.
    ///
    /// **In memory, not in `settings.json`, on purpose.** A name match is a guess; persisting it
    /// would silently enshrine a wrong one where the user would have to notice and undo it. The
    /// only stored mapping is the one they chose themselves.
    ///
    /// `None` means "resolved to nothing" — cached too, so a game SteamGridDB simply does not
    /// have does not re-search on every page.
    pub game_matches: Mutex<std::collections::HashMap<u32, Option<crate::commands::GameMatch>>>,
}

impl AppState {
    pub fn load() -> Self {
        let steam = Self::load_steam();
        if let Err(e) = &steam {
            tracing::warn!(error = %e, "Steam not available; starting in a degraded state");
        }

        let store = Store::default_location().unwrap_or_else(|_| {
            // No %APPDATA% is close to impossible on Windows, but falling back to a relative
            // path keeps the app usable rather than dead.
            tracing::warn!("%APPDATA% unavailable; settings will be stored beside the exe");
            Store::at("settings.json")
        });
        // `load_or_default` preserves an unreadable file instead of overwriting it.
        let settings = store.load_or_default();

        let cache = Cache::default_location().unwrap_or_else(|_| Cache::at("cache"));

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_default();

        // A stored key that cannot be decrypted (a settings file from another Windows account)
        // must not stop startup — the user just has to re-enter it.
        let client = match settings.api_key() {
            Ok(Some(key)) => match sgdb::Client::new(key) {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::warn!(error = %e, "could not build the SteamGridDB client");
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(error = %e, "stored API key could not be read");
                None
            }
        };

        AppState {
            steam,
            store,
            settings: Mutex::new(settings),
            cache,
            http,
            sgdb: Mutex::new(client),
            game_matches: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn load_steam() -> Result<SteamContext, String> {
        let install = locate::locate().map_err(|e| e.to_string())?;
        let account = account::resolve(&install).map_err(|e| e.to_string())?;
        // Degrades to `None` on any failure, with the reason logged — never to an empty
        // library.
        let app_types = apptype::AppTypes::load_or_none(&install);
        Ok(SteamContext {
            install,
            account,
            app_types,
        })
    }

    pub fn steam(&self) -> Result<&SteamContext, crate::error::UiError> {
        self.steam
            .as_ref()
            .map_err(|e| crate::error::UiError::steam_not_found(e.clone()))
    }

    /// The artwork directory for the signed-in account.
    pub fn grid_dir(&self) -> Result<std::path::PathBuf, crate::error::UiError> {
        let ctx = self.steam()?;
        Ok(ctx.install.grid_dir(ctx.account.id))
    }
}
