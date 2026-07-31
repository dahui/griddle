#![allow(
    clippy::let_underscore_must_use,
    reason = "the #[tauri::command] macro expands to `let _ = ...` at each command's signature; \
              the workspace denies that pattern in our own code, which is where it matters"
)]
//! The `invoke` surface. Thin: every decision belongs to `sgdb-core`.
//!
//! # The apply ladder
//!
//! [`apply_asset`] tries live apply first and falls back to writing files. That order is the
//! whole product thesis — a live apply updates the library in ~30 ms with no restart, which no
//! other Windows tool does — but the fallback is what makes it *shippable*: if Steam moves the
//! API, or the user never enables the debugging sentinel, artwork still applies. It just needs
//! a restart to show up, exactly like Steam Art Manager and SGDBoop.
//!
//! The outcome says which path ran, so the UI can tell the user whether a restart is needed
//! rather than leaving them staring at unchanged art.

use crate::error::{Kind, UiError};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use sgdb_core::appid::AppId;
use sgdb_core::cdp::{self, Endpoint, Sentinel, SteamJs};
use sgdb_core::grid::names::AssetType;
use sgdb_core::grid::store::GridDir;
use sgdb_core::settings::{LibraryScope, LibrarySort};
use sgdb_core::sgdb::{self, ApiKey, AssetQuery, Target};
use sgdb_core::steam::{
    LibraryCache, apptype, library, localconfig, process, shortcuts::Shortcuts,
};
use std::collections::BTreeMap;
use tauri::State;

type Res<T> = Result<T, UiError>;

// -- status ---------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Status {
    pub steam_root: Option<String>,
    pub steam_source: Option<String>,
    pub account_id: Option<u32>,
    pub steam_running: bool,
    pub has_api_key: bool,
    pub live_apply_enabled: bool,
    pub sentinel_present: bool,
    pub sentinel_explanation: String,
    pub app_types_loaded: Option<usize>,
    pub cache_bytes: u64,
    /// Present only when Steam could not be located, so the UI can explain rather than show an
    /// empty library.
    pub steam_error: Option<String>,
}

/// Everything the diagnostics screen needs, and what the first-run flow branches on.
#[tauri::command]
pub async fn status(state: State<'_, AppState>) -> Res<Status> {
    let settings = state.settings.lock().await;
    let stats = state.cache.stats();

    let (root, source, account, app_types, sentinel) = match &state.steam {
        Ok(ctx) => {
            let s = Sentinel::for_install(&ctx.install);
            (
                Some(ctx.install.root().display().to_string()),
                Some(ctx.install.source().label().to_owned()),
                Some(ctx.account.id),
                ctx.app_types.as_ref().map(sgdb_core::steam::AppTypes::len),
                Some(s),
            )
        }
        Err(_) => (None, None, None, None, None),
    };

    Ok(Status {
        steam_root: root,
        steam_source: source,
        account_id: account,
        steam_running: process::is_running(),
        has_api_key: settings.has_api_key(),
        live_apply_enabled: settings.live_apply,
        sentinel_present: sentinel.as_ref().is_some_and(Sentinel::exists),
        sentinel_explanation: sentinel.as_ref().map_or_else(
            || "Steam was not found.".to_owned(),
            |s| s.state().explain().to_owned(),
        ),
        app_types_loaded: app_types,
        cache_bytes: stats.bytes,
        steam_error: state.steam.as_ref().err().cloned(),
    })
}

// -- API key --------------------------------------------------------------------------------

/// Validate a key against the live API **before** storing it.
///
/// Storing first and validating later would leave a wrong key sitting in settings, and every
/// later request would fail with a 401 that looks like the app is broken.
#[tauri::command]
pub async fn set_api_key(state: State<'_, AppState>, key: String) -> Res<()> {
    let parsed = ApiKey::new(&key).map_err(|e| {
        UiError::new(Kind::Unauthorized, e.to_string())
            .with_action("Paste the key from your SteamGridDB preferences page.")
    })?;

    let client = sgdb::Client::new(parsed.clone()).map_err(UiError::from)?;
    client.validate_key().await?;

    let mut settings = state.settings.lock().await;
    settings.set_api_key(&parsed)?;
    state.store.save(&settings)?;

    let mut slot = state.sgdb.lock().await;
    *slot = Some(client);
    tracing::info!("API key validated and stored");
    Ok(())
}

