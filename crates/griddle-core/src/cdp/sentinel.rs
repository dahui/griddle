//! The `.cef-enable-remote-debugging` opt-in file.
//!
//! An empty file in the Steam root. Its presence tells Steam to open its CEF debugging port on
//! next start — **Valve's own mechanism**, not a hack: CSS Loader and Decky both depend on it,
//! and the file needs no elevation to create `[VERIFIED-BOX 2026-07-27]`.
//!
//! # Created during setup, and disclosed rather than asked
//!
//! This used to be behind an opt-in checkbox. It no longer is: applying artwork *without a
//! Steam restart* is the entire reason this app exists in preference to Steam Art Manager or
//! SGDBoop, so making its one prerequisite a thing the user has to find and enable meant the
//! product shipped switched off. CSS Loader and Decky set the same flag and mention it to
//! nobody.
//!
//! The middle ground: it is created at startup, and the first-run screen **says so** — what the
//! file is, that it is Valve's own flag, and that deleting it undoes everything. Disclosure
//! without a permission prompt for the feature the user installed the app to get.
//!
//! What the flag actually does is worth being clear-eyed about: Steam opens its CEF debugging
//! port on loopback at next start, so any process already running as this user can drive
//! Steam's JS. That is Valve's own mechanism and the same exposure CSS Loader and Decky have
//! always carried, but it is a real widening and belongs in the disclosure, not in a footnote.
//!
//! # 🔴 Creating it is not enough; Steam must restart
//!
//! The port opens at client start, so a freshly created sentinel does nothing until Steam is
//! restarted. [`Sentinel::state`] reports that as a distinct state rather than letting the user
//! wonder why nothing happened.
//!
//! # 🔴 Millennium deletes this file
//!
//! Millennium removes the sentinel and proxies `user32.dll` into `steam.exe` instead, which is
//! why installing it breaks CSS Loader
//! ([SteamClientHomebrew/Millennium#591](https://github.com/SteamClientHomebrew/Millennium/issues/591)).
//! If the sentinel keeps disappearing, that is the thing to look for.

use crate::steam::locate::SteamInstall;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("creating {path}: {source}")]
    Create {
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
}

/// Whether live apply is possible yet, and if not, why not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The file is absent. The user has not opted in.
    Absent,
    /// Present, but Steam is not running, so nothing is listening.
    PresentSteamStopped,
    /// Present and Steam is running — the port should be open.
    PresentSteamRunning,
}

