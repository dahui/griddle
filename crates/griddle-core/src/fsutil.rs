//! The atomic write, in one place.
//!
//! Three modules write files, and all three did the same dance: create a temp file beside the
//! target, write, `fsync`, rename over it, and clean up the temp file if the rename failed. Three
//! copies is three chances for one of them to lose the `fsync` — which is invisible until a crash
//! at exactly the wrong moment leaves a correctly-named empty file where the user's settings or
//! artwork used to be.
//!
//! # Why this may write files
//!
//! `scripts/check-boundaries.sh` fails the build on a file write outside the sanctioned modules,
//! and this module is on that list. It is not a fourth writer: it has no idea what a grid
//! directory or a settings file is, and every path it touches is one a caller already chose. The
//! boundary exists to keep writes to the user's irreplaceable Steam config auditable, and moving
//! the mechanics here leaves that audit exactly where it was — at the three call sites.
//!
//! # Why the error type is its own
//!
//! Each caller has its own `Error` with its own `Write { path, source }` variant, and each maps
//! [`WriteError`] into it in one line. A shared error enum would have to name every module's
//! failures, and the one thing the boundary check must be able to see is *which* module wrote.

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// A failed write, carrying the path that actually failed rather than the one asked for.
///
/// The distinction matters when reporting: a failure creating the temp file names the temp file,
/// a failure renaming names the target. Collapsing them would tell the user their settings could
/// not be written when the truth is the directory is not writable.
#[derive(Debug)]
pub struct WriteError {
    pub path: PathBuf,
    pub source: std::io::Error,
}

/// Write `data` to `tmp`, flush it to disk, then rename over `target`.
///
/// `tmp` **must be in the same directory as `target`**, or the rename becomes a cross-volume copy
/// and stops being atomic. Callers pass it explicitly rather than having it derived, because the
/// three of them name their temp files differently and the name ends up visible to the user when
/// something goes wrong.
///
/// The `fsync` is what makes the rename meaningful: without it the metadata operation can land
/// while the contents are still in flight. The rename is also what makes Steam notice a new piece
/// of artwork — SGDBoop relies on the same behaviour.
pub fn write_atomic(tmp: &Path, target: &Path, data: &[u8]) -> Result<(), WriteError> {
    let at_tmp = |source| WriteError {
        path: tmp.to_path_buf(),
        source,
    };

    {
        let mut f = std::fs::File::create(tmp).map_err(at_tmp)?;
        f.write_all(data).map_err(at_tmp)?;
        f.sync_all().map_err(at_tmp)?;
    }

    std::fs::rename(tmp, target).map_err(|source| {
        // Best-effort, but never silent: a stray temp file in the user's grid folder is
        // confusing, and swallowing the reason is what `let_underscore_must_use = deny` exists
        // to prevent.
        if let Err(cleanup) = std::fs::remove_file(tmp) {
            tracing::warn!(
                temp = %tmp.display(),
                error = %cleanup,
                "could not remove temp file after a failed rename",
            );
        }
        WriteError {
            path: target.to_path_buf(),
            source,
        }
    })
}

/// `foo/shortcuts.vdf` + `.sgdb-orig` -> `foo/shortcuts.vdf.sgdb-orig`.
///
/// Deliberately not `Path::with_extension`, which would replace `.vdf` and produce
/// `shortcuts.sgdb-orig` — a name that no longer says what the file is.
pub fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suffix_is_appended_rather_than_replacing_the_extension() {
        let p = sibling_with_suffix(Path::new("/steam/config/shortcuts.vdf"), ".sgdb-orig");
        assert_eq!(p.file_name().unwrap(), "shortcuts.vdf.sgdb-orig");

        // The control, and the reason this is not `with_extension`: that would have produced
        // `shortcuts.sgdb-orig`, which no longer says what the file is a backup of.
        assert_ne!(p.file_name().unwrap(), "shortcuts.sgdb-orig");
    }

    #[test]
    fn a_write_replaces_the_target_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("thing.json");
        let tmp = sibling_with_suffix(&target, ".tmp");
        std::fs::write(&target, b"old").unwrap();

        write_atomic(&tmp, &target, b"new").unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(
            !tmp.exists(),
            "the temp file must not survive a successful write"
        );
    }

    #[test]
    fn a_failure_names_the_path_that_actually_failed() {
        let dir = tempfile::tempdir().unwrap();
        // A temp path inside a directory that does not exist, so `File::create` fails first.
        let tmp = dir.path().join("nope").join("x.tmp");
        let target = dir.path().join("x");

        let err = write_atomic(&tmp, &target, b"data").unwrap_err();
        assert_eq!(
            err.path, tmp,
            "the failure was creating the temp file, not renaming"
        );
        assert!(
            !target.exists(),
            "a failed write must not have touched the target"
        );
    }
}
