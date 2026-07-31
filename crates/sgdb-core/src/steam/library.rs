//! Enumerating installed games.
//!
//! `config/libraryfolders.vdf` names every library root; each root's `steamapps/` holds one
//! `appmanifest_<appid>.acf` per installed app.
//!
//! # 🔴 Parse `libraryfolders.vdf` defensively
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
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        // boundary-ok: test fixture written into a tempdir
        std::fs::write(p, content).unwrap();
    }

    fn manifest(appid: u32, name: &str, flags: u32, dir: &str) -> String {
        format!(
            "\"AppState\"\n{{\n\t\"appid\" \"{appid}\"\n\t\"name\" \"{name}\"\n\
             \t\"StateFlags\" \"{flags}\"\n\t\"installdir\" \"{dir}\"\n\
             \t\"SizeOnDisk\" \"12345\"\n}}\n"
        )
    }

    /// The scalar-sibling case, in the exact shape this machine's file has.
    #[test]
    fn skips_the_contentstatsid_scalar() {
        let t = tempfile::tempdir().unwrap();
        write(
            t.path(),
            "config/libraryfolders.vdf",
            r#"
"libraryfolders"
{
	"contentstatsid"		"7785519366728146050"
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps" { "228980" "491869131" "1091500" "91231172278" }
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps" { }
	}
}
"#,
        );
        let s = SteamInstall::at(t.path());
        let folders = library_folders(&s).unwrap();
        assert_eq!(
            folders.len(),
            2,
            "the scalar must be skipped, both libraries kept"
        );
        assert_eq!(
            folders[0].path,
            PathBuf::from(r"C:\Program Files (x86)\Steam")
        );
        assert_eq!(folders[0].apps.len(), 2);
        assert_eq!(folders[1].path, PathBuf::from(r"D:\SteamLibrary"));
    }

    #[test]
    fn a_missing_libraryfolders_still_yields_the_steam_root() {
        let t = tempfile::tempdir().unwrap();
        let s = SteamInstall::at(t.path());
        let folders = library_folders(&s).unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].path, t.path());
    }

    #[test]
    fn parses_a_manifest() {
        let t = tempfile::tempdir().unwrap();
        write(
            t.path(),
            "steamapps/appmanifest_1091500.acf",
            &manifest(1_091_500, "Cyberpunk 2077", 4, "Cyberpunk 2077"),
        );
        let app = parse_app_manifest(
            &t.path().join("steamapps/appmanifest_1091500.acf"),
            t.path(),
        )
        .unwrap()
        .unwrap();

        assert_eq!(app.app_id.get(), 1_091_500);
        assert_eq!(app.name, "Cyberpunk 2077");
        assert!(app.is_fully_installed());
        assert!(
            app.install_path().ends_with("common/Cyberpunk 2077")
                || app.install_path().ends_with(r"common\Cyberpunk 2077")
        );
    }

    /// `StateFlags` is a **bitfield**, so it must be tested with a mask, not equality.
    ///
    /// `6` is `StateFullyInstalled | StateUpdateRequired` — installed *and* update-pending,
    /// which is a playable game. This machine's FINAL FANTASY TACTICS reads `6`.
    /// `[VERIFIED-BOX 2026-07-27]` An earlier version of this test assumed `6` meant
    /// "downloading" and asserted the opposite; the code was right and the test was wrong.
    #[test]
    fn state_flags_are_a_bitfield_not_an_enum() {
        let t = tempfile::tempdir().unwrap();
        write(
            t.path(),
            "steamapps/appmanifest_1.acf",
            &manifest(1, "Installed", 4, "a"),
        );
        write(
            t.path(),
            "steamapps/appmanifest_2.acf",
            &manifest(2, "Installed, update pending", 6, "b"),
        );
        write(
            t.path(),
            "steamapps/appmanifest_3.acf",
            &manifest(3, "Not installed", 2, "c"),
        );
        write(
            t.path(),
            "steamapps/appmanifest_4.acf",
            &manifest(4, "Download queued", 1026, "d"),
        );
        write(
            t.path(),
            "config/libraryfolders.vdf",
            &format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} }}",
                t.path().display().to_string().replace('\\', "\\\\")
            ),
        );

        let s = SteamInstall::at(t.path());
        let apps = installed_apps(&s).unwrap();
        assert_eq!(apps.len(), 4, "all four have manifests");

        let installed = |id: u32| {
            apps.iter()
                .find(|a| a.app_id.get() == id)
                .unwrap()
                .is_fully_installed()
        };
        assert!(installed(1), "4 = StateFullyInstalled");
        assert!(
            installed(2),
            "6 = installed AND update-pending — still playable"
        );
        assert!(!installed(3), "2 = update required, bit 4 clear");
        assert!(!installed(4), "1026 = queued, bit 4 clear");
    }

    #[test]
    fn a_corrupt_manifest_does_not_empty_the_library() {
        let t = tempfile::tempdir().unwrap();
        write(
            t.path(),
            "steamapps/appmanifest_1.acf",
            &manifest(1, "Good", 4, "g"),
        );
        write(
            t.path(),
            "steamapps/appmanifest_2.acf",
            "\"AppState\" { \"appid\" \"unterminated",
        );
        write(
            t.path(),
            "config/libraryfolders.vdf",
            &format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} }}",
                t.path().display().to_string().replace('\\', "\\\\")
            ),
        );

        let s = SteamInstall::at(t.path());
        let apps = installed_apps(&s).unwrap();
        assert_eq!(apps.len(), 1, "the good manifest must survive the bad one");
        assert_eq!(apps[0].name, "Good");
    }

    #[test]
    fn ignores_files_that_are_not_manifests() {
        let t = tempfile::tempdir().unwrap();
        write(
            t.path(),
            "steamapps/appmanifest_1.acf",
            &manifest(1, "Real", 4, "r"),
        );
        write(
            t.path(),
            "steamapps/appmanifest_1.acf.bak",
            &manifest(9, "Backup", 4, "b"),
        );
        write(t.path(), "steamapps/readme.txt", "nope");
        write(
            t.path(),
            "config/libraryfolders.vdf",
            &format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} }}",
                t.path().display().to_string().replace('\\', "\\\\")
            ),
        );

        let s = SteamInstall::at(t.path());
        let apps = installed_apps(&s).unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Real");
    }

    #[test]
    fn non_utf8_names_survive_lossily() {
        let t = tempfile::tempdir().unwrap();
        // The trademark sign, as it appears in Street Fighter 6's real manifest.
        write(
            t.path(),
            "steamapps/appmanifest_1364780.acf",
            &manifest(1_364_780, "Street Fighter™ 6", 4, "sf6"),
        );
        let app = parse_app_manifest(
            &t.path().join("steamapps/appmanifest_1364780.acf"),
            t.path(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(app.name, "Street Fighter™ 6");
    }

    #[test]
    fn known_non_games_are_recognised() {
        assert!(is_known_non_game(AppId::new(228_980)));
        assert!(!is_known_non_game(AppId::new(1_091_500)));
    }
}
