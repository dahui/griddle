//! What Griddle found on this machine.

use super::Res;
use crate::state::AppState;
use griddle_core::cdp::Sentinel;
use griddle_core::steam::process;
use serde::Serialize;
use tauri::State;

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
