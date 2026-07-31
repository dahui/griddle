//! Desktop shell. Thin by design — every decision belongs to `griddle-core`.
//!
//! `windows_subsystem = "windows"` is the whole reason this project exists in preference to
//! running Decky on Windows: no console window flashes on launch. **Verify it by starting the
//! built exe from Explorer, not from a terminal** — a terminal launch hides the regression.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod error;
mod state;

use tauri::Manager as _;

/// The Big Picture injection bundle, staged by `build.rs`.
///
/// Not used yet — M6. Referenced here so a missing or malformed bundle is a build-time failure
/// rather than a runtime surprise during injection.
const BPM_BUNDLE: &str = include_str!(concat!(env!("OUT_DIR"), "/bpm.js"));

#[tauri::command]
fn bpm_bundle_len() -> usize {
    BPM_BUNDLE.len()
}

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
            // 🔴 `recursive = true`, unlike the grid grant above. 278 of the 2248 cached apps
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

            // Live apply is set up, not offered.
            //
            // Applying artwork without restarting Steam is the entire reason this app exists
            // rather than Steam Art Manager or SGDBoop, and its one prerequisite is an empty
            // `.cef-enable-remote-debugging` file in Steam's folder — Valve's own flag, the same
            // one CSS Loader and Decky rely on. Behind an opt-in checkbox, the product shipped
            // switched off for anyone who never found it.
            //
            // 🔑 It is disclosed rather than silent: the first-run screen says what the file is
            // and that deleting it undoes everything. Creating it opens Steam's CEF debugging
            // port on loopback at Steam's next start, which is a real (if modest) widening and
            // belongs in that copy.
            //
            // Re-run on every launch on purpose — `enable()` is idempotent and never truncates,
            // so this also repairs the file if something removed it. Millennium is known to.
            //
            // Never fatal: the apply ladder falls back to writing files, which needs no port.
            if let Ok(ctx) = state.steam() {
                let sentinel = griddle_core::cdp::Sentinel::for_install(&ctx.install);
                match sentinel.enable() {
                    Ok(()) => tracing::info!(path = %sentinel.path().display(), "live apply ready"),
                    Err(e) => {
                        tracing::warn!(error = %e, "could not enable live apply; artwork will be written to disk instead");
                    }
                }
            }

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bpm_bundle_len,
            commands::status,
            commands::set_api_key,
            commands::clear_api_key,
            commands::library,
            commands::open_url,
            commands::prefs,
            commands::set_library_view,
            commands::set_filters,
            commands::reset_filters,
            commands::set_game_override,
            commands::search_games,
            commands::current_game_match,
            commands::search_assets,
            commands::apply_asset,
            commands::asset_status,
            commands::clear_asset,
            commands::reset_all_plan,
            commands::reset_all_art,
            commands::resolve_modules,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // The one place a hard exit is right: if the webview cannot start there is no UI
            // in which to report the failure.
            eprintln!("fatal: could not start the application window: {e}");
            std::process::exit(1);
        });
}
