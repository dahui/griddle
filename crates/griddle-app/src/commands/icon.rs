//! Icons for **non-Steam shortcuts** — the one artwork slot that is not just a file.
//!
//! # This is an addition, not a replacement
//!
//! Icons for ordinary Steam games go through [`super::apply_asset`] like every other slot: the
//! image is written to `grid/<appid>_icon.<ext>` and shows after a Steam restart. That works and
//! always has. Nothing here changes it, and nothing here should ever gate it.
//!
//! # What a shortcut needs on top
//!
//! A shortcut's icon is a *path* in `shortcuts.vdf`, which Steam reads at startup. Writing
//! `grid/<appid>_icon.png` alone leaves the file on disk and the icon unchanged.
//!
//! Steam holds that file in memory and rewrites it on exit, so editing it while Steam runs is
//! silently discarded. **The way round it is not to close Steam** — it is to ask Steam to make
//! the change, through `SteamClient.Apps.SetShortcutIcon`, and let it persist the file itself.
//! That is exactly what the Decky plugin does from inside Steam, and it is why the plugin never
//! restarts anything.
//!
//! So:
//!
//! - **Steam running** — write the image, then `SetShortcutIcon` over CDP.
//! - **Steam closed** — write the image, then edit `shortcuts.vdf` directly, which is safe
//!   precisely because Steam is not there to overwrite it.
//!
//! Either way the icon does not appear until Steam restarts. Every icon route has that property,
//! the plugin's included; it is a thing to say in the toast, not a problem to engineer around.
//!
//! **Griddle never closes Steam for this.** An earlier draft of this module did, behind a
//! confirmation. It was unnecessary — the live call had simply not been looked for.

use super::Res;
use crate::error::UiError;
use crate::state::AppState;
use griddle_core::appid::AppId;
use griddle_core::cdp::{Endpoint, SteamJs};
use griddle_core::grid::names::AssetType;
use griddle_core::grid::store::GridDir;
use griddle_core::steam::{process, shortcuts::Shortcuts};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct IconApplied {
    /// Where the image was written.
    pub path: String,
    /// `"live"` when Steam took the change, `"file"` when `shortcuts.vdf` was edited directly.
    pub method: &'static str,
    /// Always true. Icons never appear until Steam restarts, whichever route applied them —
    /// carried as a field anyway so the UI is not asserting it from memory.
    pub needs_restart: bool,
}

/// Apply an icon to a non-Steam shortcut.
///
/// Never closes Steam. With Steam up the change goes through `SetShortcutIcon`; with Steam down
/// `shortcuts.vdf` is edited directly.
#[tauri::command]
pub async fn apply_shortcut_icon(
    state: State<'_, AppState>,
    app_id: u32,
    url: String,
) -> Res<IconApplied> {
    let app = AppId::new(app_id);
    let ctx = state.steam()?;
    let vdf = ctx.install.shortcuts_vdf(ctx.account.id);

    // Read first, so a Steam app is refused before it costs a network round trip. The frontend
    // does not route Steam apps here at all; this is the backstop.
    let mut shortcuts = Shortcuts::load(&vdf)?;
    if shortcuts.find(app).is_none() {
        return Err(UiError::new(
            crate::error::Kind::Unexpected,
            "That app is not a non-Steam shortcut.",
        ));
    }

    let bytes = {
        let guard = state.sgdb.lock().await;
        let client = guard.as_ref().ok_or_else(UiError::no_api_key)?;
        match state.cache.get_image(&url) {
            Some(cached) => cached,
            None => {
                let fetched = client.download(&url).await?;
                if let Err(e) = state.cache.put_image(&url, &fetched) {
                    tracing::warn!(error = %e, "could not cache the downloaded icon");
                }
                fetched
            }
        }
    };

    // The extension is taken from the URL rather than forced to `png`.
    //
    // Artwork can lie about its container because Chromium sniffs content — that is why an
    // animated WebP written as `.png` animates. **A shortcut icon is not rendered by Chromium.**
    // It is a path handed to the OS, so a `.ico` has to stay `.ico`.
    let ext = icon_extension(&url);

    let grid = GridDir::new(state.grid_dir()?);
    grid.ensure()?;
    let applied = grid.apply(app, AssetType::Icon, ext, &bytes)?;
    let icon_path = applied.written.display().to_string();

    let method = if process::is_running() {
        // Steam owns the file while it is up, so it has to make the change itself.
        try_live_icon(&state, app, &icon_path).await?;
        "live"
    } else {
        // Steam is down, so the file is ours to edit — and `save` re-confirms that immediately
        // before writing, in case it came back in the meantime.
        let proof = process::verify_stopped().map_err(UiError::from)?;
        let _ = shortcuts.set_icon(app, &icon_path)?;
        shortcuts.save(&proof)?;
        "file"
    };

    Ok(IconApplied {
        path: icon_path,
        method,
        needs_restart: true,
    })
}

