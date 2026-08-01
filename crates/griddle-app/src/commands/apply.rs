//! Applying and clearing artwork for one slot, and reporting what each slot currently holds.
//!
//! Both mutating commands walk the same ladder — live first, file-write as the floor. See the
//! module docs on [`super`] for why that order is the product thesis.

use super::{Res, parse_asset_type, path_string};
use crate::error::{Kind, UiError};
use crate::state::AppState;
use griddle_core::appid::AppId;
use griddle_core::cdp::{Endpoint, SteamJs};
use griddle_core::grid::names::AssetType;
use griddle_core::grid::store::GridDir;
use griddle_core::steam::LibraryCache;
use serde::Serialize;
use tauri::State;

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
