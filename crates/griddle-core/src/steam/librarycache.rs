//! Steam's own artwork cache, `appcache/librarycache/<appid>/`.
//!
//! This is where the *default* art comes from — what a game looks like in Steam before anyone
//! applies anything. It is what lets the library list show real capsules instead of a grid of
//! "no artwork" placeholders.
//!
//! # 🔴 READ-ONLY, and structurally so
//!
//! Steam owns this directory and re-downloads over anything placed into it, so this module
//! contains **no filesystem mutation of any kind**. That is why it needs no `boundary-ok:`
//! annotation in `scripts/check-boundaries.sh`, and it **must never acquire one**. If a future
//! change here needs to modify a file, it belongs in `grid::store` instead.
//!
//! # The finder predicate
//!
//! **`appinfo.vdf`'s `common/library_assets_full/<slot>/image/<lang>` is the index.** It holds
//! the path relative to this app's directory, including a `<sha1>/` component when there is one.
//! See [`crate::vdf::appinfo`] for the measurements.
//!
//! Matching on filenames is *not* the predicate, because the same slot is `library_600x900.jpg`
//! for one app and `<sha1>/library_capsule.jpg` for another. Basenames survive only as a
//! fallback for the 1570 apps that have no `library_assets_full` at all and just a bare
//! `header.jpg`. `[VERIFIED-BOX 2026-07-30]`
//!
//! # Why every rung ends in `is_file()`
//!
//! appinfo runs **ahead of disk** — Steam records what artwork an app has before downloading it,
//! so 24-32 paths per slot pointed at nothing on this box. A resolver that trusted appinfo would
//! hand the UI a path that 404s through the `asset:` protocol, which looks like a broken app
//! rather than a game whose art has not been fetched yet.
//!
//! # Why the join is guarded
//!
//! The relative path is a string read out of a 6 MB binary file we do not control. Joining it
//! blind would let a crafted value walk out of the cache and hand the webview any file on disk,
//! since the `asset:` scope is granted to this whole directory tree. [`safe_join`] refuses `..`,
//! absolute paths and drive letters — the same guard, and the same reasoning, as `cache`.

use crate::appid::AppId;
use crate::grid::AssetType;
use crate::steam::apptype::AppTypes;
use crate::steam::locate::SteamInstall;
use crate::vdf::appinfo::DEFAULT_LANGUAGE;
use std::path::{Component, Path, PathBuf};

/// One of Steam's own art slots, named the way `library_assets_full` names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SteamSlot {
    /// The portrait capsule. `library_600x900.jpg` or `<sha1>/library_capsule.jpg`.
    LibraryCapsule,
    LibraryHero,
    LibraryHeroBlur,
    LibraryLogo,
    /// The wide capsule as Steam's *library* stores it — only 172 apps have one.
    LibraryHeader,
    /// The store header, `header.jpg`. Far more common (1856 apps) and the practical fallback
    /// for the wide slot. Lives in `common/header_image`, not in `library_assets_full`.
    Header,
    /// The small icon, `<common/icon>.jpg`. Lives in `common/icon`.
    Icon,
}

impl SteamSlot {
    /// This slot's key under `common/library_assets_full`.
    ///
    /// `None` for the two slots indexed elsewhere: [`SteamSlot::Header`] comes from
    /// `common/header_image` and [`SteamSlot::Icon`] from `common/icon`.
    pub const fn appinfo_key(self) -> Option<&'static str> {
        match self {
            SteamSlot::LibraryCapsule => Some("library_capsule"),
            SteamSlot::LibraryHero => Some("library_hero"),
            SteamSlot::LibraryHeroBlur => Some("library_hero_blur"),
            SteamSlot::LibraryLogo => Some("library_logo"),
            SteamSlot::LibraryHeader => Some("library_header"),
            SteamSlot::Header | SteamSlot::Icon => None,
        }
    }

    /// Which Steam slots can stand in for one of our editable asset types, best first.
    ///
    /// The wide capsule tries the library header first because that is the art Steam actually
    /// renders there, then falls back to the store header, which nearly every app has.
    pub const fn for_asset_type(asset: AssetType) -> &'static [SteamSlot] {
        match asset {
            AssetType::Capsule => &[SteamSlot::LibraryCapsule],
            AssetType::Hero => &[SteamSlot::LibraryHero],
            AssetType::Logo => &[SteamSlot::LibraryLogo],
            AssetType::Header => &[SteamSlot::LibraryHeader, SteamSlot::Header],
            AssetType::Icon => &[SteamSlot::Icon],
            AssetType::HeroBlur => &[SteamSlot::LibraryHeroBlur],
        }
    }

    /// Bare filenames to probe when appinfo has no entry for this app.
    ///
    /// A fallback, never the predicate — see the module docs. These are the names measured flat
    /// under `<appid>/` on this box, most common first.
    pub const fn fallback_basenames(self) -> &'static [&'static str] {
        match self {
            SteamSlot::LibraryCapsule => &["library_600x900.jpg", "library_capsule.jpg"],
            SteamSlot::LibraryHero => &["library_hero.jpg"],
            SteamSlot::LibraryHeroBlur => &["library_hero_blur.jpg"],
            SteamSlot::LibraryLogo => &["logo.png"],
            SteamSlot::LibraryHeader => &["library_header.jpg"],
            SteamSlot::Header => &["header.jpg"],
            // The icon filename is a content hash, so there is nothing to guess. Without
            // `common/icon` there is no way to tell it from any other sha1-named file, and
            // picking "the only sha1 .jpg" would be exactly the positional guess this module
            // exists to avoid.
            SteamSlot::Icon => &[],
        }
    }
}