/// Hand the change to Steam.
///
/// No fallback to editing the file: with Steam running that write would be silently discarded on
/// exit, which is worse than an error. If this fails the user is told to close Steam and retry,
/// and the image is already on disk either way.
async fn try_live_icon(state: &State<'_, AppState>, app: AppId, path: &str) -> Result<(), UiError> {
    let (mut steam, _readiness) = SteamJs::connect(&state.http, &Endpoint::default())
        .await
        .map_err(|e| {
            UiError::from(e).with_action(
                "Close Steam and apply the icon again, and Griddle will set it directly.",
            )
        })?;
    // Deliberately not gated on `readiness.can_apply()`. That flag reports on
    // `SetCustomArtworkForApp`, which has nothing to do with this call — refusing on it would
    // block a working icon route because an unrelated one was missing.
    steam
        .set_shortcut_icon(app, path)
        .await
        .map_err(UiError::from)
}

/// Whether this appid is a shortcut, and so whether the icon flow applies at all.
///
/// Read from `shortcuts.vdf` rather than inferred from the appid's range. High-bit appids are
/// what Steam *generates* for shortcuts, but the file is the authority and the folklore about
/// deriving them is disproven — see `appid`, which deliberately contains no CRC32.
#[tauri::command]
pub async fn icon_target(state: State<'_, AppState>, app_id: u32) -> Res<IconTarget> {
    let ctx = state.steam()?;
    let shortcuts = Shortcuts::load(ctx.install.shortcuts_vdf(ctx.account.id))?;
    let found = shortcuts.find(AppId::new(app_id));
    Ok(IconTarget {
        is_shortcut: found.is_some(),
        current_icon: found.and_then(|s| s.icon().map(str::to_owned)),
        steam_running: process::is_running(),
    })
}

#[derive(Debug, Serialize)]
pub struct IconTarget {
    /// False for a real Steam app, whose icon is an ordinary file write with no extra step.
    pub is_shortcut: bool,
    /// The `icon` field as it stands, quotes and all — shortcuts.vdf stores them literally.
    pub current_icon: Option<String>,
    /// Reported for diagnostics only. Both paths work, so nothing branches on it in the UI.
    pub steam_running: bool,
}

/// Pick the on-disk extension for an icon URL.
///
/// SteamGridDB serves icons as PNG and ICO. Anything unrecognised becomes `png`, which is what
/// the rest of the app writes and what Windows will happily render.
fn icon_extension(url: &str) -> &'static str {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase();
    if path.ends_with(".ico") { "ico" } else { "png" }
}

#[cfg(test)]
mod tests {
    use super::icon_extension;

    #[test]
    fn an_ico_url_keeps_its_extension() {
        // The one case that matters: a shortcut icon is a path Steam hands to the OS, not
        // something Chromium sniffs, so renaming a .ico to .png breaks it.
        assert_eq!(
            icon_extension("https://cdn2.steamgriddb.com/icon/a.ico"),
            "ico"
        );
        assert_eq!(
            icon_extension("https://cdn2.steamgriddb.com/icon/a.ICO"),
            "ico"
        );
    }

    #[test]
    fn a_query_string_neither_defeats_nor_fakes_the_check() {
        // Same reasoning as `isVideoPreview` on the frontend, which reads the path for exactly
        // this reason.
        assert_eq!(
            icon_extension("https://cdn2.steamgriddb.com/icon/a.ico?v=2"),
            "ico"
        );
        assert_eq!(
            icon_extension("https://cdn2.steamgriddb.com/icon/a.png?x=.ico"),
            "png"
        );
    }

    #[test]
    fn anything_else_falls_back_to_png() {
        assert_eq!(icon_extension("https://example.invalid/a.webp"), "png");
        assert_eq!(icon_extension(""), "png");
    }
}