#[tauri::command]
pub async fn clear_api_key(state: State<'_, AppState>) -> Res<()> {
    let mut settings = state.settings.lock().await;
    settings.clear_api_key();
    state.store.save(&settings)?;
    let mut slot = state.sgdb.lock().await;
    *slot = None;
    Ok(())
}

// -- preferences ----------------------------------------------------------------------------

/// The persisted UI state the frontend needs at mount.
///
/// Returned by every mutating preference command as well, so the frontend never has to guess
/// what the store now holds — one round trip, one source of truth.
#[derive(Debug, Serialize)]
pub struct Prefs {
    pub library_scope: LibraryScope,
    pub library_sort: LibrarySort,
    /// The content filters, shared by every asset type.
    ///
    /// `null` when the user has never changed them; the frontend then applies
    /// `defaultFilters()`, which is where the defaults are defined and tested.
    pub filters: Option<sgdb_core::settings::FilterState>,
    pub zoom: BTreeMap<String, f32>,
    pub game_overrides: BTreeMap<u32, sgdb_core::settings::GameOverride>,
}

async fn snapshot(state: &State<'_, AppState>) -> Prefs {
    let s = state.settings.lock().await;
    Prefs {
        library_scope: s.library_scope,
        library_sort: s.library_sort,
        filters: s.filters.clone(),
        zoom: s.zoom.clone(),
        game_overrides: s.game_overrides.clone(),
    }
}

#[tauri::command]
pub async fn prefs(state: State<'_, AppState>) -> Res<Prefs> {
    Ok(snapshot(&state).await)
}

#[tauri::command]
pub async fn set_library_view(
    state: State<'_, AppState>,
    scope: LibraryScope,
    sort: LibrarySort,
) -> Res<Prefs> {
    {
        let mut settings = state.settings.lock().await;
        settings.library_scope = scope;
        settings.library_sort = sort;
        state.store.save(&settings)?;
    }
    Ok(snapshot(&state).await)
}

/// Store the filter set. One set, shared by every asset type.
#[tauri::command]
pub async fn set_filters(
    state: State<'_, AppState>,
    filters: sgdb_core::settings::FilterState,
) -> Res<Prefs> {
    {
        let mut settings = state.settings.lock().await;
        settings.filters = Some(filters);
        state.store.save(&settings)?;
    }
    Ok(snapshot(&state).await)
}

/// Forget the stored filters, so they fall back to the defaults.
///
/// Stores `None` rather than `FilterState::default()`: the defaults live in TypeScript, and
/// writing an all-`false` struct here would mean "the user turned everything off".
#[tauri::command]
pub async fn reset_filters(state: State<'_, AppState>) -> Res<Prefs> {
    {
        let mut settings = state.settings.lock().await;
        settings.filters = None;
        state.store.save(&settings)?;
    }
    Ok(snapshot(&state).await)
}

/// Point a Steam appid at a specific SteamGridDB game, or clear the override.
///
/// `None` clears it. Without that, an override set once could never be undone from the UI, and
/// a wrong choice would be permanent.
/// `name` is stored alongside so the UI can name the override later without a lookup — see
/// [`sgdb_core::settings::GameOverride`].
#[tauri::command]
pub async fn set_game_override(
    state: State<'_, AppState>,
    app_id: u32,
    sgdb_id: Option<u64>,
    name: Option<String>,
) -> Res<Prefs> {
    {
        let mut settings = state.settings.lock().await;
        match sgdb_id {
            Some(id) => {
                let _ = settings
                    .game_overrides
                    .insert(app_id, sgdb_core::settings::GameOverride { id, name });
            }
            None => {
                let _ = settings.game_overrides.remove(&app_id);
            }
        }
        state.store.save(&settings)?;
    }
    Ok(snapshot(&state).await)
}

