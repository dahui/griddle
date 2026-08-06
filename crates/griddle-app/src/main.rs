// Test assertions are allowed to panic; the shipping code is not. Stated once here rather than
// repeated on every test module -- see the same attribute in `griddle-core/src/lib.rs`.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Desktop shell. Thin by design — every decision belongs to `griddle-core`.
//!
//! `windows_subsystem = "windows"` is the whole reason this project exists in preference to
//! running Decky on Windows: no console window flashes on launch. **Verify it by starting the
//! built exe from Explorer, not from a terminal** — a terminal launch hides the regression.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
#[cfg(windows)]
mod fatal;
mod state;

use tauri::{Emitter as _, Manager as _};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "griddle_core=info,griddle_app=info".into()),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let state = state::AppState::load();

            // Grant the webview read access to exactly one directory: the account's `grid/`.
            //
            // The scope is set at runtime rather than in tauri.conf.json because the path
            // depends on the Steam install and account id. Scoping it to that single directory
            // — rather than the drive, or Steam's whole tree — keeps the webview unable to
            // read anything else if it is ever compromised by a remote image.
            if let Ok(grid) = state.grid_dir() {
                match app.asset_protocol_scope().allow_directory(&grid, false) {
                    Ok(()) => tracing::info!(path = %grid.display(), "asset scope granted"),
                    Err(e) => tracing::warn!(error = %e, "could not grant the asset scope"),
                }
            }

            // The second scope: Steam's own artwork cache, so the library list can show default
            // art for games the user has never customised. Read-only — nothing in this app
            // writes there, and `steam::librarycache` contains no write at all.
            //
            // `recursive = true`, unlike the grid grant above. 278 of the 2248 cached apps
            // store their art one level down under a sha1 directory, and a non-recursive grant
            // would 403 exactly those — the failure would look like "some games have no art",
            // which is indistinguishable from the cache simply not having it.
            //
            // This is a genuine widening: ~2248 directories of Steam-owned store artwork. It is
            // still far narrower than the Steam root, and contains nothing but public images.
            if let Ok(ctx) = state.steam() {
                let cache = ctx.install.library_cache_dir();
                match app.asset_protocol_scope().allow_directory(&cache, true) {
                    Ok(()) => tracing::info!(path = %cache.display(), "librarycache scope granted"),
                    // Deliberately not fatal: without this the UI falls through to Steam's CDN
                    // and still shows art, just over the network.
                    Err(e) => tracing::warn!(error = %e, "could not grant the librarycache scope"),
                }
            }

            // Live apply is set up, not offered. Why, and what it costs: `cdp::sentinel`.
            //
            // Two things specific to doing it *here*: it runs on every launch because `enable()`
            // is idempotent and never truncates, so this also repairs the file if something
            // removed it (Millennium is known to) — and it is never fatal, because the apply
            // ladder falls back to writing files, which needs no port at all.
            //
            // "live apply ready" means the *file* is in place, not that the port is open. Steam
            // reads it only at startup, so on the launch that creates it a running Steam still has
            // no port — which silently costs live apply and a few hundred games in All games.
            // Nothing here can tell the difference, and deliberately does not try: the frontend
            // asks the port itself (`steam_debug_ready`) and offers a restart.
            if let Ok(ctx) = state.steam() {
                let sentinel = griddle_core::cdp::Sentinel::for_install(&ctx.install);
                match sentinel.enable() {
                    Ok(()) => tracing::info!(path = %sentinel.path().display(), "live apply ready"),
                    Err(e) => {
                        tracing::warn!(error = %e, "could not enable live apply; artwork will be written to disk instead");
                    }
                }
            }

            // Controller navigation.
            //
            // Read natively rather than through the webview's Gamepad API, which two open
            // WebView2 bugs rule out — #5507 kills gamepad input in WebView2 apps whenever the
            // Steam Overlay is attached, and launching Griddle from Big Picture always attaches
            // it. See `griddle_core::input`.
            //
            // Gated on window focus, because this reads the pad *globally*: without the gate,
            // playing a game with a controller would also be driving Griddle in the background.
            let gate = griddle_core::input::FocusGate::new(true);

            // Taken from the window list rather than looked up by the label `"main"`. The
            // label is not set in `tauri.conf.json`, so it is only `"main"` by Tauri's default —
            // and an `if let Some(...)` on a wrong guess starts no input thread **and says
            // nothing**, which surfaces as "my controller does nothing" with no way to tell that
            // from a driver problem. There is exactly one window; take it and complain if not.
            match app.webview_windows().values().next().cloned() {
                Some(window) => {
                    tracing::info!(
                        label = window.label(),
                        pads = ?griddle_core::input::connected(),
                        "controller navigation starting"
                    );

                    let for_events = gate.clone();
                    window.on_window_event(move |event| {
                        if let tauri::WindowEvent::Focused(focused) = event {
                            for_events.set(*focused);
                        }
                    });

                    // The handle is dropped, which detaches the thread deliberately: it should
                    // live for the whole process, and there is no shutdown to join it at.
                    let emitter = window.clone();
                    drop(griddle_core::input::spawn(gate, move |action| {
                        if let Err(e) = emitter.emit("nav", action) {
                            tracing::debug!(error = %e, "could not deliver a controller action");
                        }
                    }));
                }
                None => tracing::warn!("no webview window; controller navigation is unavailable"),
            }

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::set_api_key,
            commands::clear_api_key,
            commands::library,
            commands::open_url,
            commands::prefs,
            commands::set_library_view,
            commands::set_filters,
            commands::reset_filters,
            commands::set_zoom,
            commands::set_game_override,
            commands::search_games,
            commands::current_game_match,
            commands::search_assets,
            commands::apply_asset,
            commands::asset_status,
            commands::clear_asset,
            commands::icon_target,
            commands::apply_shortcut_icon,
            commands::logo_placement,
            commands::set_logo_placement,
            commands::reset_all_plan,
            commands::reset_all_art,
            commands::live_apply_check,
            commands::start_steam,
            commands::set_offer_to_start_steam,
            commands::set_auto_start_steam,
            commands::steam_library_ready,
            commands::steam_debug_ready,
            commands::restart_steam,
            commands::set_offer_to_restart_steam,
        ])
        .run(tauri::generate_context!())
        // The one place a hard exit is right: if the webview cannot start there is no UI in
        // which to report the failure. `fatal::no_window` puts up a message box, because
        // `eprintln!` in a `windows_subsystem = "windows"` binary reaches nobody at all — the
        // app would simply fail to appear, with nothing anywhere to say why.
        .unwrap_or_else(|e| fatal::no_window(&e));
}
