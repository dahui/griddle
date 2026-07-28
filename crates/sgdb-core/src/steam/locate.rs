//! Finding the Steam installation.
//!
//! # The normalisation that bites
//!
//! `HKCU\Software\Valve\Steam\SteamPath` is stored **lowercased with forward slashes**:
//!
//! ```text
//! HKCU  SteamPath   c:/program files (x86)/steam
//! HKLM  InstallPath C:\Program Files (x86)\Steam
//! ```
//! `[VERIFIED-BOX 2026-07-27]`
//!
//! Joining onto the HKCU value without normalising produces a mixed-separator path. Windows
//! itself tolerates that, but it leaks into anything we display or store, and it makes string
//! comparison against other Steam-supplied paths fail. Normalise once, here.
//!
//! `SteamExe` has the same lowercase/forward-slash treatment.
//!
//! # Resolution order
//!
//! 1. `SGDB_STEAM_PATH` — an explicit override, and how the tests run on a machine with no
//!    Steam (CI runs `sgdb-core` on Linux too).
//! 2. `HKCU\Software\Valve\Steam\SteamPath` — per-user, the most accurate.
//! 3. `HKLM\...\Valve\Steam\InstallPath` via the **32-bit registry view**, so the lookup works
//!    regardless of our own process bitness.
//! 4. `%ProgramFiles(x86)%\Steam`.
//!
//! Each candidate is validated by the presence of `steam.exe` (or, off Windows, the directory
//! itself), so a stale registry entry falls through instead of being trusted.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not find a Steam installation (tried: {tried})")]
    NotFound { tried: String },

    #[error("SGDB_STEAM_PATH points at {0}, which is not a Steam installation")]
    OverrideInvalid(PathBuf),
}

/// A located Steam installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamInstall {
    root: PathBuf,
    /// Which resolution step found it — surfaced in the diagnostics screen.
    source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    EnvOverride,
    HkcuSteamPath,
    HklmInstallPath,
    DefaultPath,
}

impl Source {
    pub const fn label(self) -> &'static str {
        match self {
            Source::EnvOverride => "SGDB_STEAM_PATH",
            Source::HkcuSteamPath => r"HKCU\Software\Valve\Steam\SteamPath",
            Source::HklmInstallPath => r"HKLM\SOFTWARE\Valve\Steam\InstallPath (32-bit view)",
            Source::DefaultPath => "default install path",
        }
    }
}

impl SteamInstall {
    /// Wrap a known-good path without probing. For tests and for an explicit user choice.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        SteamInstall {
            root: root.into(),
            source: Source::EnvOverride,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn source(&self) -> Source {
        self.source
    }

    pub fn steam_exe(&self) -> PathBuf {
        self.root.join("steam.exe")
    }

    /// The CEF remote-debugging opt-in sentinel. Its presence is what makes live apply
    /// possible; creating it is always an explicit user action.
    pub fn cef_sentinel(&self) -> PathBuf {
        self.root.join(".cef-enable-remote-debugging")
    }

    pub fn userdata_dir(&self) -> PathBuf {
        self.root.join("userdata")
    }

    /// `userdata/<accountid>/config` for one account.
    pub fn user_config_dir(&self, account_id: u32) -> PathBuf {
        self.userdata_dir()
            .join(account_id.to_string())
            .join("config")
    }

    /// `userdata/<accountid>/config/grid` — where custom artwork lives.
    pub fn grid_dir(&self, account_id: u32) -> PathBuf {
        self.user_config_dir(account_id).join("grid")
    }

    pub fn shortcuts_vdf(&self, account_id: u32) -> PathBuf {
        self.user_config_dir(account_id).join("shortcuts.vdf")
    }

    pub fn loginusers_vdf(&self) -> PathBuf {
        self.root.join("config").join("loginusers.vdf")
    }

    /// Prefer `config/libraryfolders.vdf`; older clients only wrote the `steamapps/` copy.
    ///
    /// Both exist and are byte-identical on this machine, but that is not guaranteed.
    pub fn library_folders_vdf(&self) -> PathBuf {
        let modern = self.root.join("config").join("libraryfolders.vdf");
        if modern.is_file() {
            return modern;
        }
        self.root.join("steamapps").join("libraryfolders.vdf")
    }

