//! Removing every piece of custom artwork at once.
//!
//! Split from the per-slot clear in [`super::apply`] because it is a different operation with
//! different costs: one CDP connection for the whole sweep rather than one per slot, and a
//! separate read-only command that counts what would go, so the confirmation is not quoting
//! figures nobody checked.

use super::Res;
use crate::error::UiError;
use crate::state::AppState;
use griddle_core::cdp::{Endpoint, SteamJs};
use griddle_core::grid::names::AssetType;
use griddle_core::grid::store::GridDir;
use serde::Serialize;
use tauri::State;

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
/// 🔴 **One CDP connection for the whole sweep.** [`super::clear_asset`] opens one per slot, which
/// is right for a single reset and would be hundreds of handshakes here. The connection is made
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
