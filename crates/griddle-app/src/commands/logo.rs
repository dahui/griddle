//! Where a custom logo sits within the hero banner.
//!
//! Steam stores this beside the artwork, in `grid/<appid>.json`, and it is the one piece of
//! artwork state that is not an image. A custom logo with **no** stored position may not render
//! at all, which is why `grid::store` writes a default alongside every logo it applies — this
//! module is what lets the user then move it.
//!
//! Same ladder as artwork: live through Steam's API first, the file as the floor. The live call
//! is worth more here than anywhere else in the app, because positioning is something you do by
//! eye — a round trip through "write the file, restart Steam, look, adjust" is not a positioner,
//! it is a guessing game.

use super::Res;
use crate::error::{Kind, UiError};
use crate::state::AppState;
use griddle_core::appid::AppId;
use griddle_core::cdp::{Endpoint, SteamJs};
use griddle_core::grid::store::GridDir;
use griddle_core::logo::{DEFAULT_POSITION, LogoPosition};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct LogoPlacement {
    /// What is stored today, or `None` when the app has never had a position written.
    pub position: Option<LogoPosition>,
    /// What a reset would restore, so the UI can offer it without duplicating the constant.
    pub default: LogoPosition,
}

/// Read the stored logo position.
///
/// From the file rather than from Steam, deliberately: the file is what exists with Steam closed,
/// and it is the same value Steam loaded at startup. Asking the live client would make the
/// positioner unavailable in exactly the state where the file path is the only thing that works.
#[tauri::command]
pub async fn logo_placement(state: State<'_, AppState>, app_id: u32) -> Res<LogoPlacement> {
    let grid = GridDir::new(state.grid_dir()?);
    Ok(LogoPlacement {
        position: grid.read_logo_position(AppId::new(app_id))?,
        default: DEFAULT_POSITION,
    })
}

#[derive(Debug, Serialize)]
pub struct LogoMoved {
    /// `"live"` or `"file"`.
    pub method: &'static str,
    pub needs_restart: bool,
    pub path: Option<String>,
    /// Why the live path was not used, when it was not. A note, not an error.
    pub fell_back_because: Option<String>,
}

/// Move the logo, live if Steam is running and by file if it is not.
///
/// **The file is written either way**, which is the one place this differs from an artwork apply.
/// A live artwork apply was *measured* writing the file itself (S3, with a before/after diff of
/// `grid/`); whether `SetCustomLogoPositionForApp` does the same has **not** been checked. Rather
/// than assume it, this writes the file unconditionally: it costs one small JSON document, and it
/// guarantees a later read returns the same value whichever path ran.
#[tauri::command]
pub async fn set_logo_placement(
    state: State<'_, AppState>,
    app_id: u32,
    position: LogoPosition,
) -> Res<LogoMoved> {
    let app = AppId::new(app_id);

    // Refused here rather than clamped. The bounds are the UI's — it owns the sliders and the
    // ramp — and silently accepting a nonsense value would leave the file disagreeing with what
    // is on screen.
    if !position.width_pct.is_finite()
        || !position.height_pct.is_finite()
        || !(0.0..=100.0).contains(&position.width_pct)
        || !(0.0..=100.0).contains(&position.height_pct)
    {
        return Err(UiError::new(
            Kind::Unexpected,
            "A logo position must be a percentage between 0 and 100.",
        ));
    }

    let fell_back_because = match try_live_move(&state, app, position).await {
        Ok(()) => None,
        Err(e) => {
            tracing::info!(error = %e, "live logo move unavailable; writing the file instead");
            Some(e.message)
        }
    };

    let grid = GridDir::new(state.grid_dir()?);
    grid.ensure()?;
    let path = grid.write_logo_position(app, position)?;

    Ok(LogoMoved {
        method: if fell_back_because.is_none() {
            "live"
        } else {
            "file"
        },
        // Only when the live call did not land. The file is written either way, so its presence
        // says nothing about whether a restart is needed.
        needs_restart: fell_back_because.is_some(),
        path: Some(path.display().to_string()),
        fell_back_because,
    })
}

async fn try_live_move(
    state: &State<'_, AppState>,
    app: AppId,
    position: LogoPosition,
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
        .set_logo_position(app, position)
        .await
        .map_err(UiError::from)
}
