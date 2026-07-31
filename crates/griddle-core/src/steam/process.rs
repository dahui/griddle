//! Whether Steam is running, and stopping it when it must be stopped.
//!
//! # Why this module exists at all
//!
//! Steam holds `shortcuts.vdf` in memory and rewrites it from memory on exit. A write while it
//! is running is **silently discarded** — no error, no warning, and the damage only becomes
//! visible after the next restart, long after the code that caused it ran. `[VERIFIED-SOURCE]`
//!
//! So this module is the sole minter of [`SteamStopped`], and [`crate::steam::shortcuts`] will
//! not write without one. The token cannot be constructed anywhere else: its field is private
//! and there is no public constructor. Forgetting the check is a compile error, not a bug
//! report.
//!
//! # 🔴 The registry pid is not the signal
//!
//! `HKCU\...\ActiveProcess\pid` goes to **0 early in shutdown, while `steam.exe` is still
//! alive** — observed directly during S9. `[VERIFIED-BOX 2026-07-27]` Trusting it would hand
//! out a token during the exact window in which Steam is still holding the file and still
//! going to rewrite it on the way out. That is the worst possible moment to write.
//!
//! We therefore wait on the **processes**: `steam.exe` and every `steamwebhelper.exe`. The
//! helpers outlive the main process briefly, so both are checked.
//!
//! # A token proves the past, not the present
//!
//! [`SteamStopped`] records an observation that has already happened. The user can start Steam
//! from the taskbar a second later and the token becomes a lie. The type system cannot see
//! that, so it is not asked to: the writer calls [`SteamStopped::reconfirm`] immediately before
//! it writes. The type prevents *forgetting to check*; the reconfirm prevents *checking too
//! long ago*. Neither is sufficient alone, which is why both exist.

use crate::steam::locate::SteamInstall;
use std::time::{Duration, Instant};

/// Process names that must all be gone before `shortcuts.vdf` is safe to write.
const STEAM_PROCESSES: [&str; 2] = ["steam.exe", "steamwebhelper.exe"];

/// How long to wait for a shutdown before giving up. Steam takes a few seconds normally; a
/// game still closing, or a "quit anyway?" prompt, can make it much longer.
pub const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(45);

/// How often to re-check the process list while waiting. Frequent enough to feel instant,
/// infrequent enough to cost nothing. Deliberately a poll and not a fixed sleep — a fixed
/// sleep is either too short (races) or too slow (annoying), and is never right.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Steam is up and we did not ask it to stop. The user's move.
    #[error("Steam is running ({count} process(es): {names}). Close Steam and try again.")]
    StillRunning { count: usize, names: String },

    /// We asked Steam to exit and it did not, within the timeout. Distinct from
    /// [`Error::StillRunning`] because the remedy differs — a game may still be closing, or
    /// Steam may be showing a confirmation prompt that needs a human.
    #[error(
        "Steam did not exit within {waited:?} ({count} process(es) left: {names}). \
         It may be waiting on a prompt or on a game still closing."
    )]
    ShutdownTimedOut {
        waited: Duration,
        count: usize,
        names: String,
    },

    #[error("Steam restarted after it was checked — the write was not attempted")]
    Restarted,

    #[error("could not run {path}: {source}")]
    Spawn {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("steam.exe not found at {0}")]
    SteamExeMissing(String),

    #[error("managing the Steam process is only implemented on Windows")]
    UnsupportedPlatform,
}

/// A running Steam-related process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamProcess {
    pub pid: u32,
    pub name: String,
}

/// Proof that Steam was observed to be fully stopped.
///
/// Only [`verify_stopped`] and [`shutdown`] can produce one. See the module docs for why this
/// is a token rather than a boolean, and why it is re-checked before use anyway.
#[derive(Debug)]
pub struct SteamStopped {
    observed_at: Instant,
    /// A test-only token skips the re-check. `#[cfg(test)]` does not apply to dependent
    /// crates, so this cannot exist in a build of the real application — the guarantee holds
    /// where it matters while unit tests still run on a developer machine with Steam open.
    #[cfg(test)]
    synthetic: bool,
}

impl SteamStopped {
    /// How long ago the observation was made. Surfaced in diagnostics.
    pub fn age(&self) -> Duration {
        self.observed_at.elapsed()
    }

    /// Re-check that Steam is *still* stopped, immediately before acting on the token.
    ///
    /// See the module docs: the token is evidence of a past observation, and the user is free
    /// to relaunch Steam in the meantime.
    pub fn reconfirm(&self) -> Result<(), Error> {
        #[cfg(test)]
        if self.synthetic {
            return Ok(());
        }
        if running().is_empty() {
            Ok(())
        } else {
            Err(Error::Restarted)
        }
    }

    #[cfg(test)]
    pub(crate) fn synthetic_for_test() -> Self {
        SteamStopped {
            observed_at: Instant::now(),
            synthetic: true,
        }
    }

