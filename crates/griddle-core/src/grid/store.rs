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
use crate::fsutil;
use crate::grid::names::{self, AssetType};
use crate::logo::{LogoPosition, LogoPositionForApp};
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
            let Some(id) = app_id_of(&path) else {
                continue;
            };
            if !known.contains(&id) {
                out.push(path);
            }
        }
        out.sort();
        Ok(out)
    }

    /// Every file [`clear`](Self::clear) would remove for one app, across all editable slots.
    ///
    /// **Exists so a confirmation can quote a number that matches what actually gets deleted.**
    /// The obvious version — summing [`existing`](Self::existing) over the asset types —
    /// under-counts by one whenever a logo has a position sidecar, because that `.json` is not a
    /// sibling of any slot but `clear` takes it anyway. Under-reporting a deletion is precisely
    /// what this project's "name it before it happens" rule exists to prevent, so the count comes
    /// from here and a test pins it to `clear`'s actual behaviour.
    pub fn removable(&self, app: AppId) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for asset in AssetType::EDITABLE {
            out.extend(self.existing(app, asset));
        }
        // Unconditional, matching `clear`: a sidecar can outlive the logo it positioned.
        let json = self.root.join(names::logo_position_file_name(app));
        if json.is_file() {
            out.push(json);
        }
        out
    }

    /// Every appid with at least one file in this directory.
    ///
    /// The enumeration behind "reset everything": the grid directory *is* the record of what has
    /// been customised, so it is asked directly rather than by walking the library and probing
    /// each game in it. That also picks up artwork belonging to apps no longer in the library,
    /// which is right — those files are exactly as stale as the rest.
    ///
    /// **A missing directory is an empty list, not an error.** Steam creates `grid/` only when
    /// the first custom art appears, so "has never customised anything" is an ordinary state, and
    /// it must not surface as a failure on a screen whose whole job is to say *nothing to reset*.
    /// [`orphans`](Self::orphans) errors instead, because it is only reached from a directory
    /// already known to hold artwork.
    pub fn customised_apps(&self) -> Result<Vec<AppId>, Error> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(Error::Read {
                    path: self.root.clone(),
                    source: e,
                });
            }
        };
        // Deduplicated and sorted: one app owns up to six files, and a stable order keeps the
        // count the confirmation dialog quotes reproducible between runs.
        let mut out = std::collections::BTreeSet::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && let Some(id) = app_id_of(&path)
            {
                let _ = out.insert(id);
            }
        }
        Ok(out.into_iter().collect())
    }
}

/// The appid a grid filename belongs to, from its leading digits.
///
/// Shared by [`GridDir::orphans`] and [`GridDir::customised_apps`] so the two cannot disagree
/// about what counts as artwork — one deciding a file belongs to an app while the other does not
/// would mean "reset everything" quietly skipping whatever the other one found.
///
/// Returns `None` for anything not starting with digits, which is what keeps a stray file in
/// `grid/` invisible to both.
fn app_id_of(path: &Path) -> Option<AppId> {
    let stem = path.file_stem()?.to_str()?;
    let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u32>().ok().map(AppId::new)
}

/// Write via a same-directory temp file and rename.
///
/// The temp name keeps the target's extension and appends `.sgdbtmp` — so a half-written capsule
/// is `1234p.png.sgdbtmp`, which sorts beside the real file and is obviously ours if one is ever
/// left behind. The mechanics are in [`crate::fsutil`]; this is only the naming.
fn write_atomic(target: &Path, data: &[u8]) -> Result<(), Error> {
    let tmp = target.with_extension(format!(
        "{}.sgdbtmp",
        target.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    ));

    fsutil::write_atomic(&tmp, target, data).map_err(|e| Error::Write {
        path: e.path,
        source: e.source,
    })
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
