#![allow(
    clippy::let_underscore_must_use,
    reason = "the #[tauri::command] macro expands to `let _ = ...` at each command's signature; \
              the workspace denies that pattern in our own code, which is where it matters"
)]
//! The `invoke` surface. Thin: every decision belongs to `griddle-core`.
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
use griddle_core::appid::AppId;
use griddle_core::cdp::{self, Endpoint, Sentinel, SteamJs};
use griddle_core::grid::names::AssetType;
use griddle_core::grid::store::GridDir;
use griddle_core::settings::{LibraryScope, LibrarySort};
use griddle_core::sgdb::{self, ApiKey, AssetQuery, Target};
use griddle_core::steam::{
    LibraryCache, apptype, library, localconfig, process, shortcuts::Shortcuts,
};
use serde::Serialize;
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
    /// Whether the CEF debugging flag is in place. Set up at startup, not by the user — this is
    /// reported for diagnostics, not offered as a control.
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
                ctx.app_types
                    .as_ref()
                    .map(griddle_core::steam::AppTypes::len),
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

// -- links ----------------------------------------------------------------------------------

/// Open a link in the user's default browser.
///
/// A Tauri webview ignores `target="_blank"`, so an ordinary `<a>` does nothing at all — which
/// is what made the API-key link look broken. The URL is checked against an allowlist in
/// [`griddle_core::browser`] before it reaches the shell; this command deliberately cannot open an
/// arbitrary address.
#[tauri::command]
pub async fn open_url(url: String) -> Res<()> {
    griddle_core::browser::open(&url).map_err(|e| {
        let ui = UiError::new(Kind::Unexpected, e.to_string());
        match e.suggestion() {
            Some(s) => ui.with_action(s),
            None => ui,
        }
    })
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
    pub filters: Option<griddle_core::settings::FilterState>,
    pub zoom: BTreeMap<String, f32>,
    pub game_overrides: BTreeMap<u32, griddle_core::settings::GameOverride>,
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
    filters: griddle_core::settings::FilterState,
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
/// [`griddle_core::settings::GameOverride`].
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
                    .insert(app_id, griddle_core::settings::GameOverride { id, name });
            }
            None => {
                let _ = settings.game_overrides.remove(&app_id);
            }
        }
        state.store.save(&settings)?;
    }
    // The session cache holds whatever this appid resolved to before. Clearing an override has
    // to re-resolve, or "use the automatic match" would keep returning the overridden game.
    let _ = state.game_matches.lock().await.remove(&app_id);
    Ok(snapshot(&state).await)
}

// -- library --------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct LibraryEntry {
    pub app_id: u32,
    pub name: String,
    /// Whether [`name`](Self::name) is a real name or a stand-in built from the appid.
    ///
    /// Now a **degraded-mode signal only**: an app with no `appinfo.vdf` name is one the account
    /// no longer holds and is dropped from the list entirely, so the only rows that reach the UI
    /// unnamed are those from an unreadable `appinfo.vdf` (where nothing is dropped, by design)
    /// and a shortcut with no `appname`.
    ///
    /// Kept rather than removed because that is exactly when the UI most needs to explain
    /// itself: a synthesised name with nothing to say for it reads as artwork that failed to
    /// load, which is how it was reported.
    pub named: bool,
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