    fn observed() -> Self {
        SteamStopped {
            observed_at: Instant::now(),
            #[cfg(test)]
            synthetic: false,
        }
    }
}

/// Every running `steam.exe` / `steamwebhelper.exe`.
pub fn running() -> Vec<SteamProcess> {
    imp::enumerate()
        .into_iter()
        .filter(|p| {
            STEAM_PROCESSES
                .iter()
                .any(|want| p.name.eq_ignore_ascii_case(want))
        })
        .collect()
}

/// True if any Steam process is alive.
pub fn is_running() -> bool {
    !running().is_empty()
}

/// Mint a token if Steam is already stopped, without trying to stop it.
///
/// This is the path for "the user closed Steam themselves" — no reason to spawn anything.
pub fn verify_stopped() -> Result<SteamStopped, Error> {
    let procs = running();
    if procs.is_empty() {
        return Ok(SteamStopped::observed());
    }
    let (count, names) = describe(&procs);
    Err(Error::StillRunning { count, names })
}

/// Ask Steam to exit, then wait for every Steam process to actually be gone.
///
/// Returns immediately with a token if Steam was not running to begin with.
pub fn shutdown(install: &SteamInstall, timeout: Duration) -> Result<SteamStopped, Error> {
    if !is_running() {
        return Ok(SteamStopped::observed());
    }

    let exe = install.steam_exe();
    if !exe.is_file() {
        return Err(Error::SteamExeMissing(exe.display().to_string()));
    }

    // `steam.exe -shutdown` signals the running client and exits almost immediately. We do
    // not wait on it: if it ever failed to exit we would hang here forever, and its exit
    // status tells us nothing anyway. The process list is the thing that actually answers the
    // question, so poll that instead.
    imp::spawn_detached(&exe, &["-shutdown"])?;

    wait_until_stopped(timeout)
}

