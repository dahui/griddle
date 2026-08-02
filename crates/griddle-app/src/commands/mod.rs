#![allow(
    clippy::let_underscore_must_use,
    reason = "the #[tauri::command] macro expands to `let _ = ...` at each command's signature; \
              the workspace denies that pattern in our own code, which is where it matters"
)]
//! The `invoke` surface. Thin: every decision belongs to `griddle-core`.
//!
//! One module per group of commands, and the groups are the same ones the UI is organised
//! around. Everything they share lives here: [`Res`], the small path helpers, and
//! [`parse_asset_type`], which is the single place the frontend's asset-type vocabulary is
//! translated into the core enum.
//!
//! The commands themselves are re-exported flat, so `main.rs` registers `commands::library`
//! rather than `commands::library::library`.
//!
//! # The apply ladder
//!
//! [`apply_asset`] tries live apply first and falls back to writing files. That order is the
//! whole product thesis — a live apply updates the library in ~30 ms with no restart, which no
//! other Windows tool does — but the fallback is what makes it *shippable*: if Steam moves the
//! API, or the user never enables the debugging sentinel, artwork still applies. It just needs
//! a restart to show up, exactly like Steam Art Manager and SGDBoop.
//!
//! The outcome says which path ran, so the UI can tell the user whether a restart is needed
//! rather than leaving them staring at unchanged art.

mod apikey;
mod apply;
mod diagnostics;
mod library;
mod logo;
mod prefs;
mod reset;
mod search;
mod status;

// Glob re-exports, and they cannot be narrowed to the command names.
//
// `#[tauri::command]` generates two hidden items beside each function — a `__cmd__<name>` macro
// and a `__tauri_command_name_<name>` constant — and `generate_handler!` in `main.rs` expands to
// paths through *those*, not through the function. `pub use status::status` compiles and then
// fails at the handler with "cannot find `__cmd__status` in `commands`", which reads as a
// missing command rather than as a re-export that was too specific.
pub use apikey::*;
pub use apply::*;
pub use diagnostics::*;
pub use library::*;
pub use logo::*;
pub use prefs::*;
pub use reset::*;
pub use search::*;
pub use status::*;

use crate::error::{Kind, UiError};
use griddle_core::appid::AppId;
use griddle_core::grid::names::AssetType;
use griddle_core::grid::store::GridDir;

pub(crate) type Res<T> = Result<T, UiError>;

/// Map the frontend's asset-type string onto the core enum.
///
/// The names match `@griddle/shared`'s `ASSET_TYPES`, which in turn match the Decky plugin's, so
/// the two frontends and the docs all use one vocabulary.
pub(crate) fn parse_asset_type(s: &str) -> Result<AssetType, UiError> {
    match s {
        "grid_p" => Ok(AssetType::Capsule),
        "grid_l" => Ok(AssetType::Header),
        "hero" => Ok(AssetType::Hero),
        "logo" => Ok(AssetType::Logo),
        "icon" => Ok(AssetType::Icon),
        other => Err(UiError::new(
            Kind::Unexpected,
            format!("unknown asset type {other:?}"),
        )),
    }
}

pub(crate) fn path_string(path: Option<std::path::PathBuf>) -> Option<String> {
    path.map(|p| p.display().to_string())
}

pub(crate) fn first_existing(grid: &GridDir, app: AppId, asset: AssetType) -> Option<String> {
    grid.existing(app, asset)
        .first()
        .map(|p| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_type_names_match_the_shared_vocabulary() {
        // These strings are the contract between the Rust bridge, @griddle/shared and the Decky
        // plugin's own naming. A mismatch would send hero art to the capsule slot.
        assert_eq!(parse_asset_type("grid_p").unwrap(), AssetType::Capsule);
        assert_eq!(parse_asset_type("grid_l").unwrap(), AssetType::Header);
        assert_eq!(parse_asset_type("hero").unwrap(), AssetType::Hero);
        assert_eq!(parse_asset_type("logo").unwrap(), AssetType::Logo);
        assert_eq!(parse_asset_type("icon").unwrap(), AssetType::Icon);
    }

    #[test]
    fn the_two_extra_zoom_targets_are_not_asset_types() {
        // `set_zoom` accepts seven names; `parse_asset_type` must still accept only five. If these
        // ever started parsing, "library" would resolve to some slot and a size change on the game
        // list would silently resize a browsing tab instead.
        for extra in super::prefs::EXTRA_ZOOM_TARGETS {
            assert!(
                parse_asset_type(extra).is_err(),
                "{extra} must not be an asset type",
            );
        }
    }

    #[test]
    fn an_unknown_asset_type_is_refused_rather_than_defaulted() {
        // Defaulting to the capsule would write art into the wrong slot and look like a
        // rendering bug rather than a wiring bug.
        let err = parse_asset_type("grid").unwrap_err();
        assert_eq!(err.kind, Kind::Unexpected);
        assert!(err.message.contains("grid"), "{}", err.message);
    }

    #[test]
    fn the_portrait_and_wide_slots_do_not_collide() {
        // `grid_p` and `grid_l` both come from SteamGridDB's `grids` endpoint but write to
        // different files, which is exactly the confusion worth guarding.
        let p = parse_asset_type("grid_p").unwrap();
        let l = parse_asset_type("grid_l").unwrap();
        assert_ne!(p, l);
        assert_ne!(p as u32, l as u32);
    }
}