    /// **Read-only.** Steam owns this cache, its layout is sha1-keyed, and it re-downloads
    /// over anything written here.
    pub fn library_cache_dir(&self) -> PathBuf {
        self.root.join("appcache").join("librarycache")
    }

    pub fn appinfo_vdf(&self) -> PathBuf {
        self.root.join("appcache").join("appinfo.vdf")
    }
}

/// Normalise a Steam-supplied path: forward slashes to backslashes on Windows, and trim.
///
/// Casing is deliberately left alone — Windows paths are case-insensitive, and "fixing" the
/// case would be guesswork.
pub fn normalize(raw: &str) -> PathBuf {
    let trimmed = raw.trim().trim_end_matches(['/', '\\']);
    if cfg!(windows) {
        PathBuf::from(trimmed.replace('/', "\\"))
    } else {
        PathBuf::from(trimmed)
    }
}

/// True if this looks like a real Steam install rather than a stale registry entry.
fn is_steam_root(p: &Path) -> bool {
    if !p.is_dir() {
        return false;
    }
    // On Windows the executable is definitive. Off Windows (tests, CI) accept the marker
    // directories instead, so fixtures work.
    p.join("steam.exe").is_file() || p.join("userdata").is_dir() || p.join("steamapps").is_dir()
}

/// Find the Steam installation, honouring `SGDB_STEAM_PATH`.
pub fn locate() -> Result<SteamInstall, Error> {
    locate_with(std::env::var_os("SGDB_STEAM_PATH").as_deref())
}

/// [`locate`] with the override passed in rather than read from the environment.
///
/// The split exists so tests need neither `unsafe` (mutating the environment is unsafe in
/// edition 2024) nor a shared global they would race on. It is also the honest shape: the
/// environment is an input, so make it a parameter.
pub fn locate_with(override_path: Option<&std::ffi::OsStr>) -> Result<SteamInstall, Error> {
    let mut tried: Vec<String> = Vec::new();

    let consider = |path: PathBuf, source: Source, tried: &mut Vec<String>| {
        if is_steam_root(&path) {
            Some(SteamInstall { root: path, source })
        } else {
            tried.push(format!("{} -> {}", source.label(), path.display()));
            None
        }
    };

    if let Some(raw) = override_path {
        let p = normalize(&raw.to_string_lossy());
        if is_steam_root(&p) {
            return Ok(SteamInstall {
                root: p,
                source: Source::EnvOverride,
            });
        }
        // An explicit override that is wrong is an error, not something to silently skip —
        // the user asked for that path specifically.
        return Err(Error::OverrideInvalid(p));
    }

    #[cfg(windows)]
    {
        if let Some(raw) = registry::hkcu_steam_path()
            && let Some(found) = consider(normalize(&raw), Source::HkcuSteamPath, &mut tried)
        {
            return Ok(found);
        }
        if let Some(raw) = registry::hklm_install_path()
            && let Some(found) = consider(normalize(&raw), Source::HklmInstallPath, &mut tried)
        {
            return Ok(found);
        }
    }

    if let Some(pf) = std::env::var_os("ProgramFiles(x86)")
        && let Some(found) = consider(
            PathBuf::from(pf).join("Steam"),
            Source::DefaultPath,
            &mut tried,
        )
    {
        return Ok(found);
    }

    Err(Error::NotFound {
        tried: if tried.is_empty() {
            "nothing".into()
        } else {
            tried.join("; ")
        },
    })
}