/// Whether an app that only `localconfig.vdf` remembers should be left out of the library.
///
/// 🔴 **An app `localconfig.vdf` knows and `appinfo.vdf` does not is one the account no longer
/// holds** — a refunded purchase, or a demo or beta Steam has withdrawn. Confirmed against a real
/// library: all 29 such apps there were exactly that. `[VERIFIED-BOX 2026-07-31]` `localconfig` is
/// a record of what was *configured*, never of what is *owned* — there is no offline ownership
/// list, `licensecache` being encrypted — and absence from appinfo is the closest signal there is.
///
/// 🔴 `appinfo_loaded` is the whole safety story, which is why it is a parameter rather than
/// something this function works out. When `appinfo.vdf` cannot be read, **every** app looks
/// nameless, and dropping on that would cut the All-games scope down to installed apps and
/// shortcuts — surfacing as "some of my games are missing", which this codebase treats as the
/// hardest kind of bug to report. No appinfo means no opinion.
///
/// Installed apps never reach this: they are named from their `appmanifest` and are already in
/// the map by the time `localconfig` is walked.
fn is_disowned(appinfo_loaded: bool, appinfo_name: Option<&str>) -> bool {
    appinfo_loaded && appinfo_name.is_none()
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
                        named: true,
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
    let mut dropped = 0usize;
    for record in &known {
        let id = record.app_id;
        let appinfo_name = ctx.app_types.as_ref().and_then(|t| t.name(id));

        // Refunded games and withdrawn demos — see `is_disowned`. This reverses the project's
        // usual "unknown means show it" direction, deliberately and on the library owner's
        // say-so: these are not games missing from the list, they are games no longer in the
        // account, and every one of them is unnamed and artless.
        let disowned = is_disowned(ctx.app_types.is_some(), appinfo_name);
        if scope == LibraryScope::All && disowned && !steam_apps.contains_key(&id.get()) {
            dropped += 1;
            continue;
        }

        if scope == LibraryScope::All
            && !steam_apps.contains_key(&id.get())
            && apptype::include_in_library(ctx.app_types.as_ref(), id)
        {
            let _ = steam_apps.insert(
                id.get(),
                LibraryEntry {
                    app_id: id.get(),
                    // Only reachable when `appinfo.vdf` could not be read at all, since a
                    // nameless app is otherwise dropped above. `Steam app <id>` reads as a label
                    // rather than as art that failed to load, which is how `Unknown app <id>`
                    // was reported. The id stays *inside* the name on purpose: the filter box
                    // matches on `name`, so the row is still findable by typing the number.
                    name: appinfo_name
                        .map_or_else(|| format!("Steam app {}", id.get()), str::to_owned),
                    named: appinfo_name.is_some(),
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

    // The tripwire for the paragraph above. If this ever starts hiding real games, the count is
    // the first place to look — and a count that suddenly jumps is the signal that `appinfo.vdf`
    // changed shape rather than that the account did.
    if dropped > 0 {
        tracing::info!(
            dropped,
            of = known.len(),
            "skipped apps absent from appinfo.vdf (refunded, or withdrawn demos and betas)"
        );
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
                    // A shortcut with no `appname` is the same situation, from a different file.
                    named: s.app_name().is_some(),
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
    pub assets: Vec<griddle_core::sgdb::Asset>,
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

    // Override, then appid, then a search by name — see `resolve_game`. Resolved before the
    // client is locked, because resolving needs that same lock.
    //
    // Falling back to `Target::Steam` when nothing resolves is deliberate: the API's own 404 is
    // a clearer, more specific error than anything invented here, and it carries the "search by
    // name instead" action.
    let target = match resolve_game(&state, app_id).await? {
        Some(game) => Target::Sgdb(game.id),
        None => Target::Steam(AppId::new(app_id)),
    };

    let guard = state.sgdb.lock().await;
    let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;

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

/// How an appid was matched to a SteamGridDB game.
///
/// Internal: it distinguishes a manual override from an automatic match, which is what lets
/// [`current_game_match`] re-resolve the name of an override stored before names were kept.
/// It is deliberately **not** sent to the UI — the label is just the game's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedBy {
    /// The user picked this game themselves.
    Override,
    /// SteamGridDB knows this Steam appid. Exact.
    AppId,
    /// SteamGridDB does not know the appid, so the game's name was searched instead.
    Name,
}

/// A SteamGridDB game, for the "wrong game?" picker.
#[derive(Debug, Clone, Serialize)]
pub struct GameMatch {
    /// SteamGridDB's own id — **not** a Steam appid.
    pub id: u64,
    pub name: String,
    pub verified: bool,
    pub types: Vec<String>,
    /// Not serialised: the UI shows the game's name, not how it was found.
    #[serde(skip)]
    pub matched_by: MatchedBy,
}

impl GameMatch {
    fn from_game(g: sgdb::Game, matched_by: MatchedBy) -> Self {
        GameMatch {
            id: g.id,
            name: g.name,
            verified: g.verified,
            types: g.types,
            matched_by,
        }
    }
}

/// The name to search SteamGridDB with when the appid is not known there.
///
/// `appinfo.vdf` covers Steam apps; `shortcuts.vdf` covers non-Steam ones, whose appid is a
/// random high-bit number SteamGridDB has never seen and never will — so for those the name is
/// the *only* way to match anything at all.
fn searchable_name(ctx: &crate::state::SteamContext, app_id: u32) -> Option<String> {
    let app = AppId::new(app_id);
    if let Some(name) = ctx.app_types.as_ref().and_then(|t| t.name(app)) {
        return Some(name.to_owned());
    }
    let shortcuts = Shortcuts::load_or_empty(ctx.install.shortcuts_vdf(ctx.account.id)).ok()?;
    shortcuts
        .iter()
        .find(|s| s.app_id() == Some(app))
        .and_then(|s| s.app_name())
        .map(str::to_owned)
}

/// Which SteamGridDB game an appid pulls artwork from.
///
/// The ladder, and why the last rung exists:
///
/// 1. **A manual override** — the user's choice always wins.
/// 2. **The Steam appid** — exact, and right for most games.
/// 3. **A name search** — because plenty of appids are not on SteamGridDB at all. Measured:
///    `3837340` (FINAL FANTASY VII) 404s by appid but its name finds the game immediately, and
///    every non-Steam shortcut 404s by construction since its appid is a random number Steam
///    generated locally. `[VERIFIED-BOX 2026-07-30]` Without this rung those games showed an
///    error instead of artwork.
///
/// Cached per session in [`AppState::game_matches`], including a `None`, so a game with no match
/// does not re-search on every page of a scroll.
async fn resolve_game(state: &State<'_, AppState>, app_id: u32) -> Res<Option<GameMatch>> {
    // The user's own choice, and the only mapping that is persisted.
    if let Some(over) = state.settings.lock().await.game_overrides.get(&app_id) {
        return Ok(Some(GameMatch {
            id: over.id,
            name: over
                .name
                .clone()
                .unwrap_or_else(|| format!("SteamGridDB game #{}", over.id)),
            verified: false,
            types: Vec::new(),
            matched_by: MatchedBy::Override,
        }));
    }

    if let Some(cached) = state.game_matches.lock().await.get(&app_id) {
        return Ok(cached.clone());
    }

    let guard = state.sgdb.lock().await;
    let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;

    let mut resolved = client
        .game_by_steam_appid(AppId::new(app_id))
        .await?
        .map(|g| GameMatch::from_game(g, MatchedBy::AppId));

    if resolved.is_none()
        && let Ok(ctx) = state.steam()
        && let Some(name) = searchable_name(ctx, app_id)
    {
        // First hit only. SteamGridDB's autocomplete is already ranked, and offering the user a
        // list here would be the "Wrong game?" picker — which they can still open to change it.
        resolved = client
            .search(&name)
            .await?
            .into_iter()
            .next()
            .map(|g| GameMatch::from_game(g, MatchedBy::Name));
        match &resolved {
            Some(g) => tracing::info!(app_id, name, matched = g.name, "matched by name"),
            None => tracing::info!(app_id, name, "no SteamGridDB match by appid or name"),
        }
    }

    let _ = state
        .game_matches
        .lock()
        .await
        .insert(app_id, resolved.clone());
    Ok(resolved)
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
        // These are candidates the user is choosing between, so they are all "by name" until
        // one is picked and becomes an override.
        .map(|g| GameMatch::from_game(g, MatchedBy::Name))
        .collect())
}

/// Which SteamGridDB game this appid currently resolves to: the manual override if one is set,
/// otherwise the automatic match. `None` means SteamGridDB has no entry for it.
#[tauri::command]
pub async fn current_game_match(state: State<'_, AppState>, app_id: u32) -> Res<Option<GameMatch>> {
    let resolved = resolve_game(&state, app_id).await?;

    // An override stored before the name was kept alongside it shows as `SteamGridDB game
    // #17830`. `/games/id/{id}` is probed and works, so resolve it properly.
    if let Some(game) = &resolved
        && game.matched_by == MatchedBy::Override
        && game.name.starts_with("SteamGridDB game #")
    {
        let guard = state.sgdb.lock().await;
        let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;
        if let Some(found) = client.game_by_id(game.id).await? {
            return Ok(Some(GameMatch::from_game(found, MatchedBy::Override)));
        }
    }

    Ok(resolved)
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

    // Why the live path is not being taken, or `None` if it is about to be tried.
    //
    // Live is always attempted — there is no longer a setting for it. The user installed this
    // to avoid restarting Steam, so the ladder decides by capability, not by preference.
    let fell_back_because: Option<String> = if !asset.supports_live_apply() {
        // Icon and HeroBlur are silent no-ops through Steam's API — ordinal 4 takes ~500 ms
        // and writes nothing at all. Going straight to the file path is the honest choice.
        Some(format!("Steam can't set {asset} artwork live."))
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
            "Steam's artwork API isn't available in this build.",
        ));
    }
    steam
        .apply_artwork(app, asset, bytes)
        .await
        .map_err(UiError::from)
}

/// What one artwork slot currently holds.
#[derive(Debug, Serialize)]
pub struct AssetSlot {
    /// SteamGridDB's name for the slot — `grid_p`, `grid_l`, `hero`, `logo`, `icon`.
    pub asset_type: &'static str,
    pub label: &'static str,
    /// The user's own artwork, if they have set any.
    pub custom_art: Option<String>,
    /// Steam's own artwork, which is what a reset falls back to.
    pub steam_art: Option<String>,
    /// The files a reset would delete, as bare filenames.
    ///
    /// Returned so the UI can **name the deletion before it happens**, which this project
    /// requires of anything that removes a file from the user's Steam directory.
    pub removes: Vec<String>,
}

/// What every artwork slot for one game currently holds.
///
/// The overview the five browsing tabs cannot give: which slots the user has customised, which
/// are still Steam's own, and which have nothing at all.
#[tauri::command]
pub async fn asset_status(state: State<'_, AppState>, app_id: u32) -> Res<Vec<AssetSlot>> {
    let ctx = state.steam()?;
    let app = AppId::new(app_id);
    let grid = GridDir::new(ctx.install.grid_dir(ctx.account.id));
    let steam_art = LibraryCache::new(&ctx.install, ctx.app_types.as_ref());

    Ok(AssetType::EDITABLE
        .into_iter()
        .map(|asset| {
            let existing = grid.existing(app, asset);
            AssetSlot {
                asset_type: asset.sgdb_name(),
                label: asset.label(),
                custom_art: existing.first().map(|p| p.display().to_string()),
                steam_art: path_string(steam_art.resolve(app, asset)),
                removes: existing
                    .iter()
                    .filter_map(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .collect(),
            }
        })
        .collect())
}

/// What a reset actually did.
#[derive(Debug, Serialize)]
pub struct Cleared {
    /// `"live"` or `"file"`.
    pub method: &'static str,
    pub needs_restart: bool,
    /// Files removed from `grid/`, as bare filenames.
    pub removed: Vec<String>,
    pub fell_back_because: Option<String>,
}

/// Remove custom artwork, restoring Steam's own.
///
/// The same live→file ladder as [`apply_asset`], for the same reason: a reset that only takes
/// effect after a Steam restart is a worse experience than the apply it undoes.
///
/// **The files are always removed, whether or not the live call ran.** Steam's own
/// `ClearCustomArtworkForApp` may well delete them itself, but leaving that unverified and
/// trusting it would risk the client forgetting the art while the file stayed on disk to be
/// picked up again later. Sweeping afterwards costs one `read_dir` and cannot be wrong.
#[tauri::command]
pub async fn clear_asset(
    state: State<'_, AppState>,
    app_id: u32,
    asset_type: String,
) -> Res<Cleared> {
    let asset = parse_asset_type(&asset_type)?;
    let app = AppId::new(app_id);
    let grid = GridDir::new(state.grid_dir()?);

    // Captured before anything is removed, so the report names what was actually there even if
    // Steam's own call deletes the files first.
    let had: Vec<String> = grid
        .existing(app, asset)
        .iter()
        .filter_map(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();

    let fell_back_because: Option<String> = if !asset.supports_live_apply() {
        Some(format!("Steam can't clear {asset} artwork live."))
    } else {
        match try_live_clear(&state, app, asset).await {
            Ok(()) => None,
            Err(e) => {
                tracing::info!(error = %e, "live clear unavailable; removing files instead");
                Some(e.message)
            }
        }
    };

    let removed = grid.clear(app, asset)?;
    let live = fell_back_because.is_none();
    Ok(Cleared {
        method: if live { "live" } else { "file" },
        // A live clear has already updated the running client; the file sweep after it is
        // bookkeeping, so it does not make a restart necessary.
        needs_restart: !live && !removed.is_empty(),
        removed: had,
        fell_back_because,
    })
}

async fn try_live_clear(
    state: &State<'_, AppState>,
    app: AppId,
    asset: AssetType,
) -> Result<(), UiError> {
    let (mut steam, readiness) = SteamJs::connect(&state.http, &Endpoint::default())
        .await
        .map_err(UiError::from)?;
    if !readiness.can_apply() {
        return Err(UiError::new(
            Kind::LiveApplyUnavailable,
            "Steam's artwork API isn't available in this build.",
        ));
    }
    steam.clear_artwork(app, asset).await.map_err(UiError::from)
}

// -- reset everything -----------------------------------------------------------------------

/// What a full reset would remove, for the confirmation dialog.
#[derive(Debug, Serialize)]
pub struct ResetPlan {
    pub games: usize,
    pub files: usize,
}

/// Count what a full reset would delete, **without deleting anything**.
///
/// This project does not remove a file from the user's Steam directory without naming it first,
/// and at this scale naming every file is useless — so the confirmation quotes counts instead,
/// and this is where they come from. Read-only, and deliberately its own command: computing the
/// numbers inside the reset itself would mean the dialog was quoting figures nobody had checked.
#[tauri::command]
pub async fn reset_all_plan(state: State<'_, AppState>) -> Res<ResetPlan> {
    let grid = GridDir::new(state.grid_dir()?);
    let apps = grid.customised_apps()?;

    let mut files = 0usize;
    let mut games = 0usize;
    for app in &apps {
        // 🔴 `removable`, not a sum of `existing` — the latter misses a logo's position sidecar,
        // and a confirmation that under-states a deletion is worse than no confirmation.
        let n = grid.removable(*app).len();
        files += n;
        if n > 0 {
            games += 1;
        }
    }
    Ok(ResetPlan { games, files })
}

/// What a full reset actually did.
#[derive(Debug, Serialize)]
pub struct ResetAll {
    pub games: usize,
    pub files_removed: usize,
    /// `"live"` or `"file"`.
    pub method: &'static str,
    pub needs_restart: bool,
    pub fell_back_because: Option<String>,
    /// Games whose files could not be removed, named so a partial result is never silent.
    pub failed: Vec<String>,
}

/// Remove every piece of custom artwork, restoring Steam's own everywhere.
///
/// 🔴 **One CDP connection for the whole sweep.** [`clear_asset`] opens one per slot, which is
/// right for a single reset and would be hundreds of handshakes here. The connection is made
/// once up front; if it fails, the entire run degrades to the file path and says so once rather
/// than failing per game.
///
/// **Partial failure is reported, not swallowed.** A file that will not delete — locked, or
/// read-only — leaves the rest of the sweep to continue and lands in `failed`. Aborting midway
/// would leave the library in a state the user cannot reason about and did not ask for.
#[tauri::command]
pub async fn reset_all_art(state: State<'_, AppState>) -> Res<ResetAll> {
    let grid = GridDir::new(state.grid_dir()?);
    let apps = grid.customised_apps()?;

    let (mut live, fell_back_because) = match SteamJs::connect(&state.http, &Endpoint::default())
        .await
    {
        Ok((steam, readiness)) if readiness.can_apply() => (Some(steam), None),
        Ok(_) => (
            None,
            Some("Steam's artwork API isn't available in this build.".to_owned()),
        ),
        Err(e) => {
            let ui = UiError::from(e);
            tracing::info!(error = %ui.message, "live clear unavailable; removing files instead");
            (None, Some(ui.message))
        }
    };

    let mut games = 0usize;
    let mut files_removed = 0usize;
    let mut failed = Vec::new();

    for app in &apps {
        let mut touched = false;
        for asset in AssetType::EDITABLE {
            let had = !grid.existing(*app, asset).is_empty();
            // The live call is worth making only where there is something to clear; the file
            // sweep below runs regardless, because it also takes a stranded logo position.
            if had
                && asset.supports_live_apply()
                && let Some(steam) = live.as_mut()
                && let Err(e) = steam.clear_artwork(*app, asset).await
            {
                tracing::warn!(app = %app, %asset, error = %e, "live clear failed for one slot");
            }
            match grid.clear(*app, asset) {
                Ok(removed) => {
                    files_removed += removed.len();
                    touched |= !removed.is_empty();
                }
                Err(e) => failed.push(format!("{app} ({asset}): {e}")),
            }
        }
        if touched {
            games += 1;
        }
    }

    let was_live = fell_back_because.is_none();
    tracing::info!(games, files_removed, live = was_live, "reset all artwork");
    Ok(ResetAll {
        games,
        files_removed,
        method: if was_live { "live" } else { "file" },
        needs_restart: !was_live && files_removed > 0,
        fell_back_because,
        failed,
    })
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
/// The names match `@griddle/shared`'s `ASSET_TYPES`, which in turn match the Decky plugin's, so
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
    ) -> Vec<griddle_core::sgdb::Dimensions> {
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
        use griddle_core::sgdb::Dimensions;

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
    fn an_app_appinfo_has_never_heard_of_is_dropped() {
        // Refunded purchases and withdrawn demos keep their `localconfig` entry forever. They
        // are unnamed and artless, and the account no longer holds them.
        assert!(is_disowned(true, None));

        // The control: an app appinfo *can* name is kept. Without this the test would pass just
        // as well against a function that dropped everything.
        assert!(!is_disowned(true, Some("Portal 2")));
    }

    #[test]
    fn nothing_is_dropped_when_appinfo_could_not_be_read() {
        // 🔴 The guard that keeps the rule above safe. With no readable `appinfo.vdf` every app
        // looks nameless, so dropping on that alone would cut the All-games scope down to
        // installed apps and shortcuts — "some of my games are missing", which is the hardest
        // kind of bug for a user to report and the one this codebase most wants to avoid.
        assert!(!is_disowned(false, None));

        // And it must not depend on the name being absent: with no appinfo there is no opinion
        // to have, whatever else is true.
        assert!(!is_disowned(false, Some("Portal 2")));
    }

    #[test]
    fn the_library_sort_is_stable_and_never_played_sorts_last() {
        fn entry(name: &str, last_played: Option<u64>, minutes: Option<u32>) -> LibraryEntry {
            LibraryEntry {
                app_id: 1,
                name: name.to_owned(),
                named: true,
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
                    named: true,
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
                    named: true,
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
        // These strings are the contract between the Rust bridge, @griddle/shared and the Decky
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