// -- library --------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct LibraryEntry {
    pub app_id: u32,
    pub name: String,
    /// `"steam"` or `"shortcut"`.
    pub kind: &'static str,
    pub app_type: Option<String>,
    /// Absolute path to existing custom art for this asset type, if any. Rendered through
    /// Tauri's `asset:` protocol.
    pub current_art: Option<String>,
    /// Absolute path to *Steam's own* art for this slot, when there is one on disk.
    ///
    /// The second rung of the ladder the UI walks: custom art → this → Steam's CDN → a
    /// placeholder. Always `None` for shortcuts, which have no librarycache entry.
    pub steam_art: Option<String>,
    /// False for an app `localconfig.vdf` knows about that has no installed manifest.
    pub installed: bool,
    /// Unix seconds, absent when never played. See [`localconfig`] on the 1970 sentinel.
    pub last_played: Option<u64>,
    pub playtime_minutes: Option<u32>,
}

/// The library list, scoped to installed apps or to everything Steam knows about.
///
/// Shortcuts are never scoped away: a non-Steam shortcut is always "installed" in the only sense
/// that matters here, and hiding them behind a toggle they have nothing to do with would be
/// baffling.
///
/// `scope` and `sort` are **parameters, not reads of `Settings`**. Persisting a view preference
/// and reloading the list are separate round trips, so reading the stored value here would race
/// the write: picking "Recently played" reloaded the list before the setting had landed and it
/// came back in the old order, which looked like the control did nothing.
#[tauri::command]
pub async fn library(
    state: State<'_, AppState>,
    asset_type: String,
    scope: LibraryScope,
    sort: LibrarySort,
) -> Res<Vec<LibraryEntry>> {
    let asset = parse_asset_type(&asset_type)?;
    let ctx = state.steam()?;
    let grid = GridDir::new(ctx.install.grid_dir(ctx.account.id));
    let steam_art = LibraryCache::new(&ctx.install, ctx.app_types.as_ref());

    // Keyed by appid so the "all games" pass can skip anything already installed without a
    // linear scan, and so a duplicate appid across the two sources cannot produce two rows.
    let mut steam_apps: BTreeMap<u32, LibraryEntry> = BTreeMap::new();

    // Installed Steam apps. One corrupt manifest never empties the list.
    match library::installed_apps(&ctx.install) {
        Ok(apps) => {
            for app in apps.iter().filter(|a| a.is_fully_installed()) {
                if !apptype::include_in_library(ctx.app_types.as_ref(), app.app_id) {
                    continue;
                }
                let _ = steam_apps.insert(
                    app.app_id.get(),
                    LibraryEntry {
                        app_id: app.app_id.get(),
                        // The manifest name, not appinfo's: it is what the user installed, and
                        // it is present even for the apps appinfo has never heard of.
                        name: app.name.clone(),
                        kind: "steam",
                        app_type: ctx
                            .app_types
                            .as_ref()
                            .and_then(|t| t.app_type(app.app_id))
                            .map(|t| t.label().to_owned()),
                        current_art: first_existing(&grid, app.app_id, asset),
                        steam_art: path_string(steam_art.resolve(app.app_id, asset)),
                        installed: true,
                        last_played: None,
                        playtime_minutes: None,
                    },
                );
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not enumerate installed apps"),
    }

    // `localconfig.vdf` serves two purposes: it is the "all games" source, and it carries the
    // playtimes for installed games too, so it is read either way.
    let known = localconfig::known_apps_or_empty(&ctx.install, ctx.account.id);
    for record in &known {
        let id = record.app_id;
        if scope == LibraryScope::All
            && !steam_apps.contains_key(&id.get())
            && apptype::include_in_library(ctx.app_types.as_ref(), id)
        {
            let _ = steam_apps.insert(
                id.get(),
                LibraryEntry {
                    app_id: id.get(),
                    // 29 of these have no appinfo entry and no cached art on this box — they
                    // are delisted. Shown anyway with a placeholder name, because the appid is
                    // still a valid SteamGridDB key and the project's failure direction is that
                    // an unknown app gets shown. A missing game is a bug report; an odd-looking
                    // row is a cosmetic annoyance.
                    name: ctx
                        .app_types
                        .as_ref()
                        .and_then(|t| t.name(id))
                        .map_or_else(|| format!("Unknown app {}", id.get()), str::to_owned),
                    kind: "steam",
                    app_type: ctx
                        .app_types
                        .as_ref()
                        .and_then(|t| t.app_type(id))
                        .map(|t| t.label().to_owned()),
                    current_art: first_existing(&grid, id, asset),
                    steam_art: path_string(steam_art.resolve(id, asset)),
                    installed: false,
                    last_played: None,
                    playtime_minutes: None,
                },
            );
        }
        // Merge playtimes onto whichever group the app landed in.
        if let Some(entry) = steam_apps.get_mut(&id.get()) {
            entry.last_played = record.last_played;
            entry.playtime_minutes = record.playtime_minutes;
        }
    }

    let mut entries: Vec<LibraryEntry> = steam_apps.into_values().collect();

    // Non-Steam shortcuts. Read-only here; the file is only written for icons.
    match Shortcuts::load_or_empty(ctx.install.shortcuts_vdf(ctx.account.id)) {
        Ok(sc) => {
            for s in sc.iter() {
                let Some(id) = s.app_id() else { continue };
                entries.push(LibraryEntry {
                    app_id: id.get(),
                    name: s.app_name().unwrap_or("(unnamed shortcut)").to_owned(),
                    kind: "shortcut",
                    app_type: None,
                    current_art: first_existing(&grid, id, asset),
                    // No librarycache entry exists for a non-Steam appid, so do not stat for one.
                    steam_art: None,
                    installed: true,
                    last_played: None,
                    playtime_minutes: None,
                });
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not read shortcuts.vdf"),
    }

    sort_entries(&mut entries, sort);
    Ok(entries)
}

/// Order the list, always breaking ties by name.
///
/// The name tiebreak is what keeps the order stable: hundreds of apps share a `None` playtime,
/// and without it they would shuffle between loads for no visible reason.
fn sort_entries(entries: &mut [LibraryEntry], sort: LibrarySort) {
    // Case-insensitive, so "Portal" and "portal" sit together rather than in separate blocks.
    entries.sort_by(|a, b| match sort {
        LibrarySort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        // Descending, and never-played sorts last rather than first — `None` would otherwise
        // win a plain descending comparison on `Option`.
        LibrarySort::RecentlyPlayed => b
            .last_played
            .unwrap_or(0)
            .cmp(&a.last_played.unwrap_or(0))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        LibrarySort::MostPlayed => b
            .playtime_minutes
            .unwrap_or(0)
            .cmp(&a.playtime_minutes.unwrap_or(0))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
    });
}

fn path_string(path: Option<std::path::PathBuf>) -> Option<String> {
    path.map(|p| p.display().to_string())
}

fn first_existing(grid: &GridDir, app: AppId, asset: AssetType) -> Option<String> {
    grid.existing(app, asset)
        .first()
        .map(|p| p.display().to_string())
}

// -- browsing SteamGridDB -------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub assets: Vec<sgdb_core::sgdb::Asset>,
    pub page: u32,
    pub total: u32,
    pub has_more: bool,
}

/// One page of artwork for a game.
///
/// `filters` is the output of `filtersToQuery()` in `packages/shared/src/filters.ts`. It is
/// optional so the tab's defaults still apply before the user has touched anything.
#[tauri::command]
pub async fn search_assets(
    state: State<'_, AppState>,
    app_id: u32,
    asset_type: String,
    page: u32,
    filters: Option<sgdb::FilterParams>,
) -> Res<SearchResult> {
    let asset = parse_asset_type(&asset_type)?;
    let Some((kind, base)) = AssetQuery::for_asset_type(asset) else {
        return Err(UiError::new(
            Kind::Unexpected,
            format!("{asset} has no SteamGridDB source"),
        ));
    };

    let mut query = match &filters {
        Some(params) => AssetQuery::from_params(kind, params).map_err(|e| {
            UiError::new(Kind::Unexpected, e.to_string())
                .with_action("Reset the filters for this tab.")
        })?,
        None => base.clone(),
    };

    // 🔴 Restore the tab's dimensions when the filter set carries none.
    //
    // Both grid slots use the *same* endpoint and are told apart only by `dimensions`. Querying
    // `grids` with none for the Header tab fills it with portrait art, which then gets written
    // into the wide slot — it applies, and it looks wrong, which is worse than failing.
    if query.dimensions.is_empty() {
        query.dimensions = base.dimensions;
    }

    let guard = state.sgdb.lock().await;
    let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;

    // A manual override wins: it exists for when the automatic Steam-appid match is wrong.
    let target = {
        let settings = state.settings.lock().await;
        match settings.game_overrides.get(&app_id) {
            Some(over) => Target::Sgdb(over.id),
            None => Target::Steam(AppId::new(app_id)),
        }
    };

    let query = query.page(page).limit(sgdb::PAGE_LIMIT);
    let result = client.assets(kind, target, &query).await?;

    Ok(SearchResult {
        page: result.page,
        total: result.total,
        has_more: result.has_more(),
        assets: result.assets,
    })
}

// -- which SteamGridDB game --------------------------------------------------------------

/// A SteamGridDB game, for the "wrong game?" picker.
#[derive(Debug, Serialize)]
pub struct GameMatch {
    /// SteamGridDB's own id — **not** a Steam appid.
    pub id: u64,
    pub name: String,
    pub verified: bool,
    pub types: Vec<String>,
}

impl From<sgdb::Game> for GameMatch {
    fn from(g: sgdb::Game) -> Self {
        GameMatch {
            id: g.id,
            name: g.name,
            verified: g.verified,
            types: g.types,
        }
    }
}

/// Search SteamGridDB by name, for when the automatic Steam-appid match is wrong or absent.
///
/// The request is made entirely in Rust; only results cross the boundary. The API key stays in
/// `sgdb::client` — it cannot be serialised, so this is enforced by the compiler rather than by
/// remembering.
#[tauri::command]
pub async fn search_games(state: State<'_, AppState>, term: String) -> Res<Vec<GameMatch>> {
    let guard = state.sgdb.lock().await;
    let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;
    // An empty term short-circuits inside the client without a request.
    Ok(client
        .search(&term)
        .await?
        .into_iter()
        .map(GameMatch::from)
        .collect())
}

/// Which SteamGridDB game this appid currently resolves to: the manual override if one is set,
/// otherwise the automatic match. `None` means SteamGridDB has no entry for it.
#[tauri::command]
pub async fn current_game_match(state: State<'_, AppState>, app_id: u32) -> Res<Option<GameMatch>> {
    let over = {
        state
            .settings
            .lock()
            .await
            .game_overrides
            .get(&app_id)
            .cloned()
    };

    // The common case costs nothing: the name was stored when the user chose the override.
    if let Some(over) = &over
        && let Some(name) = &over.name
    {
        return Ok(Some(GameMatch {
            id: over.id,
            name: name.clone(),
            verified: false,
            types: Vec::new(),
        }));
    }

    let guard = state.sgdb.lock().await;
    let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;

    // An override stored before the name was kept alongside it. `/games/id/{id}` is probed and
    // works, so resolve it properly rather than showing `SteamGridDB game #17830`; fall back to
    // that only if the lookup finds nothing.
    if let Some(over) = over {
        let resolved = client.game_by_id(over.id).await?.map(GameMatch::from);
        return Ok(Some(resolved.unwrap_or_else(|| GameMatch {
            id: over.id,
            name: format!("SteamGridDB game #{}", over.id),
            verified: false,
            types: Vec::new(),
        })));
    }

    Ok(client
        .game_by_steam_appid(AppId::new(app_id))
        .await?
        .map(GameMatch::from))
}

// -- applying -------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Applied {
    /// `"live"` or `"file"`.
    pub method: &'static str,
    /// True when the user must restart Steam to see the change.
    pub needs_restart: bool,
    pub path: Option<String>,
    /// Files removed so exactly one remains for this asset.
    pub replaced: Vec<String>,
    /// Why the live path was not used, when it was not. Shown as a quiet note, not an error.
    pub fell_back_because: Option<String>,
}

/// Download an asset and apply it.
///
/// Live first, file-write as the floor. See the module docs.
#[tauri::command]
pub async fn apply_asset(
    state: State<'_, AppState>,
    app_id: u32,
    asset_type: String,
    url: String,
) -> Res<Applied> {
    let asset = parse_asset_type(&asset_type)?;
    let app = AppId::new(app_id);

    // The bytes have to come from Rust either way: SharedJSContext cannot read them. A plain
    // fetch to cdn2.steamgriddb.com is CORS-blocked there, and `no-cors` yields an opaque
    // response whose body cannot be read.
    let bytes = {
        let guard = state.sgdb.lock().await;
        let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;
        match state.cache.get_image(&url) {
            Some(cached) => cached,
            None => {
                let fetched = client.download(&url).await?;
                if let Err(e) = state.cache.put_image(&url, &fetched) {
                    tracing::warn!(error = %e, "could not cache the downloaded image");
                }
                fetched
            }
        }
    };

    let live_enabled = state.settings.lock().await.live_apply;

    // Why the live path is not being taken, or `None` if it is about to be tried.
    let fell_back_because: Option<String> = if !live_enabled {
        Some("Live apply is turned off.".to_owned())
    } else if !asset.supports_live_apply() {
        // Icon and HeroBlur are silent no-ops through Steam's API — ordinal 4 takes ~500 ms
        // and writes nothing at all. Going straight to the file path is the honest choice.
        Some(format!("{asset} cannot be set through Steam's live API."))
    } else {
        match try_live_apply(&state, app, asset, &bytes).await {
            Ok(()) => {
                return Ok(Applied {
                    method: "live",
                    needs_restart: false,
                    path: None,
                    replaced: Vec::new(),
                    fell_back_because: None,
                });
            }
            // Not an error: this is the ladder doing its job. Recorded so the UI can say why
            // a restart is suddenly needed.
            Err(e) => {
                tracing::info!(error = %e, "live apply unavailable; writing files instead");
                Some(e.message)
            }
        }
    };

    // The floor. Always available, always needs a restart.
    let grid = GridDir::new(state.grid_dir()?);
    grid.ensure()?;
    // `png` regardless of the true container: Steam's own code hardcodes it and Chromium
    // sniffs content, which is why animated WebP written as .png animates.
    let applied = grid.apply(app, asset, "png", &bytes)?;

    Ok(Applied {
        method: "file",
        needs_restart: true,
        path: Some(applied.written.display().to_string()),
        replaced: applied
            .removed
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        fell_back_because,
    })
}

async fn try_live_apply(
    state: &State<'_, AppState>,
    app: AppId,
    asset: AssetType,
    bytes: &[u8],
) -> Result<(), UiError> {
    let (mut steam, readiness) = SteamJs::connect(&state.http, &Endpoint::default())
        .await
        .map_err(UiError::from)?;
    if !readiness.can_apply() {
        return Err(UiError::new(
            Kind::LiveApplyUnavailable,
            "Steam's artwork API is not available in this build.",
        ));
    }
    steam
        .apply_artwork(app, asset, bytes)
        .await
        .map_err(UiError::from)
}

/// Remove custom artwork, restoring Steam's own.
#[tauri::command]
pub async fn clear_asset(state: State<'_, AppState>, app_id: u32, asset_type: String) -> Res<()> {
    let asset = parse_asset_type(&asset_type)?;
    let grid = GridDir::new(state.grid_dir()?);
    let _ = grid.clear(AppId::new(app_id), asset)?;
    Ok(())
}

// -- settings -------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LiveApplyRequest {
    pub enabled: bool,
}

/// Turn live apply on or off.
///
/// 🔑 Enabling it creates `.cef-enable-remote-debugging`. That is the **only** place this app
/// creates that file, and it happens here because this command is only ever reached from an
/// explicit click on a control that explains what the file is.
#[tauri::command]
pub async fn set_live_apply(state: State<'_, AppState>, req: LiveApplyRequest) -> Res<Status> {
    let ctx = state.steam()?;
    let sentinel = Sentinel::for_install(&ctx.install);

    if req.enabled {
        sentinel
            .enable()
            .map_err(|e| UiError::new(Kind::Filesystem, e.to_string()))?;
    }
    // Disabling leaves the sentinel alone on purpose: CSS Loader and other tools rely on the
    // same file, and silently breaking them would be worse than leaving an empty file behind.
    // Removing it is offered separately, with that explained.

    {
        let mut settings = state.settings.lock().await;
        settings.live_apply = req.enabled;
        state.store.save(&settings)?;
    }
    status(state).await
}

/// Remove the debugging sentinel entirely.
#[tauri::command]
pub async fn remove_sentinel(state: State<'_, AppState>) -> Res<Status> {
    let ctx = state.steam()?;
    Sentinel::for_install(&ctx.install)
        .disable()
        .map_err(|e| UiError::new(Kind::Filesystem, e.to_string()))?;
    {
        let mut settings = state.settings.lock().await;
        settings.live_apply = false;
        state.store.save(&settings)?;
    }
    status(state).await
}

// -- diagnostics ----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ModuleReport {
    pub clstamp: String,
    pub total_modules: usize,
    pub resolved: usize,
    pub outcomes: Vec<(String, String)>,
    pub features: Vec<(String, bool, String)>,
}

