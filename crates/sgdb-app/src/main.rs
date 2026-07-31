//! Desktop shell. Thin by design — every decision belongs to `sgdb-core`.
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
                .unwrap_or_else(|_| "sgdb_core=info,sgdb_app=info".into()),
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

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bpm_bundle_len,
            commands::status,
            commands::set_api_key,
            commands::clear_api_key,
            commands::library,
            commands::search_assets,
            commands::apply_asset,
            commands::clear_asset,
            commands::set_live_apply,
            commands::remove_sentinel,
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
