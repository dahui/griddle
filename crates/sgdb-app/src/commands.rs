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
use sgdb_core::sgdb::{self, ApiKey, AssetQuery, Target};
use sgdb_core::steam::{apptype, library, process, shortcuts::Shortcuts};
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
}

/// Installed games plus non-Steam shortcuts, with whatever custom art already exists.
#[tauri::command]
pub async fn library(state: State<'_, AppState>, asset_type: String) -> Res<Vec<LibraryEntry>> {
    let asset = parse_asset_type(&asset_type)?;
    let ctx = state.steam()?;
    let grid = GridDir::new(ctx.install.grid_dir(ctx.account.id));

    let mut entries = Vec::new();

    // Installed Steam apps. One corrupt manifest never empties the list.
    match library::installed_apps(&ctx.install) {
        Ok(apps) => {
            for app in apps.iter().filter(|a| a.is_fully_installed()) {
                if !apptype::include_in_library(ctx.app_types.as_ref(), app.app_id) {
                    continue;
                }
                entries.push(LibraryEntry {
                    app_id: app.app_id.get(),
                    name: app.name.clone(),
                    kind: "steam",
                    app_type: ctx
                        .app_types
                        .as_ref()
                        .and_then(|t| t.app_type(app.app_id))
                        .map(|t| t.label().to_owned()),
                    current_art: first_existing(&grid, app.app_id, asset),
                });
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not enumerate installed apps"),
    }

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
                });
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not read shortcuts.vdf"),
    }

    // Case-insensitive, so "Portal" and "portal" sit together rather than in separate blocks.
    entries.sort_by_key(|e| e.name.to_lowercase());
    Ok(entries)
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

/// One page of artwork for a game, filtered to the asset type's dimensions.
#[tauri::command]
pub async fn search_assets(
    state: State<'_, AppState>,
    app_id: u32,
    asset_type: String,
    page: u32,
) -> Res<SearchResult> {
    let asset = parse_asset_type(&asset_type)?;
    let Some((kind, base)) = AssetQuery::for_asset_type(asset) else {
        return Err(UiError::new(
            Kind::Unexpected,
            format!("{asset} has no SteamGridDB source"),
        ));
    };

    let guard = state.sgdb.lock().await;
    let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;

    // A manual override wins: it exists for when the automatic Steam-appid match is wrong.
    let target = {
        let settings = state.settings.lock().await;
        match settings.game_overrides.get(&app_id) {
            Some(sgdb_id) => Target::Sgdb(*sgdb_id),
            None => Target::Steam(AppId::new(app_id)),
        }
    };

    let query = base.page(page).limit(50);
    let result = client.assets(kind, target, &query).await?;

    Ok(SearchResult {
        page: result.page,
        total: result.total,
        has_more: result.has_more(),
        assets: result.assets,
    })
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
