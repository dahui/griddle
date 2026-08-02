//! What Griddle found on this machine.

use super::Res;
use crate::state::AppState;
use griddle_core::cdp::Sentinel;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct Status {
    /// Griddle's own version — the first line of any bug report, and the one thing about this
    /// panel that the docs already promised was here.
    ///
    /// Reads `0.0.0` on a development build and that is deliberate: nothing in the repository
    /// carries a version between releases, and `scripts/set-version.ps1` stamps the tag in
    /// during the release job. So `0.0.0` is itself accurate information — it says "this was
    /// not built from a tag".
    pub app_version: &'static str,
    pub steam_root: Option<String>,
    /// Which registry key (or override) produced [`Status::steam_root`].
    ///
    /// Kept for the one failure it explains — the wrong Steam of two installs — and rendered
    /// beside the path rather than as a row of its own, because on its own a registry key path
    /// reads as internals.
    pub steam_source: Option<String>,
    pub account_id: Option<u32>,
    /// Whether a key is *stored*. This is ciphertext-present, not key-usable — see
    /// [`Status::key_unreadable`], which the first-run flow must check alongside it.
    pub has_api_key: bool,
    /// A key is stored but could not be turned into a client — almost always a settings file
    /// carried over from another Windows account, since DPAPI is scoped to the user who sealed
    /// it.
    ///
    /// Without this the two states are indistinguishable to the UI: `has_api_key` is true either
    /// way, so the user sails past first run into a library where every request fails with no
    /// explanation. `settings.api_key()` already reports the difference and `AppState::load`
    /// already logs it — but a `warn!` in a `windows_subsystem = "windows"` binary reaches
    /// nobody.
    pub key_unreadable: bool,
    /// Live apply in one sentence, including whether Steam is up.
    ///
    /// This absorbed the old separate `steam_running` row. [`State::explain`] already
    /// distinguishes "Live apply is on" from "Live apply is on, but Steam isn't running", which
    /// is the same fact in the form that says what it means — and unlike a bare yes/no it does
    /// not read as current when it is a snapshot taken at startup.
    ///
    /// [`State::explain`]: griddle_core::cdp::sentinel::State::explain
    pub sentinel_explanation: String,
    /// How many apps were read from `appinfo.vdf`, or `None` when it could not be read.
    ///
    /// Only the `None` case reaches the screen. The count is a parser statistic — there is no
    /// number a user could compare it against — but "the cache is unreadable, falling back to
    /// the built-in list" explains a library that looks wrong.
    pub app_types_loaded: Option<usize>,
    /// Present only when Steam could not be located, so the UI can explain rather than show an
    /// empty library.
    pub steam_error: Option<String>,
}

/// Everything the diagnostics screen needs, and what the first-run flow branches on.
#[tauri::command]
pub async fn status(state: State<'_, AppState>) -> Res<Status> {
    let settings = state.settings.lock().await;

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

    // Stored, but no client was built from it. The slot is populated only by a successful load
    // or a successful `set_api_key`, so this pair is exactly "stored and unusable" and needs no
    // extra state to track.
    let key_unreadable = settings.has_api_key() && state.sgdb.lock().await.is_none();

    Ok(Status {
        app_version: env!("CARGO_PKG_VERSION"),
        steam_root: root,
        steam_source: source,
        account_id: account,
        has_api_key: settings.has_api_key(),
        key_unreadable,
        sentinel_explanation: sentinel.as_ref().map_or_else(
            || "Steam was not found.".to_owned(),
            |s| s.state().explain().to_owned(),
        ),
        app_types_loaded: app_types,
        steam_error: state.steam.as_ref().err().cloned(),
    })
}