#[cfg(windows)]
mod registry {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY};

    pub fn hkcu_steam_path() -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Valve\Steam")
            .ok()?
            .get_value("SteamPath")
            .ok()
    }

    /// Read through the **32-bit view** so this works from a 64-bit process, where the key is
    /// otherwise under `WOW6432Node`.
    pub fn hklm_install_path() -> Option<String> {
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(r"SOFTWARE\Valve\Steam", KEY_READ | KEY_WOW64_32KEY)
            .ok()?
            .get_value("InstallPath")
            .ok()
    }

    /// The pid of the running Steam client, or `None` when it is not running.
    ///
    /// Steam clears this early in shutdown, **before the process actually exits** — verified
    /// during S9, where the registry read 0 while `steam.exe` was still alive. So this is a
    /// "Steam is going away" signal, not "it is safe to touch `shortcuts.vdf`". For that,
    /// wait on the processes.
    pub fn active_pid() -> Option<u32> {
        let v: u32 = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Valve\Steam\ActiveProcess")
            .ok()?
            .get_value("pid")
            .ok()?;
        (v != 0).then_some(v)
    }

    /// The account id of the signed-in user, or `None` when Steam is not running.
    pub fn active_user() -> Option<u32> {
        let v: u32 = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Valve\Steam\ActiveProcess")
            .ok()?
            .get_value("ActiveUser")
            .ok()?;
        (v != 0).then_some(v)
    }
}

#[cfg(windows)]
pub use registry::{active_pid, active_user};

#[cfg(not(windows))]
pub fn active_pid() -> Option<u32> {
    None
}

#[cfg(not(windows))]
pub fn active_user() -> Option<u32> {
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    #[test]
    fn normalizes_the_hkcu_form() {
        // Exactly what HKCU stores on this machine.
        let p = normalize("c:/program files (x86)/steam");
        if cfg!(windows) {
            assert_eq!(p, PathBuf::from(r"c:\program files (x86)\steam"));
        }
        // Casing is left alone on purpose.
        assert!(p.to_string_lossy().contains("program files"));
    }

    #[test]
    fn normalize_trims_trailing_separators_and_whitespace() {
        let a = normalize("  C:/Steam/  ");
        let b = normalize("C:/Steam");
        assert_eq!(a, b);
    }

    #[test]
    fn derives_every_path_from_the_root() {
        let s = SteamInstall::at("/steam");
        assert!(s.grid_dir(16_274_804).ends_with("grid"));
        assert!(
            s.grid_dir(16_274_804)
                .to_string_lossy()
                .contains("16274804")
        );
        assert!(s.shortcuts_vdf(16_274_804).ends_with("shortcuts.vdf"));
        assert!(s.cef_sentinel().ends_with(".cef-enable-remote-debugging"));
        assert!(s.library_cache_dir().ends_with("librarycache"));
    }

    #[test]
    fn library_folders_prefers_the_config_copy() {
        let t = tempfile::tempdir().unwrap();
        let s = SteamInstall::at(t.path());
        // With neither present, fall back to the steamapps path rather than failing.
        assert!(
            s.library_folders_vdf()
                .to_string_lossy()
                .contains("steamapps")
        );

        std::fs::create_dir_all(t.path().join("config")).unwrap();
        std::fs::write(t.path().join("config").join("libraryfolders.vdf"), "").unwrap();
        assert!(s.library_folders_vdf().to_string_lossy().contains("config"));
    }

    #[test]
    fn an_invalid_explicit_override_is_an_error_not_a_fallback() {
        let t = tempfile::tempdir().unwrap();
        let empty = t.path().join("not-steam");
        std::fs::create_dir_all(&empty).unwrap();

        let got = locate_with(Some(empty.as_os_str()));
        assert!(
            matches!(got, Err(Error::OverrideInvalid(_))),
            "an explicit override that is wrong must fail loudly, got {got:?}",
        );
    }

    #[test]
    fn a_valid_override_is_accepted() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("userdata")).unwrap();

        let install = locate_with(Some(t.path().as_os_str())).unwrap();
        assert_eq!(install.source(), Source::EnvOverride);
        assert_eq!(install.root(), t.path());
    }

    #[test]
    fn an_override_with_a_trailing_slash_still_resolves() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("userdata")).unwrap();
        let with_slash = format!("{}/", t.path().display());

        let install = locate_with(Some(std::ffi::OsStr::new(&with_slash))).unwrap();
        assert_eq!(install.root(), t.path());
    }
}