/// Poll until no Steam process remains, or the timeout expires.
pub fn wait_until_stopped(timeout: Duration) -> Result<SteamStopped, Error> {
    let started = Instant::now();
    loop {
        let procs = running();
        if procs.is_empty() {
            tracing::info!(waited = ?started.elapsed(), "Steam is fully stopped");
            return Ok(SteamStopped::observed());
        }
        if started.elapsed() >= timeout {
            let (count, names) = describe(&procs);
            return Err(Error::ShutdownTimedOut {
                waited: started.elapsed(),
                count,
                names,
            });
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Start Steam again after a [`shutdown`].
pub fn launch(install: &SteamInstall) -> Result<(), Error> {
    let exe = install.steam_exe();
    if !exe.is_file() {
        return Err(Error::SteamExeMissing(exe.display().to_string()));
    }
    imp::spawn_detached(&exe, &[])
}

/// Wait for Steam to finish coming up: the registry pid is set and the helpers have spawned.
///
/// The pid alone is too early — it appears well before `SharedJSContext` exists — so a helper
/// count is required too. Returns `false` on timeout rather than erroring: a slow start is not
/// a failure, it just means the caller should not assume readiness.
pub fn wait_until_running(timeout: Duration, min_helpers: usize) -> bool {
    let started = Instant::now();
    loop {
        let procs = running();
        let helpers = procs
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case("steamwebhelper.exe"))
            .count();
        let main = procs
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("steam.exe"));
        if main && helpers >= min_helpers {
            return true;
        }
        if started.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Render a process list for an error message. Sorted so the text is stable between runs —
/// an error that reorders itself looks like a different error in a bug report.
fn describe(procs: &[SteamProcess]) -> (usize, String) {
    let mut names: Vec<String> = procs
        .iter()
        .map(|p| format!("{} (pid {})", p.name, p.pid))
        .collect();
    names.sort();
    (procs.len(), names.join(", "))
}

#[cfg(windows)]
mod imp {
    use super::{Error, SteamProcess};
    use std::path::Path;

    /// Do not create a console window for a child process. This app exists partly because the
    /// Decky workaround flashes a console on every boot; re-introducing one here would be
    /// embarrassing.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn spawn_detached(exe: &Path, args: &[&str]) -> Result<(), Error> {
        use std::os::windows::process::CommandExt as _;
        std::process::Command::new(exe)
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_child| ())
            .map_err(|e| Error::Spawn {
                path: exe.display().to_string(),
                source: e,
            })
    }

    /// Snapshot every process on the system via ToolHelp.
    ///
    /// Returns an empty list on failure rather than erroring. That is the safe direction only
    /// because callers treat "no Steam processes" as a reason to *proceed*, so it must not be
    /// reachable by accident — `CreateToolhelp32Snapshot` failing is a broken system, and the
    /// caller re-checks anyway. An empty snapshot is logged loudly for exactly that reason.
    #[allow(
        unsafe_code,
        reason = "process enumeration has no safe std equivalent; the unsafe is confined to \
                  this function and every raw pointer is created and consumed within it"
    )]
    pub fn enumerate() -> Vec<SteamProcess> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        };

        let mut out = Vec::new();
        // SAFETY: TH32CS_SNAPPROCESS with pid 0 snapshots all processes. The returned handle
        // is checked against INVALID_HANDLE_VALUE and closed on every path out.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                tracing::warn!("CreateToolhelp32Snapshot failed; cannot tell if Steam is running");
                return out;
            }

            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;

            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    let name = &entry.szExeFile;
                    let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
                    out.push(SteamProcess {
                        pid: entry.th32ProcessID,
                        name: String::from_utf16_lossy(&name[..len]),
                    });
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }

            CloseHandle(snapshot);
        }
        out
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Error, SteamProcess};
    use std::path::Path;

    /// Non-Windows exists so `griddle-core` compiles and its pure tests run on the Linux CI leg.
    /// There is no Steam client to find, so the list is empty — which is the truth there.
    pub fn enumerate() -> Vec<SteamProcess> {
        Vec::new()
    }

    pub fn spawn_detached(_exe: &Path, _args: &[&str]) -> Result<(), Error> {
        Err(Error::UnsupportedPlatform)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    #[test]
    fn enumeration_finds_this_process() {
        // A weak assertion on purpose: the point is that the ToolHelp call returns something
        // rather than silently yielding an empty list, which would make `verify_stopped`
        // hand out tokens on a machine where Steam is running.
        let all = imp::enumerate();
        if cfg!(windows) {
            assert!(!all.is_empty(), "process enumeration returned nothing");
            let me = std::process::id();
            assert!(
                all.iter().any(|p| p.pid == me),
                "our own pid {me} was not in the snapshot"
            );
        }
    }

    #[test]
    fn only_steam_processes_are_matched() {
        // `running()` filters by exact name, so a process merely containing "steam" must not
        // count. Checked against the real snapshot: whatever is running, nothing named e.g.
        // "steamfriends.exe" should be reported.
        for p in running() {
            assert!(
                STEAM_PROCESSES
                    .iter()
                    .any(|w| p.name.eq_ignore_ascii_case(w)),
                "{} should not have matched",
                p.name
            );
        }
    }

    #[test]
    fn a_synthetic_token_reconfirms_without_touching_the_system() {
        let t = SteamStopped::synthetic_for_test();
        assert!(t.reconfirm().is_ok());
        assert!(t.age() < Duration::from_secs(5));
    }

    fn sample() -> Vec<SteamProcess> {
        vec![
            SteamProcess {
                pid: 43,
                name: "steamwebhelper.exe".into(),
            },
            SteamProcess {
                pid: 42,
                name: "steam.exe".into(),
            },
        ]
    }

    #[test]
    fn the_running_error_names_the_processes_and_says_what_to_do() {
        let (count, names) = describe(&sample());
        let msg = Error::StillRunning { count, names }.to_string();
        // The message has to be actionable on its own — it is what the UI shows.
        assert!(msg.contains("steam.exe (pid 42)"), "{msg}");
        assert!(msg.contains("steamwebhelper.exe (pid 43)"), "{msg}");
        assert!(msg.contains("Close Steam"), "{msg}");
        // "still running after 0ns" was the first draft. A duration belongs only on the
        // timeout case, where waiting actually happened.
        assert!(!msg.contains("0ns"), "{msg}");
    }

    #[test]
    fn the_timeout_error_is_distinct_and_reports_how_long_it_waited() {
        let (count, names) = describe(&sample());
        let msg = Error::ShutdownTimedOut {
            waited: Duration::from_secs(45),
            count,
            names,
        }
        .to_string();
        assert!(msg.contains("45s"), "{msg}");
        // Different remedy from StillRunning, so it must not tell the user to close Steam —
        // they already did, via us.
        assert!(!msg.contains("Close Steam"), "{msg}");
    }

    #[test]
    fn process_descriptions_are_sorted_for_stable_error_text() {
        let (count, names) = describe(&sample());
        assert_eq!(count, 2);
        assert_eq!(names, "steam.exe (pid 42), steamwebhelper.exe (pid 43)");
    }

    #[test]
    fn verify_stopped_agrees_with_is_running() {
        // Whichever state this machine is in, the two must not disagree — a token handed out
        // while `is_running()` is true would be exactly the bug this module prevents.
        let token = verify_stopped();
        assert_eq!(token.is_ok(), !is_running());
    }

    #[cfg(not(windows))]
    #[test]
    fn spawning_is_refused_off_windows() {
        assert!(matches!(
            imp::spawn_detached(std::path::Path::new("/steam"), &[]),
            Err(Error::UnsupportedPlatform)
        ));
    }
}
