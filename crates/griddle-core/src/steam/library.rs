//! Enumerating installed games.
//!
//! `config/libraryfolders.vdf` names every library root; each root's `steamapps/` holds one
//! `appmanifest_<appid>.acf` per installed app.
//!
//! # Parse `libraryfolders.vdf` defensively
//!
//! Its children are numbered `"0"`, `"1"`, … but some client versions emit a **scalar**
//! sibling among them:
//!
//! ```text
//! "libraryfolders"
//! {
//!     "contentstatsid"  "7785519366728146050"
//!     "0" { "path" "C:\\Program Files (x86)\\Steam"  "apps" { "228980" "491869131" } }
//! }
//! ```
//!
//! Code that assumes every child is a map skips real libraries or panics. This is the single
//! most common breakage in third-party parsers.
//! `[VERIFIED-SOURCE — steamlocate-rs #3, HXE #218]`
//!
//! # `StateFlags`
//!
//! A bitfield; bit 2 (`4`) is `StateFullyInstalled`. Filtering on it drops apps that are
//! downloading, queued, or update-pending — which have a manifest but no playable install.

use crate::appid::AppId;
use crate::steam::locate::SteamInstall;
use crate::vdf::text;
use std::path::{Path, PathBuf};

/// `StateFullyInstalled`.
const STATE_FULLY_INSTALLED: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: crate::vdf::text::Error,
    },
}

/// One library root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFolder {
    pub path: PathBuf,
    /// Appids the manifest claims are installed here, from the nested `apps` map. A cheap
    /// first pass; the `.acf` files are authoritative.
    pub apps: Vec<AppId>,
}

impl LibraryFolder {
    pub fn steamapps_dir(&self) -> PathBuf {
        self.path.join("steamapps")
    }
}

/// An installed app, from its `appmanifest_<appid>.acf`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub app_id: AppId,
    pub name: String,
    pub install_dir: String,
    pub state_flags: u32,
    pub size_on_disk: u64,
    pub last_played: u64,
    /// Which library root it came from.
    pub library: PathBuf,
}

impl InstalledApp {
    pub fn is_fully_installed(&self) -> bool {
        self.state_flags & STATE_FULLY_INSTALLED != 0
    }

    pub fn install_path(&self) -> PathBuf {
        self.library
            .join("steamapps")
            .join("common")
            .join(&self.install_dir)
    }
}

/// Parse `libraryfolders.vdf`.
///
/// Unreadable or unparseable entries are skipped rather than failing the whole scan — one bad
/// library should not hide the others.
pub fn library_folders(install: &SteamInstall) -> Result<Vec<LibraryFolder>, Error> {
    let path = install.library_folders_vdf();
    if !path.is_file() {
        // A fresh install with no extra libraries still has games under the Steam root.
        return Ok(vec![LibraryFolder {
            path: install.root().to_path_buf(),
            apps: Vec::new(),
        }]);
    }

    let raw = std::fs::read(&path).map_err(|e| Error::Read {
        path: path.clone(),
        source: e,
    })?;
    let doc = text::parse(&String::from_utf8_lossy(&raw)).map_err(|e| Error::Parse {
        path: path.clone(),
        source: e,
    })?;

    let Some(root) = text::get(&doc.entries, "libraryfolders").and_then(|v| v.as_map()) else {
        return Ok(Vec::new());
    };

    let mut out = Vec::new();
    for entry in root {
        // THE defensive step: skip `contentstatsid` and any other scalar sibling.
        let Some(fields) = entry.value.as_map() else {
            continue;
        };
        let Some(p) = text::get(fields, "path").and_then(|v| v.as_str()) else {
            continue;
        };

        let apps = text::get(fields, "apps")
            .and_then(|v| v.as_map())
            .map(|m| {
                m.iter()
                    .filter_map(|a| a.key.parse::<u32>().ok().map(AppId::new))
                    .collect()
            })
            .unwrap_or_default();

        out.push(LibraryFolder {
            path: PathBuf::from(p),
            apps,
        });
    }
    Ok(out)
}

/// Parse one `appmanifest_<appid>.acf`.
pub fn parse_app_manifest(path: &Path, library: &Path) -> Result<Option<InstalledApp>, Error> {
    let raw = std::fs::read(path).map_err(|e| Error::Read {
        path: path.to_path_buf(),
        source: e,
    })?;
    let doc = text::parse(&String::from_utf8_lossy(&raw)).map_err(|e| Error::Parse {
        path: path.to_path_buf(),
        source: e,
    })?;

    let Some(state) = text::get(&doc.entries, "AppState").and_then(|v| v.as_map()) else {
        return Ok(None);
    };
    let Some(app_id) = text::get(state, "appid")
        .and_then(|v| v.as_u32())
        .map(AppId::new)
    else {
        return Ok(None);
    };

    Ok(Some(InstalledApp {
        app_id,
        name: text::get(state, "name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        install_dir: text::get(state, "installdir")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        state_flags: text::get(state, "StateFlags")
            .and_then(|v| v.as_u32())
            .unwrap_or(0),
        size_on_disk: text::get(state, "SizeOnDisk")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        last_played: text::get(state, "LastPlayed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        library: library.to_path_buf(),
    }))
}

/// Every installed app across every library.
///
/// A manifest that cannot be read or parsed is skipped — a single corrupt `.acf` must not
/// empty the user's library. Results are sorted by appid so the order is stable.
pub fn installed_apps(install: &SteamInstall) -> Result<Vec<InstalledApp>, Error> {
    let mut out = Vec::new();
    for folder in library_folders(install)? {
        let Ok(entries) = std::fs::read_dir(folder.steamapps_dir()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                continue;
            }
            match parse_app_manifest(&path, &folder.path) {
                Ok(Some(app)) => out.push(app),
                Ok(None) => {}
                Err(e) => tracing::warn!(path = %path.display(), error = %e, "skipping manifest"),
            }
        }
    }
    out.sort_by_key(|a| a.app_id.get());
    out.dedup_by_key(|a| a.app_id.get());
    Ok(out)
}

/// Appids that are Steam's own tooling rather than games.
///
/// A fallback for when `appinfo.vdf` cannot be read — `steam::apptype` is the real filter.
/// Kept short and specific; a long blocklist becomes wrong faster than it becomes useful.
pub const KNOWN_NON_GAMES: [u32; 6] = [
    228980,  // Steamworks Common Redistributables
    1070560, // Steam Linux Runtime 1.0
    1391110, // Steam Linux Runtime 2.0 (soldier)
    1493710, // Proton Experimental
    1826330, // Steam Linux Runtime 3.0 (sniper)
    2180100, // Proton Hotfix
];

pub fn is_known_non_game(app: AppId) -> bool {
    KNOWN_NON_GAMES.contains(&app.get())
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
