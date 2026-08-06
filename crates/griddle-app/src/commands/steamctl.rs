//! Starting and restarting Steam, and remembering whether to offer either.
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
//!
//! # Why restarting is a second question and not a variation on the first
//!
//! Steam being *up* is not the same as Griddle being able to talk to it.
//! `.cef-enable-remote-debugging` is created silently at every launch and Steam reads it only when
//! it starts, so on the launch that first creates it a running Steam still has no debugging port —
//! and the user gets the offline library and file-only artwork with nothing saying why.
//!
//! [`steam_debug_ready`] is what detects that, and [`restart_steam`] is the remedy. It has its own
//! preference rather than sharing [`set_offer_to_start_steam`]'s because the two cost different
//! amounts: one starts a program, the other stops one and takes any running game with it. Wanting
//! the first and not the second is an entirely reasonable position.

use super::Res;
use crate::error::UiError;
use crate::state::AppState;
use griddle_core::cdp::{Endpoint, SteamJs, target};
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

/// Whether Steam's CEF debugging port is open and answering as Steam.
///
/// This is the question `Sentinel::state` cannot answer. It reports `PresentSteamRunning` whenever
/// the file exists and Steam is up — including when Steam was started *before* the file appeared,
/// which is the ordinary state on the launch where Griddle created it. The port opens at Steam's
/// next start and nothing on disk records whether that has happened, so the only way to know is to
/// ask the port.
///
/// **`fetch_version`, not a full [`SteamJs::connect`], and the difference matters.** `connect` also
/// requires `SharedJSContext`, which does not exist for the first few seconds of a Steam start
/// `[VERIFIED-BOX 2026-08-02]` — so it answers "unreachable" during a window in which restarting
/// Steam would achieve nothing. What the restart offer needs is narrower: is the *port* open. That
/// is one loopback GET rather than a WebSocket handshake, which also matters because
/// [`steam_library_ready`] is polling the same port on the same interval.
///
/// **Never an error**, for the same reason as [`steam_library_ready`]: a closed port is the
/// ordinary case while something waits for it, and a caller polling this would otherwise have to
/// treat the expected state as a failure.
#[tauri::command]
pub async fn steam_debug_ready(state: State<'_, AppState>) -> Res<bool> {
    match target::fetch_version(&state.http, &Endpoint::default()).await {
        Ok(v) if v.looks_like_steam() => Ok(true),
        // Port 8080 is a very common dev-server port. Restarting Steam cannot free it, so this is
        // the one negative worth a log line rather than a shrug: the offer that would otherwise
        // follow asks the user to do something that cannot possibly help.
        Ok(v) => {
            tracing::warn!(browser = %v.browser, "port 8080 is answering, but not as Steam");
            Ok(false)
        }
        Err(e) => {
            tracing::debug!(error = %e, "Steam's debugging port is not answering");
            Ok(false)
        }
    }
}

/// Ask Steam to exit, wait for it to go, and start it again.
///
/// This exists because creating the debugging sentinel is not enough: Steam reads it at startup,
/// so until it restarts, artwork can only be written to disk and **All games** is the offline
/// list. Griddle used to create the file and say nothing, leaving the user with a library a few
/// hundred games short and no reason to suspect it.
///
/// # It runs on a blocking thread, and that is not optional
///
/// [`process::shutdown`] polls the process list with `std::thread::sleep` for up to 45 seconds.
/// Awaiting that on a Tokio worker would park a runtime thread for the duration. It also does
/// **not** wait for Steam to come back up: that is another minute and a half, and every live
/// feature already re-checks on use — `SteamListWatcher` on the frontend is what notices the
/// library arriving and reloads it.
///
/// The `SteamStopped` token is deliberately dropped. It gates *writes* to `shortcuts.vdf`, and
/// nothing is being written here; there is no second observation to reconfirm against.
#[tauri::command]
pub async fn restart_steam(state: State<'_, AppState>) -> Res<()> {
    // Cloned out before the await: `State` cannot be held across one, and `SteamInstall` is a
    // path plus how it was found.
    let install = state.steam()?.install.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || -> Result<(), process::Error> {
        let _stopped = process::shutdown(&install, process::DEFAULT_SHUTDOWN_TIMEOUT)?;
        process::launch(&install)
    })
    .await;

    match joined {
        Ok(Ok(())) => {
            tracing::info!("restarted Steam at the user's request");
            Ok(())
        }
        // `UiError::from` already carries tuned wording for the variants that are about Steam
        // being in the way, `ShutdownTimedOut` — the likeliest failure here — included. Only
        // `Spawn` and `SteamExeMissing` arrive without one, and for those the remedy is the same
        // thing the user was about to have done for them.
        Ok(Err(e)) => {
            let ui = UiError::from(e);
            Err(if ui.action.is_some() {
                ui
            } else {
                ui.with_action(
                    "Close Steam and start it again yourself. Its debugging port opens at the \
                     next start either way.",
                )
            })
        }
        // The blocking task panicked or was cancelled. Reported rather than folded into a generic
        // failure: returning `Ok` here would tell the user Steam had been restarted when nothing
        // happened at all.
        Err(e) => Err(UiError::unexpected(e)),
    }
}

/// Remember whether to offer a Steam restart on future startups.
///
/// Its own field and its own switch rather than a second meaning for
/// [`set_offer_to_start_steam`]. The two prompts ask different things and cost different amounts:
/// one starts a program, the other stops one and takes any running game with it. Somebody may well
/// want the first and not the second.
#[tauri::command]
pub async fn set_offer_to_restart_steam(state: State<'_, AppState>, offer: bool) -> Res<()> {
    let mut settings = state.settings.lock().await;
    settings.offer_to_restart_steam = offer;
    state.store.save(&settings)?;
    Ok(())
}