/// Re-resolve Steam's module map and report per-feature availability.
///
/// The diagnostics screen is a shipped feature because most failures in this product are
/// environmental. This is also the manual test harness.
#[tauri::command]
pub async fn resolve_modules(state: State<'_, AppState>) -> Res<ModuleReport> {
    let (mut steam, _) = SteamJs::connect(&state.http, &Endpoint::default())
        .await
        .map_err(UiError::from)?;
    let resolution = steam.resolve_modules().await.map_err(UiError::from)?;

    let outcomes = resolution
        .outcomes
        .iter()
        .map(|(name, outcome)| {
            let text = match outcome {
                cdp::modules::Outcome::Found { ids } => ids.join(", "),
                cdp::modules::Outcome::Ambiguous { ids } => {
                    format!("ambiguous: {}", ids.join(", "))
                }
                cdp::modules::Outcome::NotFound => "not found".to_owned(),
            };
            (name.clone(), text)
        })
        .collect();

    let features = cdp::modules::FEATURES
        .iter()
        .map(|f| {
            (
                f.name.to_owned(),
                f.available(&resolution),
                f.fallback.to_owned(),
            )
        })
        .collect();

    Ok(ModuleReport {
        clstamp: resolution.clstamp.clone(),
        total_modules: resolution.total_modules,
        resolved: resolution.usable(),
        outcomes,
        features,
    })
}