/// Resolves appids to Steam's own cached artwork. Read-only.
#[derive(Debug, Clone)]
pub struct LibraryCache<'a> {
    root: PathBuf,
    types: Option<&'a AppTypes>,
    language: String,
}

impl<'a> LibraryCache<'a> {
    pub fn new(install: &SteamInstall, types: Option<&'a AppTypes>) -> Self {
        LibraryCache {
            root: install.library_cache_dir(),
            types,
            language: DEFAULT_LANGUAGE.to_owned(),
        }
    }

    /// Prefer a different language's artwork where an app has one.
    #[must_use]
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    /// This app's directory. It may not exist — most appids have no cache entry.
    pub fn app_dir(&self, app: AppId) -> PathBuf {
        self.root.join(app.get().to_string())
    }

    /// Steam's default art for one of our editable asset types, if any is on disk.
    ///
    /// `None` is an ordinary answer, not an error: non-Steam shortcuts never have one, and
    /// plenty of real apps have no cached capsule.
    pub fn resolve(&self, app: AppId, asset: AssetType) -> Option<PathBuf> {
        SteamSlot::for_asset_type(asset)
            .iter()
            .find_map(|slot| self.resolve_slot(app, *slot))
    }

    /// One specific Steam slot.
    pub fn resolve_slot(&self, app: AppId, slot: SteamSlot) -> Option<PathBuf> {
        let dir = self.app_dir(app);

        // 1. The index in appinfo. The durable predicate, and the only one that handles the
        //    sha1-subdirectory layout.
        if let Some(types) = self.types {
            let indexed = match slot {
                SteamSlot::Header => types.header_image(app, &self.language),
                SteamSlot::Icon => None, // handled below: it needs an extension appended
                _ => slot
                    .appinfo_key()
                    .and_then(|key| types.library_asset(app, key, &self.language)),
            };
            if let Some(rel) = indexed
                && let Some(path) = safe_join(&dir, rel)
                && path.is_file()
            {
                return Some(path);
            }

            // 2. The icon is a bare sha1 in appinfo; the file adds `.jpg`.
            if slot == SteamSlot::Icon
                && let Some(sha1) = types.icon_sha1(app)
                && let Some(path) = safe_join(&dir, &format!("{sha1}.jpg"))
                && path.is_file()
            {
                return Some(path);
            }
        }

        // 3. Bare filenames, for apps with no `library_assets_full` entry.
        for name in slot.fallback_basenames() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        // A tripwire, not an error. The claim that lets us skip a directory scan is that the
        // sha1-subdirectory shape only ever occurs for apps that *also* have a
        // `library_assets_full` entry. If that stops holding, this fires.
        if cfg!(debug_assertions) && has_subdirectory(&dir) {
            tracing::debug!(
                app = app.get(),
                ?slot,
                dir = %dir.display(),
                "no artwork resolved, yet this app has sha1 subdirectories; \
                 the appinfo index may no longer cover the nested layout"
            );
        }
        None
    }
}

/// Join an untrusted relative path onto a directory, refusing anything that could escape it.
///
/// Rejects absolute paths, drive prefixes and any `..` component. Steam's own values are always
/// plain `name.jpg` or `<sha1>/name.jpg`, so nothing legitimate is lost — but this value comes
/// out of a binary file we do not parse defensively enough to trust, and the `asset:` protocol
/// scope covers this whole tree.
fn safe_join(dir: &Path, relative: &str) -> Option<PathBuf> {
    if relative.is_empty() {
        return None;
    }
    let rel = Path::new(relative);
    for component in rel.components() {
        match component {
            Component::Normal(_) => {}
            // `..`, `/`, `C:` and `.` all mean this is not the simple relative name Steam
            // writes, so refuse rather than guess at an intent.
            _ => return None,
        }
    }
    Some(dir.join(rel))
}

