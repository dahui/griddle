//! Reading and writing `userdata/<accountid>/config/grid/`.
//!
//! **This is one of only three modules allowed to write files** (with `steam::shortcuts` and
//! `settings`); CI greps for writes outside them. The directory can hold artwork a user
//! curated by hand and cannot regenerate, so every operation here is deliberate.
//!
//! # The write protocol, and why each step exists
//!
//! 1. **Delete the siblings first.** If `<appid>p.png` and `<appid>p.jpg` both exist, which one
//!    Steam picks is undefined and has changed between versions. SGDBoop does the same.
//! 2. **Write a temp file in the same directory, then rename over the target.** Same-directory
//!    keeps the rename atomic (no cross-volume copy), and it means a crash mid-write leaves the
//!    old art intact rather than a truncated file.
//! 3. **Writing a logo also writes `<appid>.json` when none exists.** A custom logo with no
//!    stored position may not render at all; decky-steamgriddb force-creates
//!    `{BottomLeft, 50, 50}` for exactly this reason. `[VERIFIED-SOURCE]`
//!
//! # What this module does NOT do
//!
//! It never touches `appcache/librarycache/` — that cache is sha1-keyed, Steam owns it, and it
//! re-downloads over anything written there.

use crate::appid::AppId;
use crate::grid::names::{self, AssetType};
use crate::logo::{LogoPosition, LogoPositionForApp};
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("grid directory does not exist: {0}")]
    MissingDir(PathBuf),

    #[error("writing {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("removing {path}: {source}")]
    Remove {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("serializing logo position: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("refusing to write empty image data for {0}")]
    EmptyImage(String),
}

/// A grid directory. Construction does not touch the filesystem; [`GridDir::ensure`] does.
#[derive(Debug, Clone)]
pub struct GridDir {
    root: PathBuf,
}

/// What an apply actually did, for reporting and for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub written: PathBuf,
    /// Sibling files removed to avoid an ambiguous pair.
    pub removed: Vec<PathBuf>,
    /// Set when a default logo position had to be created alongside a logo.
    pub logo_position_created: Option<PathBuf>,
}

