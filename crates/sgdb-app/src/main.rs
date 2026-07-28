//! Desktop shell. Thin by design — every decision belongs to `sgdb-core`.
//!
//! `windows_subsystem = "windows"` is the whole reason this project exists in preference to
//! running Decky on Windows: no console window flashes on launch. **Verify it by starting the
//! built exe from Explorer, not from a terminal** — a terminal launch hides the regression.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// The Big Picture injection bundle, staged by `build.rs`.
///
/// Not used yet — M6. Referenced here so a missing or malformed bundle is a build-time
/// failure rather than a runtime surprise during the spike.
const BPM_BUNDLE: &str = include_str!(concat!(env!("OUT_DIR"), "/bpm.js"));

#[tauri::command]
fn bpm_bundle_len() -> usize {
    BPM_BUNDLE.len()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![bpm_bundle_len])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // The one place a hard exit is right: if the webview can't start there is no UI
            // in which to report the failure.
            eprintln!("fatal: could not start the application window: {e}");
            std::process::exit(1);
        });
}
