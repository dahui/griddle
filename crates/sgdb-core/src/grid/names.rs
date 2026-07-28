//! Artwork filenames in `userdata/<accountid>/config/grid/`.
//!
//! Verified against the real folder on this machine, which holds all five types at once and
//! with **mixed extensions** — so extension is per-file, not per-library:
//!
//! ```text
//! 4048848997.jpg        wide capsule (no suffix)
//! 4048848997p.png       portrait capsule
//! 4048848997_hero.jpg   hero
//! 4048848997_logo.png   logo
//! 4048848997_icon.ico   icon
//! ```
//! `[VERIFIED-BOX 2026-07-27]`
//!
//! The ordinals in [`AssetType`] are Steam's own `ELibraryAssetType`, confirmed by applying
//! each in turn and observing which file appeared — for a shortcut *and* a real Steam app,
//! with identical results. See CLAUDE.md.

use crate::appid::AppId;
use std::fmt;

/// Steam's `ELibraryAssetType`. Discriminants are load-bearing — they cross the CDP boundary.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum AssetType {
    /// Portrait capsule, `600x900`. → `<appid>p.<ext>`
    Capsule = 0,
    /// → `<appid>_hero.<ext>`
    Hero = 1,
    /// → `<appid>_logo.<ext>`
    Logo = 2,
    /// Wide capsule, `920x430`. → `<appid>.<ext>` (no suffix)
    Header = 3,
    /// → `<appid>_icon.<ext>`.
    ///
    /// 🔴 **Not settable through `SteamClient.Apps.SetCustomArtworkForApp`** — ordinal 4 is a
    /// silent no-op there (returns normally after ~500 ms, writes nothing), for shortcuts and
    /// Steam apps alike. Icons must go through the file path, plus `shortcuts.vdf` and a Steam
    /// restart for non-Steam games. `[VERIFIED-BOX 2026-07-27]`
    Icon = 4,
    /// Steam has it; neither we nor the Decky plugin edit it. Also a no-op via the API.
    HeroBlur = 5,
}

impl AssetType {
    pub const EDITABLE: [AssetType; 5] = [
        AssetType::Capsule,
        AssetType::Header,
        AssetType::Hero,
        AssetType::Logo,
        AssetType::Icon,
    ];

    /// True when the live CDP apply path works for this type; false means file-write only.
    pub const fn supports_live_apply(self) -> bool {
        matches!(
            self,
            AssetType::Capsule | AssetType::Hero | AssetType::Logo | AssetType::Header
        )
    }

    /// The filename suffix between the appid and the extension.
    const fn suffix(self) -> &'static str {
        match self {
            AssetType::Capsule => "p",
            AssetType::Header => "",
            AssetType::Hero => "_hero",
            AssetType::Logo => "_logo",
            AssetType::Icon => "_icon",
            AssetType::HeroBlur => "_hero_blur",
        }
    }

    /// SteamGridDB's name for this asset kind.
    pub const fn sgdb_name(self) -> &'static str {
        match self {
            AssetType::Capsule => "grid_p",
            AssetType::Header => "grid_l",
            AssetType::Hero => "hero",
            AssetType::Logo => "logo",
            AssetType::Icon => "icon",
            AssetType::HeroBlur => "hero_blur",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            AssetType::Capsule => "Capsule",
            AssetType::Header => "Wide Capsule",
            AssetType::Hero => "Hero",
            AssetType::Logo => "Logo",
            AssetType::Icon => "Icon",
            AssetType::HeroBlur => "Hero Blur",
        }
    }
}

impl fmt::Display for AssetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Extensions Steam reads for artwork.
///
/// **`.webp` is absent deliberately.** SGDBoop rewrites a `.webp` URL's extension to `.png`
/// before saving, and Steam's own code always passes the literal mime `"png"`. Animated WebP
/// bytes in a `.png` file animate correctly — verified in the desktop library and Big Picture
/// — because Chromium sniffs content, not extension. So we never write a `.webp` file.
pub const ART_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// Extensions an icon may use. `.ico` is included because Steam accepts it and EmuDeck writes
/// it — the real shortcut on this machine has `4048848997_icon.ico`.
pub const ICON_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "ico"];

/// The filename for one asset, e.g. `4048848997p.png`.
pub fn file_name(app: AppId, asset: AssetType, ext: &str) -> String {
    format!(
        "{}{}.{}",
        app.get(),
        asset.suffix(),
        ext.trim_start_matches('.')
    )
}

/// Every filename that could hold this asset — the set to delete before writing a new one.
///
/// 🔴 **Cleaning siblings is required, not tidiness.** If `<appid>p.png` and `<appid>p.jpg`
/// both exist, which one Steam picks is undefined and has changed between versions. SGDBoop
/// deletes the same set for the same reason. `[VERIFIED-SOURCE]`
///
/// Ordering is stable so callers and tests can compare directly.
pub fn siblings(app: AppId, asset: AssetType) -> Vec<String> {
    let exts: &[&str] = if asset == AssetType::Icon {
        &ICON_EXTENSIONS
    } else {
        &ART_EXTENSIONS
    };
    exts.iter().map(|e| file_name(app, asset, e)).collect()
}

