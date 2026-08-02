//! The SteamGridDB API key, and the one link the app opens for the user.
//!
//! [`open_url`] lives here rather than on its own because the only link the app ever opens is
//! the SteamGridDB preferences page this flow sends people to.

use super::Res;
use crate::error::{Kind, UiError};
use crate::state::AppState;
use griddle_core::sgdb::{self, ApiKey};
use std::time::Duration;
use tauri::State;

/// How patient the *validation* request is allowed to be.
///
/// The shipping defaults — a 20 s timeout and 3 retries with backoff — are right for browsing,
/// where a transient failure should be ridden out and nobody is watching a single request. They
/// are wrong here, because a human is sitting in front of a button that says "Checking…": offline,
/// the defaults take over a minute to conclude what is obvious in seconds, with no way to cancel.
///
/// One retry rather than none, so a single dropped packet does not read as a bad key.
fn validation_config() -> sgdb::client::Config {
    sgdb::client::Config {
        timeout: Duration::from_secs(8),
        max_retries: 1,
        ..sgdb::client::Config::default()
    }
}

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

    // Two clients on purpose: the impatient one answers the user, and the one that gets stored
    // keeps the ordinary retry policy for everything afterwards.
    let checker =
        sgdb::Client::with_config(parsed.clone(), validation_config()).map_err(UiError::from)?;
    checker.validate_key().await?;

    let client = sgdb::Client::new(parsed.clone()).map_err(UiError::from)?;

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