impl GridDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        GridDir { root: root.into() }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    /// Create the directory if absent. Steam creates it on first custom art, so a clean
    /// install legitimately has none.
    pub fn ensure(&self) -> Result<(), Error> {
        std::fs::create_dir_all(&self.root).map_err(|e| Error::Write {
            path: self.root.clone(),
            source: e,
        })
    }

    /// Existing files for an asset, in the sibling order from [`names::siblings`].
    ///
    /// More than one means the directory is already ambiguous — Steam's choice is undefined.
    pub fn existing(&self, app: AppId, asset: AssetType) -> Vec<PathBuf> {
        names::siblings(app, asset)
            .into_iter()
            .map(|n| self.root.join(n))
            .filter(|p| p.is_file())
            .collect()
    }

    /// Write artwork, replacing whatever was there.
    ///
    /// `ext` is the extension to write, which is **not** necessarily the true container
    /// format: animated WebP bytes are written as `png` on purpose, because Chromium sniffs
    /// content and that is exactly what Steam's own code does.
    pub fn apply(
        &self,
        app: AppId,
        asset: AssetType,
        ext: &str,
        data: &[u8],
    ) -> Result<Applied, Error> {
        if data.is_empty() {
            return Err(Error::EmptyImage(names::file_name(app, asset, ext)));
        }
        if !self.root.is_dir() {
            return Err(Error::MissingDir(self.root.clone()));
        }

        let target = self.root.join(names::file_name(app, asset, ext));

        // Step 1: remove every sibling, including one with the same name as the target — the
        // rename replaces it anyway, and removing first keeps the "exactly one file" invariant
        // true even if the rename fails.
        let mut removed = Vec::new();
        for sibling in self.existing(app, asset) {
            if sibling == target {
                continue;
            }
            std::fs::remove_file(&sibling).map_err(|e| Error::Remove {
                path: sibling.clone(),
                source: e,
            })?;
            removed.push(sibling);
        }

        // Step 2: temp + atomic rename, in the same directory.
        write_atomic(&target, data)?;

        // Step 3: a logo without a position may not render.
        let logo_position_created = if asset == AssetType::Logo {
            let json = self.root.join(names::logo_position_file_name(app));
            if json.exists() {
                None
            } else {
                self.write_logo_position(app, LogoPosition::default())?;
                Some(json)
            }
        } else {
            None
        };

        Ok(Applied {
            written: target,
            removed,
            logo_position_created,
        })
    }

    /// Remove all files for an asset. Returns what was deleted.
    ///
    /// Clearing a **logo** also clears its position sidecar; clearing anything else leaves the
    /// sidecar alone, because the Header asset shares its base name.
    pub fn clear(&self, app: AppId, asset: AssetType) -> Result<Vec<PathBuf>, Error> {
        let mut removed = Vec::new();
        for p in self.existing(app, asset) {
            std::fs::remove_file(&p).map_err(|e| Error::Remove {
                path: p.clone(),
                source: e,
            })?;
            removed.push(p);
        }
        if asset == AssetType::Logo {
            let json = self.root.join(names::logo_position_file_name(app));
            if json.is_file() {
                std::fs::remove_file(&json).map_err(|e| Error::Remove {
                    path: json.clone(),
                    source: e,
                })?;
                removed.push(json);
            }
        }
        Ok(removed)
    }

    pub fn read_logo_position(&self, app: AppId) -> Result<Option<LogoPosition>, Error> {
        let p = self.root.join(names::logo_position_file_name(app));
        if !p.is_file() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&p).map_err(|e| Error::Read { path: p, source: e })?;
        // A hand-edited or truncated file degrades to "no custom position" rather than an
        // error — the art is still fine, only the placement is unknown.
        Ok(serde_json::from_str::<LogoPositionForApp>(&raw)
            .ok()
            .map(|w| w.logo_position))
    }

    pub fn write_logo_position(&self, app: AppId, pos: LogoPosition) -> Result<PathBuf, Error> {
        let p = self.root.join(names::logo_position_file_name(app));
        let json = serde_json::to_vec(&pos.for_app())?;
        write_atomic(&p, &json)?;
        Ok(p)
    }

    /// Artwork whose appid no longer corresponds to anything — Steam reassigns shortcut ids,
    /// which strands files. Read-only: deletion is always a separate, confirmed action.
    pub fn orphans(&self, known: &[AppId]) -> Result<Vec<PathBuf>, Error> {
        let mut out = Vec::new();
        let entries = std::fs::read_dir(&self.root).map_err(|e| Error::Read {
            path: self.root.clone(),
            source: e,
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // Strip any known suffix to recover the leading appid.
            let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
            let Ok(id) = digits.parse::<u32>() else {
                continue;
            };
            if !known.contains(&AppId::new(id)) {
                out.push(path);
            }
        }
        out.sort();
        Ok(out)
    }
}