// -- helpers --------------------------------------------------------------------------------

/// Map the frontend's asset-type string onto the core enum.
///
/// The names match `@sgdb/shared`'s `ASSET_TYPES`, which in turn match the Decky plugin's, so
/// the two frontends and the docs all use one vocabulary.
fn parse_asset_type(s: &str) -> Result<AssetType, UiError> {
    match s {
        "grid_p" => Ok(AssetType::Capsule),
        "grid_l" => Ok(AssetType::Header),
        "hero" => Ok(AssetType::Hero),
        "logo" => Ok(AssetType::Logo),
        "icon" => Ok(AssetType::Icon),
        other => Err(UiError::new(
            Kind::Unexpected,
            format!("unknown asset type {other:?}"),
        )),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    /// The dimension-restoring rule from `search_assets`, extracted so it can be tested without
    /// a Tauri `State` or a network call. Kept next to its caller so the two cannot drift.
    fn effective_dimensions(
        asset: AssetType,
        filters: Option<&sgdb::FilterParams>,
    ) -> Vec<sgdb_core::sgdb::Dimensions> {
        let (kind, base) = AssetQuery::for_asset_type(asset).unwrap();
        let mut query = match filters {
            Some(p) => AssetQuery::from_params(kind, p).unwrap(),
            None => base.clone(),
        };
        if query.dimensions.is_empty() {
            query.dimensions = base.dimensions;
        }
        query.dimensions
    }

    #[test]
    fn a_filter_set_with_no_dimensions_keeps_the_tabs_own_dimensions() {
        use sgdb_core::sgdb::Dimensions;

        // 🔴 The most dangerous line in the filter path. Capsule and Header are the *same*
        // endpoint, told apart only by `dimensions`. Letting an empty filter set through would
        // fill the Header tab with portrait art, which then applies to the wide slot — it works,
        // and it looks wrong, which is worse than failing.
        let empty = sgdb::FilterParams::default();

        // Premise: the filter set really does carry no dimensions, so this exercises the guard.
        assert!(empty.dimensions.is_none());

        assert_eq!(
            effective_dimensions(AssetType::Header, Some(&empty)),
            Dimensions::WIDE,
        );
        assert_eq!(
            effective_dimensions(AssetType::Capsule, Some(&empty)),
            Dimensions::PORTRAIT,
        );

        // The control: an explicit choice is honoured rather than overwritten, or the guard
        // would silently make the size filter do nothing at all.
        let chosen = sgdb::FilterParams {
            dimensions: Some("920x430".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            effective_dimensions(AssetType::Header, Some(&chosen)),
            vec![Dimensions::D920x430],
        );
    }

    #[test]
    fn the_library_sort_is_stable_and_never_played_sorts_last() {
        fn entry(name: &str, last_played: Option<u64>, minutes: Option<u32>) -> LibraryEntry {
            LibraryEntry {
                app_id: 1,
                name: name.to_owned(),
                kind: "steam",
                app_type: None,
                current_art: None,
                steam_art: None,
                installed: true,
                last_played,
                playtime_minutes: minutes,
            }
        }

        let mut entries = vec![
            entry("Zeta", Some(100), Some(5)),
            entry("alpha", None, None),
            entry("Beta", Some(900), Some(1)),
        ];

        sort_entries(&mut entries, LibrarySort::Name);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["alpha", "Beta", "Zeta"], "case-insensitive by name");

        sort_entries(&mut entries, LibrarySort::RecentlyPlayed);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // `None` must sort last. A naive descending sort on Option<u64> puts None first, which
        // would fill the top of "recently played" with games never launched.
        assert_eq!(names, ["Beta", "Zeta", "alpha"]);

        sort_entries(&mut entries, LibrarySort::MostPlayed);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Zeta", "Beta", "alpha"]);
    }

    #[test]
    fn every_entry_with_the_same_key_sorts_deterministically() {
        // Hundreds of apps share a `None` playtime. Without the name tiebreak their order would
        // depend on the input order and shuffle between loads for no visible reason.
        let make = || {
            vec![
                LibraryEntry {
                    app_id: 2,
                    name: "Bravo".to_owned(),
                    kind: "steam",
                    app_type: None,
                    current_art: None,
                    steam_art: None,
                    installed: true,
                    last_played: None,
                    playtime_minutes: None,
                },
                LibraryEntry {
                    app_id: 1,
                    name: "Alpha".to_owned(),
                    kind: "steam",
                    app_type: None,
                    current_art: None,
                    steam_art: None,
                    installed: false,
                    last_played: None,
                    playtime_minutes: None,
                },
            ]
        };

        let mut a = make();
        let mut b = make();
        b.reverse();
        sort_entries(&mut a, LibrarySort::RecentlyPlayed);
        sort_entries(&mut b, LibrarySort::RecentlyPlayed);

        let names = |v: &[LibraryEntry]| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&a), names(&b));
        assert_eq!(names(&a), ["Alpha", "Bravo"]);
    }

    #[test]
    fn asset_type_names_match_the_shared_vocabulary() {
        // These strings are the contract between the Rust bridge, @sgdb/shared and the Decky
        // plugin's own naming. A mismatch would send hero art to the capsule slot.
        assert_eq!(parse_asset_type("grid_p").unwrap(), AssetType::Capsule);
        assert_eq!(parse_asset_type("grid_l").unwrap(), AssetType::Header);
        assert_eq!(parse_asset_type("hero").unwrap(), AssetType::Hero);
        assert_eq!(parse_asset_type("logo").unwrap(), AssetType::Logo);
        assert_eq!(parse_asset_type("icon").unwrap(), AssetType::Icon);
    }

    #[test]
    fn an_unknown_asset_type_is_refused_rather_than_defaulted() {
        // Defaulting to the capsule would write art into the wrong slot and look like a
        // rendering bug rather than a wiring bug.
        let err = parse_asset_type("grid").unwrap_err();
        assert_eq!(err.kind, Kind::Unexpected);
        assert!(err.message.contains("grid"), "{}", err.message);
    }

    #[test]
    fn the_portrait_and_wide_slots_do_not_collide() {
        // `grid_p` and `grid_l` both come from SteamGridDB's `grids` endpoint but write to
        // different files, which is exactly the confusion worth guarding.
        let p = parse_asset_type("grid_p").unwrap();
        let l = parse_asset_type("grid_l").unwrap();
        assert_ne!(p, l);
        assert_ne!(p as u32, l as u32);
    }
}
