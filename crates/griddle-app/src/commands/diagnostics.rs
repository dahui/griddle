//! The live-apply self-test behind Settings → Diagnostics.

use super::Res;
use crate::error::UiError;
use crate::state::AppState;
use griddle_core::cdp::{Endpoint, SteamJs};
use serde::Serialize;
use tauri::State;

/// The result of the live-apply self-test.
///
/// This replaced a module-map scan that reported ✓/✕ against eleven structural finders and
/// three named features. All three features belonged to the Big Picture deliverable, which is
/// cut — so the panel was reporting on capabilities the app does not have and cannot lose.
///
/// What remains is the only thing the desktop app actually depends on from Steam's realm, and it
/// is deliberately a single `typeof` check: `SetCustomArtworkForApp` is bound by the CEF host,
/// not by Steam's bundle, so there is no build-specific discovery left to do.
#[derive(Debug, Serialize)]
pub struct LiveApplyCheck {
    /// Steam's build stamp. Reported so a bug report can name the build, never acted on.
    pub clstamp: Option<String>,
    pub can_apply: bool,
}

/// Connect to Steam's realm and confirm artwork can be applied without a restart.
///
/// The diagnostics screen is a shipped feature because most failures in this product are
/// environmental — Steam not running, a port taken, the sentinel removed. This is the one check
/// that distinguishes "live apply works" from "artwork will be written to disk and need a
/// restart", which is the only difference the user can actually feel.
#[tauri::command]
pub async fn live_apply_check(state: State<'_, AppState>) -> Res<LiveApplyCheck> {
    let (_, readiness) = SteamJs::connect(&state.http, &Endpoint::default())
        .await
        .map_err(UiError::from)?;
    Ok(LiveApplyCheck {
        clstamp: readiness.clstamp.clone(),
        can_apply: readiness.can_apply(),
    })
}
