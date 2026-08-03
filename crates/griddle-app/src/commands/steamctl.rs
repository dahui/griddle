//! Starting Steam, and remembering whether to offer.
//!
//! # Why the app offers at all
//!
//! Griddle works with Steam closed — artwork is written to disk and appears at the next start —
//! so this is never a blocker. But three things are strictly better with Steam up, and none of
//! them announces itself when it is missing:
//!
//! - Artwork applies **instantly** rather than needing a restart.
//! - **All games** is the real library rather than what `localconfig.vdf` remembers, which is a
//!   few hundred games short.
//! - Refunded and withdrawn apps are pruned out, which the offline heuristic largely misses.
//!
//! A user who never starts Steam first gets the degraded version of all three and no reason to
//! suspect it. That is the argument for saying something, and it is the only one.
//!
//! # Why it is an offer and not a requirement
//!
//! Most people autostart Steam and will never see this. Some deliberately do not, and for them a
//! dialog on every launch is exactly the startup furniture this project has removed before — the
//! setup screen that asked permission for the debugging sentinel was deleted for being
//! unnecessary, and this must not quietly reintroduce it.
//!
//! So the offer carries its own off switch, [`set_offer_to_start_steam`], and the setting defaults
//! to on in a way that survives an older settings file. Dismissing once silences it for the
//! session; "don't ask again" silences it for good.

use super::Res;
use crate::error::UiError;
use crate::state::AppState;
use griddle_core::cdp::{Endpoint, SteamJs};
use griddle_core::steam::process;
use tauri::State;

/// Start Steam.
///
/// Reuses [`process::launch`], which the shutdown/relaunch cycle already depends on, so this is
/// not a second way of starting Steam that could drift from the tested one. It spawns detached:
/// Steam outlives Griddle, and a child process that dies with its parent would be a worse bug
/// than not offering at all.
///
/// **It does not wait.** Steam takes tens of seconds to have a usable JS realm, and blocking a
/// command on that would freeze the window for the duration with nothing useful to show. Every
/// live feature already re-checks on use, so the app upgrades itself as Steam comes up.
#[tauri::command]
pub async fn start_steam(state: State<'_, AppState>) -> Res<()> {
    let ctx = state.steam()?;
    process::launch(&ctx.install).map_err(|e| {
        UiError::from(e).with_action("Start Steam yourself, then reopen Griddle's library.")
    })?;
    tracing::info!("launched Steam at the user's request");
    Ok(())
}

/// Remember whether to offer next time.
///
/// Separate from `prefs`, and deliberately not returning a `Prefs` snapshot: this belongs to
/// `Status`, which is what the first-run gate and the startup offer both read, and returning the
/// wrong snapshot would have the frontend update a store the value does not live in.
#[tauri::command]
pub async fn set_offer_to_start_steam(state: State<'_, AppState>, offer: bool) -> Res<()> {
    let mut settings = state.settings.lock().await;
    settings.offer_to_start_steam = offer;
    state.store.save(&settings)?;
    Ok(())
}

/// Whether Steam is up **and** its library list has arrived.
///
/// The reason this is not `steam_running` is timing. Steam's process appears within a second and
/// its JS realm answers at about three, but the app list is not populated until several seconds
/// after that — measured at 3 s and 7 s respectively on a cold start
/// `[VERIFIED-BOX 2026-08-02]`. A caller that reloads the library on the earlier signal gets the
/// offline list and no indication anything is missing, which is precisely the failure this
/// answers.
///
/// **Never an error.** Steam not running is the ordinary case while something waits for it, and
/// a caller polling this would otherwise have to treat the expected state as a failure. Anything
/// that is not a positive count is `false`.
#[tauri::command]
pub async fn steam_library_ready(state: State<'_, AppState>) -> Res<bool> {
    let Ok((mut steam, _)) = SteamJs::connect(&state.http, &Endpoint::default()).await else {
        return Ok(false);
    };
    Ok(matches!(steam.library_app_count().await, Ok(Some(n)) if n > 0))
}

/// Remember whether to start Steam without asking.
///
/// Writes only its own field. Turning it on makes the offer moot but does **not** clear
/// [`set_offer_to_start_steam`]'s value, so switching it off again restores whatever the user had
/// chosen there rather than leaving them with a prompt they had previously silenced — or without
/// one they wanted.
#[tauri::command]
pub async fn set_auto_start_steam(state: State<'_, AppState>, auto: bool) -> Res<()> {
    let mut settings = state.settings.lock().await;
    settings.auto_start_steam = auto;
    state.store.save(&settings)?;
    Ok(())
}