/// Whether this app's cache directory contains any subdirectory. Only used for the tripwire.
fn has_subdirectory(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.file_type().is_ok_and(|t| t.is_dir()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    fn write(path: &Path, body: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap(); // boundary-ok: test fixture in a tempdir
        }
        std::fs::write(path, body).unwrap(); // boundary-ok: test fixture in a tempdir
    }

    /// A cache rooted in a tempdir, with no appinfo behind it.
    fn cache_at(root: &Path) -> LibraryCache<'static> {
        LibraryCache {
            root: root.to_path_buf(),
            types: None,
            language: DEFAULT_LANGUAGE.to_owned(),
        }
    }

    #[test]
    fn the_flat_layout_resolves_through_the_basename_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let art = tmp.path().join("620").join("library_600x900.jpg");
        write(&art, b"jpeg");

        let found = cache_at(tmp.path()).resolve(AppId::new(620), AssetType::Capsule);
        assert_eq!(found.as_deref(), Some(art.as_path()));
    }

    #[test]
    fn a_missing_app_resolves_to_none_rather_than_a_path_that_does_not_exist() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            cache_at(tmp.path()).resolve(AppId::new(999), AssetType::Capsule),
            None
        );
    }

    #[test]
    fn the_wide_slot_falls_back_from_library_header_to_the_store_header() {
        let tmp = tempfile::tempdir().unwrap();
        let header = tmp.path().join("440").join("header.jpg");
        write(&header, b"jpeg");

        // Premise: the preferred slot really is absent, so this exercises the fallback.
        assert!(
            !tmp.path().join("440").join("library_header.jpg").exists(),
            "premise: no library_header for this app",
        );
        assert_eq!(
            cache_at(tmp.path())
                .resolve(AppId::new(440), AssetType::Header)
                .as_deref(),
            Some(header.as_path()),
        );

        // Control: when the preferred slot *is* present it wins, so the ordering is real and
        // not just "whichever file happens to exist".
        let preferred = tmp.path().join("440").join("library_header.jpg");
        write(&preferred, b"jpeg");
        assert_eq!(
            cache_at(tmp.path())
                .resolve(AppId::new(440), AssetType::Header)
                .as_deref(),
            Some(preferred.as_path()),
        );
    }

    #[test]
    fn safe_join_refuses_anything_that_could_leave_the_app_directory() {
        let dir = Path::new("C:\\Steam\\appcache\\librarycache\\620");

        // The escapes.
        for bad in [
            "../../../../windows/system32/evil.jpg",
            "..",
            "sub/../../escape.jpg",
            "/etc/passwd",
            "C:\\windows\\system32\\evil.jpg",
            "\\\\server\\share\\evil.jpg",
            "",
        ] {
            assert_eq!(safe_join(dir, bad), None, "{bad:?} must be refused");
        }

        // The controls, without which the test would pass on a function that refuses
        // everything — including the two shapes Steam actually writes.
        assert_eq!(
            safe_join(dir, "library_600x900.jpg"),
            Some(dir.join("library_600x900.jpg")),
        );
        assert_eq!(
            safe_join(
                dir,
                "93637c34351160eaa7d7ff0cce69cb4312abb819/library_capsule.jpg"
            ),
            Some(
                dir.join("93637c34351160eaa7d7ff0cce69cb4312abb819")
                    .join("library_capsule.jpg")
            ),
        );
    }

    #[test]
    fn a_slot_with_no_fallback_basenames_cannot_be_guessed_positionally() {
        // The icon is content-hashed, so without `common/icon` there is nothing to match on.
        // Two sha1-named files in the directory: picking either would be a coin flip, and the
        // resolver must decline rather than return the first one it happens to read.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("620");
        write(
            &dir.join("25a5a16b2423bf7487ac5340b5b0948cef48c5f8.jpg"),
            b"a",
        );
        write(
            &dir.join("f568912870a4684f9ec76277a1a404dda6bab213.jpg"),
            b"b",
        );

        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            2,
            "premise: two indistinguishable candidates exist",
        );
        assert_eq!(
            cache_at(tmp.path()).resolve(AppId::new(620), AssetType::Icon),
            None,
        );
    }
}