/// Write via a same-directory temp file and rename.
///
/// The rename is also what makes Steam notice the change — SGDBoop relies on the same trick.
fn write_atomic(target: &Path, data: &[u8]) -> Result<(), Error> {
    let tmp = target.with_extension(format!(
        "{}.sgdbtmp",
        target.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    ));

    let mut f = std::fs::File::create(&tmp).map_err(|e| Error::Write {
        path: tmp.clone(),
        source: e,
    })?;
    f.write_all(data).map_err(|e| Error::Write {
        path: tmp.clone(),
        source: e,
    })?;
    // fsync before rename: without it a crash can leave a correctly-named but empty file.
    f.sync_all().map_err(|e| Error::Write {
        path: tmp.clone(),
        source: e,
    })?;
    drop(f);

    std::fs::rename(&tmp, target).map_err(|e| {
        // Best-effort cleanup, but say so if it fails — a stray `.sgdbtmp` in the user's grid
        // folder is confusing, and silently swallowing the reason is what the workspace's
        // `let_underscore_must_use = deny` exists to prevent.
        if let Err(cleanup) = std::fs::remove_file(&tmp) {
            tracing::warn!(
                temp = %tmp.display(),
                error = %cleanup,
                "could not remove temp file after a failed rename",
            );
        }
        Error::Write {
            path: target.to_path_buf(),
            source: e,
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;
    use crate::logo::PinnedPosition;

    fn dir() -> (tempfile::TempDir, GridDir) {
        let t = tempfile::tempdir().unwrap();
        let g = GridDir::new(t.path());
        g.ensure().unwrap();
        (t, g)
    }

    const APP: AppId = AppId::new(4_048_848_997);

    #[test]
    fn writes_the_expected_filename() {
        let (_t, g) = dir();
        let r = g.apply(APP, AssetType::Capsule, "png", b"data").unwrap();
        assert!(r.written.ends_with("4048848997p.png"));
        assert_eq!(std::fs::read(&r.written).unwrap(), b"data");
        assert!(r.removed.is_empty());
    }

    /// The core safety property: exactly one file per asset, never an ambiguous pair.
    #[test]
    fn replacing_a_jpg_with_a_png_removes_the_jpg() {
        let (_t, g) = dir();
        g.apply(APP, AssetType::Capsule, "jpg", b"old").unwrap();
        assert_eq!(g.existing(APP, AssetType::Capsule).len(), 1);

        let r = g.apply(APP, AssetType::Capsule, "png", b"new").unwrap();
        assert_eq!(r.removed.len(), 1, "the .jpg must be removed");
        assert!(r.removed[0].ends_with("4048848997p.jpg"));

        let remaining = g.existing(APP, AssetType::Capsule);
        assert_eq!(remaining.len(), 1, "exactly one file may remain");
        assert!(remaining[0].ends_with("4048848997p.png"));
    }

    #[test]
    fn cleans_up_a_pre_existing_ambiguous_pair() {
        let (t, g) = dir();
        // Simulate a directory another tool left in a bad state.
        std::fs::write(t.path().join("4048848997p.png"), b"a").unwrap();
        std::fs::write(t.path().join("4048848997p.jpg"), b"b").unwrap();
        std::fs::write(t.path().join("4048848997p.jpeg"), b"c").unwrap();
        assert_eq!(g.existing(APP, AssetType::Capsule).len(), 3);

        let r = g.apply(APP, AssetType::Capsule, "png", b"new").unwrap();
        assert_eq!(r.removed.len(), 2, "the two non-target siblings go");
        assert_eq!(g.existing(APP, AssetType::Capsule).len(), 1);
        assert_eq!(std::fs::read(&r.written).unwrap(), b"new");
    }

    #[test]
    fn assets_do_not_disturb_each_other() {
        let (_t, g) = dir();
        g.apply(APP, AssetType::Capsule, "png", b"cap").unwrap();
        g.apply(APP, AssetType::Hero, "jpg", b"hero").unwrap();
        g.apply(APP, AssetType::Header, "png", b"head").unwrap();

        assert_eq!(g.existing(APP, AssetType::Capsule).len(), 1);
        assert_eq!(g.existing(APP, AssetType::Hero).len(), 1);
        assert_eq!(g.existing(APP, AssetType::Header).len(), 1);
    }

    #[test]
    fn applying_a_logo_creates_a_default_position() {
        let (_t, g) = dir();
        assert_eq!(g.read_logo_position(APP).unwrap(), None);

        let r = g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
        assert!(
            r.logo_position_created.is_some(),
            "a logo without a position may not render"
        );

        let pos = g.read_logo_position(APP).unwrap().unwrap();
        assert_eq!(pos.pinned_position, PinnedPosition::BottomLeft);
        assert_eq!(pos.width_pct, 50.0);
        assert_eq!(pos.height_pct, 50.0);
    }

    #[test]
    fn applying_a_logo_preserves_an_existing_position() {
        let (_t, g) = dir();
        let custom = LogoPosition {
            pinned_position: PinnedPosition::CenterCenter,
            width_pct: 33.0,
            height_pct: 44.0,
        };
        g.write_logo_position(APP, custom).unwrap();

        let r = g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
        assert!(
            r.logo_position_created.is_none(),
            "must not overwrite the user's placement"
        );
        assert_eq!(g.read_logo_position(APP).unwrap().unwrap(), custom);
    }

    #[test]
    fn clearing_a_logo_also_clears_its_position() {
        let (_t, g) = dir();
        g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
        let removed = g.clear(APP, AssetType::Logo).unwrap();
        assert_eq!(removed.len(), 2, "the .png and the .json");
        assert_eq!(g.read_logo_position(APP).unwrap(), None);
    }

    /// The Header asset shares a base name with the sidecar; clearing it must not take the
    /// logo's placement with it.
    #[test]
    fn clearing_the_header_leaves_the_logo_position_alone() {
        let (_t, g) = dir();
        g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
        g.apply(APP, AssetType::Header, "png", b"header").unwrap();

        let removed = g.clear(APP, AssetType::Header).unwrap();
        assert_eq!(removed.len(), 1);
        assert!(
            g.read_logo_position(APP).unwrap().is_some(),
            "the .json must survive"
        );
    }

    #[test]
    fn clearing_an_absent_asset_is_not_an_error() {
        let (_t, g) = dir();
        assert_eq!(
            g.clear(APP, AssetType::Hero).unwrap(),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn refuses_empty_image_data() {
        let (_t, g) = dir();
        assert!(matches!(
            g.apply(APP, AssetType::Capsule, "png", b""),
            Err(Error::EmptyImage(_))
        ));
    }

    #[test]
    fn refuses_to_write_into_a_missing_directory() {
        let t = tempfile::tempdir().unwrap();
        let g = GridDir::new(t.path().join("does-not-exist"));
        assert!(matches!(
            g.apply(APP, AssetType::Capsule, "png", b"x"),
            Err(Error::MissingDir(_))
        ));
    }

    #[test]
    fn no_temp_files_survive_a_successful_write() {
        let (t, g) = dir();
        g.apply(APP, AssetType::Capsule, "png", b"data").unwrap();
        g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(t.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("sgdbtmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn a_corrupt_position_file_degrades_to_none() {
        let (t, g) = dir();
        std::fs::write(t.path().join("4048848997.json"), b"{ not json").unwrap();
        assert_eq!(g.read_logo_position(APP).unwrap(), None);
    }

    #[test]
    fn orphans_finds_art_for_unknown_appids_only() {
        let (_t, g) = dir();
        g.apply(APP, AssetType::Capsule, "png", b"keep").unwrap();
        g.apply(AppId::new(999_999), AssetType::Hero, "png", b"stale")
            .unwrap();

        let orphans = g.orphans(&[APP]).unwrap();
        assert_eq!(orphans.len(), 1);
        assert!(orphans[0].ends_with("999999_hero.png"));
    }

    /// `Path::ends_with` matches whole components, so compare file names explicitly — a
    /// partial suffix like `"_icon.ico"` silently never matches.
    fn name_of(p: &Path) -> &str {
        p.file_name().and_then(|s| s.to_str()).unwrap_or("")
    }

    #[test]
    fn icons_consider_the_ico_extension() {
        let (_t, g) = dir();
        g.apply(APP, AssetType::Icon, "ico", b"icon").unwrap();
        assert_eq!(g.existing(APP, AssetType::Icon).len(), 1);
        // Replacing with a .png must remove the .ico.
        let r = g.apply(APP, AssetType::Icon, "png", b"icon2").unwrap();
        assert_eq!(r.removed.len(), 1);
        assert_eq!(name_of(&r.removed[0]), "4048848997_icon.ico");
        assert_eq!(name_of(&r.written), "4048848997_icon.png");
    }
}