/// The logo-position sidecar, `<appid>.json`.
///
/// Note this collides in stem with the Header asset (`<appid>.png`) — same base name, different
/// extension. Clearing artwork must not delete the `.json` unless the logo itself is cleared.
pub fn logo_position_file_name(app: AppId) -> String {
    format!("{}.json", app.get())
}

/// Legacy artwork filenames keyed by the 64-bit BPID, written by much older clients.
///
/// Read-only concern: worth *finding* so a stale file can be reported, never worth writing.
pub fn legacy_file_name(app: AppId, asset: AssetType, ext: &str) -> String {
    format!(
        "{}{}.{}",
        app.to_bpid(),
        asset.suffix(),
        ext.trim_start_matches('.')
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    /// Exactly the five files present in the real grid folder.
    #[test]
    fn matches_the_real_grid_folder() {
        let a = AppId::new(4_048_848_997);
        assert_eq!(file_name(a, AssetType::Header, "jpg"), "4048848997.jpg");
        assert_eq!(file_name(a, AssetType::Capsule, "png"), "4048848997p.png");
        assert_eq!(file_name(a, AssetType::Hero, "jpg"), "4048848997_hero.jpg");
        assert_eq!(file_name(a, AssetType::Logo, "png"), "4048848997_logo.png");
        assert_eq!(file_name(a, AssetType::Icon, "ico"), "4048848997_icon.ico");
    }

    /// The ordinals cross the CDP boundary; an off-by-one would write hero art into the
    /// capsule slot. Measured, not assumed.
    #[test]
    fn ordinals_match_steams_enum() {
        assert_eq!(AssetType::Capsule as u32, 0);
        assert_eq!(AssetType::Hero as u32, 1);
        assert_eq!(AssetType::Logo as u32, 2);
        assert_eq!(AssetType::Header as u32, 3);
        assert_eq!(AssetType::Icon as u32, 4);
        assert_eq!(AssetType::HeroBlur as u32, 5);
    }

    #[test]
    fn only_the_four_working_types_support_live_apply() {
        assert!(AssetType::Capsule.supports_live_apply());
        assert!(AssetType::Hero.supports_live_apply());
        assert!(AssetType::Logo.supports_live_apply());
        assert!(AssetType::Header.supports_live_apply());
        // Both verified to write nothing through SetCustomArtworkForApp.
        assert!(!AssetType::Icon.supports_live_apply());
        assert!(!AssetType::HeroBlur.supports_live_apply());
    }

    #[test]
    fn a_leading_dot_on_the_extension_is_accepted() {
        let a = AppId::new(7);
        assert_eq!(file_name(a, AssetType::Capsule, ".png"), "7p.png");
        assert_eq!(file_name(a, AssetType::Capsule, "png"), "7p.png");
    }

    #[test]
    fn siblings_cover_every_extension_steam_reads() {
        let a = AppId::new(620);
        assert_eq!(
            siblings(a, AssetType::Capsule),
            ["620p.png", "620p.jpg", "620p.jpeg"]
        );
        assert_eq!(
            siblings(a, AssetType::Header),
            ["620.png", "620.jpg", "620.jpeg"]
        );
        // Icons additionally allow .ico.
        assert_eq!(
            siblings(a, AssetType::Icon),
            [
                "620_icon.png",
                "620_icon.jpg",
                "620_icon.jpeg",
                "620_icon.ico"
            ]
        );
    }

    #[test]
    fn siblings_never_include_webp() {
        for t in AssetType::EDITABLE {
            assert!(
                !siblings(AppId::new(1), t)
                    .iter()
                    .any(|s| s.ends_with(".webp")),
                "{t:?} must not produce a .webp filename",
            );
        }
    }

    /// The Header asset and the logo-position sidecar share a base name. Clearing artwork must
    /// not take the `.json` with it by accident.
    #[test]
    fn header_and_logo_position_share_a_stem_but_not_a_filename() {
        let a = AppId::new(4_048_848_997);
        assert_eq!(logo_position_file_name(a), "4048848997.json");
        assert!(!siblings(a, AssetType::Header).contains(&logo_position_file_name(a)));
    }

    #[test]
    fn legacy_names_use_the_bpid() {
        let a = AppId::new(4_048_848_997);
        let legacy = legacy_file_name(a, AssetType::Capsule, "png");
        assert!(legacy.starts_with(&a.to_bpid().to_string()));
        assert!(legacy.ends_with("p.png"));
        assert_ne!(legacy, file_name(a, AssetType::Capsule, "png"));
    }

    #[test]
    fn sgdb_names_map_to_the_api_endpoints() {
        // grid_p and grid_l both hit /grids; the orientation is a dimension filter.
        assert_eq!(AssetType::Capsule.sgdb_name(), "grid_p");
        assert_eq!(AssetType::Header.sgdb_name(), "grid_l");
        assert_eq!(AssetType::Hero.sgdb_name(), "hero");
        assert_eq!(AssetType::Logo.sgdb_name(), "logo");
        assert_eq!(AssetType::Icon.sgdb_name(), "icon");
    }
}
