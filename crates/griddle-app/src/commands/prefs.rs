//! Persisted UI state: library view, filters, and the manual game overrides.
//!
//! Every mutating command here returns the whole [`Prefs`] snapshot, so the frontend never has
//! to guess what the store now holds.

use super::Res;
use crate::error::{Kind, UiError};
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

/// The two resizable grids that are not asset types.
///
/// The library list and the Current-artwork overview are the same kind of thing to a user — a
/// wrapping grid of pictures they scroll — so they carry a tile size like the five browsing tabs
/// do. Kept beside [`set_zoom`] rather than in `parse_asset_type`, which every other command uses
/// and which must keep rejecting anything that is not a real asset type.
pub(super) const EXTRA_ZOOM_TARGETS: [&str; 2] = ["library", "current"];

/// Remember how wide one grid's tiles are, in rem.
///
/// The **bounds are not checked here**, deliberately. They live in `ZOOM` in `@griddle/shared`,
/// next to the stylesheet they describe, and the frontend clamps on read — so a value stored by
/// one build survives a later one moving the range instead of being rewritten. Duplicating that
/// table in Rust would be a second copy to hold in step, which is the failure this codebase keeps
/// choosing to design out rather than remember.
///
/// What *is* checked: the asset type, through the same `parse_asset_type` every other command
/// uses, so an unknown key cannot accumulate in `settings.json`; and that the value is finite and
/// positive, which is a correctness floor rather than a layout policy — zero or NaN reaches CSS
/// as a grid with no columns, and an empty tab reads as "SteamGridDB has nothing for this game".
#[tauri::command]
pub async fn set_zoom(state: State<'_, AppState>, asset_type: String, value: f32) -> Res<Prefs> {
    // Called for its rejection, not its result: an unknown key must not reach the file.
    if !EXTRA_ZOOM_TARGETS.contains(&asset_type.as_str()) {
        super::parse_asset_type(&asset_type)?;
    }
    if !value.is_finite() || value <= 0.0 {
        return Err(UiError::new(
            Kind::Unexpected,
            format!("{asset_type} zoom must be a positive number, not {value}"),
        ));
    }
    {
        let mut settings = state.settings.lock().await;
        // Stored under the wire name (`grid_p`), not `AssetType`'s display label. Every other
        // command speaks SteamGridDB's vocabulary across this boundary, and a settings file that
        // used both would need a translation table to read.
        let _ = settings.zoom.insert(asset_type, value);
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