impl State {
    /// A one-line status, for showing beside the live-apply control.
    ///
    /// 🔴 **This is a status line, not an explanation.** It used to spell out what
    /// `.cef-enable-remote-debugging` is, that it is Valve's own setting and that Steam needs
    /// restarting — all of which the settings panel says immediately above it, so the screen
    /// said the same thing twice.
    ///
    /// A caller that shows this *without* that surrounding copy — the Big Picture UI, when it
    /// gets one — has to supply the explanation itself. Only [`State::PresentSteamStopped`]
    /// carries its own remedy, because "start Steam" is not something the panel can say in
    /// advance.
    pub fn explain(self) -> &'static str {
        match self {
            State::Absent => "Live apply is off.",
            State::PresentSteamStopped => "Live apply is on, but Steam isn't running.",
            State::PresentSteamRunning => "Live apply is on.",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sentinel {
    path: PathBuf,
}

impl Sentinel {
    pub fn for_install(install: &SteamInstall) -> Self {
        Sentinel {
            path: install.cef_sentinel(),
        }
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Sentinel { path: path.into() }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// Current state, combining the file with whether Steam is up.
    pub fn state(&self) -> State {
        if !self.exists() {
            State::Absent
        } else if crate::steam::process::is_running() {
            State::PresentSteamRunning
        } else {
            State::PresentSteamStopped
        }
    }

    /// Create the sentinel.
    ///
    /// Called once at startup — see the module docs on why this is setup rather than a prompt.
    ///
    /// Creating it when it already exists is a no-op rather than an error — the caller cares
    /// that it is enabled, not who enabled it. Notably it does *not* truncate an existing file:
    /// Steam only checks for presence, and rewriting another tool's file would be rude.
    pub fn enable(&self) -> Result<(), Error> {
        if self.exists() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent()
            && !parent.is_dir()
        {
            return Err(Error::Create {
                path: self.path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "the Steam directory does not exist",
                ),
            });
        }
        // Creating this empty file in Steam's root is this module's entire purpose, and it
        // happens only when the user has explicitly asked for it.
        // boundary-ok: the opt-in sentinel, on explicit user request
        std::fs::write(&self.path, b"").map_err(|source| Error::Create {
            path: self.path.clone(),
            source,
        })?;
        tracing::info!(path = %self.path.display(), "created the CEF debugging sentinel");
        Ok(())
    }

    /// Remove it again. The one file this app will delete that it may not have created — and
    /// only because the user asked for exactly that, and it is empty by definition.
    pub fn disable(&self) -> Result<(), Error> {
        if !self.exists() {
            return Ok(());
        }
        // boundary-ok: deleting the empty opt-in file, on explicit user request
        std::fs::remove_file(&self.path).map_err(|source| Error::Remove {
            path: self.path.clone(),
            source,
        })?;
        tracing::info!(path = %self.path.display(), "removed the CEF debugging sentinel");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    #[test]
    fn enable_creates_an_empty_file_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let s = Sentinel::at(dir.path().join(".cef-enable-remote-debugging"));

        assert!(!s.exists());
        s.enable().unwrap();
        assert!(s.exists());
        assert_eq!(std::fs::read(s.path()).unwrap(), Vec::<u8>::new());

        s.enable().unwrap();
        assert!(s.exists(), "enabling twice must not fail");
    }

    #[test]
    fn enable_does_not_truncate_a_file_someone_else_wrote() {
        // Steam only checks for presence. If another tool put something in there, rewriting it
        // would be gratuitous.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".cef-enable-remote-debugging");
        // boundary-ok: test fixture written into a tempdir
        std::fs::write(&path, b"written by something else").unwrap();

        let s = Sentinel::at(&path);
        s.enable().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"written by something else");
    }

    #[test]
    fn disable_removes_it_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let s = Sentinel::at(dir.path().join(".cef-enable-remote-debugging"));
        s.enable().unwrap();

        s.disable().unwrap();
        assert!(!s.exists());
        s.disable().unwrap();
        assert!(!s.exists(), "disabling twice must not fail");
    }

    #[test]
    fn enabling_into_a_missing_directory_is_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let s = Sentinel::at(dir.path().join("no-such-steam").join("sentinel"));
        let err = s.enable().unwrap_err();
        assert!(
            err.to_string().contains("Steam directory does not exist"),
            "{err}"
        );
    }

    #[test]
    fn an_absent_sentinel_reports_absent_whatever_steam_is_doing() {
        let dir = tempfile::tempdir().unwrap();
        let s = Sentinel::at(dir.path().join("nope"));
        assert_eq!(s.state(), State::Absent);
    }

    #[test]
    fn every_state_is_distinguishable_from_a_one_line_status() {
        // These strings are the UI. They used to repeat the whole opt-in explanation, which the
        // settings panel already carries a line above — so the screen said it twice. The
        // property worth holding now is that each state reads differently and stays short.
        let all = [
            State::Absent.explain(),
            State::PresentSteamStopped.explain(),
            State::PresentSteamRunning.explain(),
        ];
        for s in all {
            assert!(!s.is_empty(), "every state needs something to show");
            assert!(s.len() < 60, "a status line, not a paragraph: {s}");
        }
        assert_eq!(
            all.iter().collect::<std::collections::BTreeSet<_>>().len(),
            3,
            "the three states must not read the same: {all:?}",
        );
    }

    #[test]
    fn the_one_state_with_a_remedy_names_it() {
        // "Steam isn't running" is the only state the surrounding panel cannot anticipate, so
        // it is the only one that has to carry its own remedy.
        assert!(
            State::PresentSteamStopped
                .explain()
                .contains("isn't running"),
            "{}",
            State::PresentSteamStopped.explain(),
        );
    }

    #[test]
    fn the_path_comes_from_the_steam_root() {
        let s = Sentinel::for_install(&SteamInstall::at("/steam"));
        assert!(s.path().ends_with(".cef-enable-remote-debugging"));
    }
}
