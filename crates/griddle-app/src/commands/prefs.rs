//! Persisted UI state: library view, filters, and the manual game overrides.
//!
//! Every mutating command here returns the whole [`Prefs`] snapshot, so the frontend never has
//! to guess what the store now holds.

use super::Res;
use crate::state::AppState;
use griddle_core::settings::{LibraryScope, LibrarySort};
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::State;

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
