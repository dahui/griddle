//! Run the offline layer against the real Steam installation.
//!
//! ```powershell
//! cargo run -p griddle-core --example scan
//! ```
//!
//! Read-only. Unit tests use synthetic fixtures, which prove the parsers handle the shapes we
//! *thought* of; this proves they handle the machine we actually have.
//!
//! Set `SGDB_STEAM_PATH` to point at a different installation.

use griddle_core::grid::{AssetType, GridDir};
use griddle_core::steam::{account, apptype, library, locate};

fn main() {
    println!("== locate ==");
    let install = match locate::locate() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("  {e}");
            std::process::exit(1);
        }
    };
    println!("  root    : {}", install.root().display());
    println!("  via     : {}", install.source().label());
    println!("  steam.exe present : {}", install.steam_exe().is_file());
    println!("  CEF sentinel      : {}", install.cef_sentinel().is_file());
    println!(
        "  steam running     : {}",
        locate::active_pid().map_or("no".to_string(), |p| format!("yes (pid {p})"))
    );

    println!("\n== account ==");
    let acct = match account::resolve(&install) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("  {e}");
            std::process::exit(1);
        }
    };
    println!("  id      : {}", acct.id);
    println!("  via     : {}", acct.source.label());
    println!("  all ids : {:?}", account::userdata_accounts(&install));

    println!("\n== libraries ==");
    match library::library_folders(&install) {
        Ok(folders) => {
            for f in &folders {
                println!(
                    "  {} ({} apps listed in the manifest)",
                    f.path.display(),
                    f.apps.len()
                );
            }
        }
        Err(e) => eprintln!("  {e}"),
    }

    println!("\n== appinfo.vdf ==");
    let types = apptype::AppTypes::load_or_none(&install);
    match &types {
        Some(t) => println!(
            "  {} apps, {} skipped, {:?}, entry list {}\n  {}",
            t.len(),
            t.skipped(),
            t.version(),
            if t.aligned() {
                "ends exactly at the string table ✓"
            } else {
                "MISALIGNED — apps may have been missed"
            },
            t.path().display()
        ),
        None => println!("  unavailable — falling back to the id blocklist alone"),
    }

    println!("\n== installed apps ==");
    match library::installed_apps(&install) {
        Ok(apps) => {
            let installed: Vec<&library::InstalledApp> =
                apps.iter().filter(|a| a.is_fully_installed()).collect();
            let installed_count = installed.len();
            let (shown, hidden): (Vec<&library::InstalledApp>, Vec<&library::InstalledApp>) =
                installed
                    .into_iter()
                    .partition(|a| apptype::include_in_library(types.as_ref(), a.app_id));
            println!(
                "  {} manifests, {} fully installed, {} shown / {} filtered out",
                apps.len(),
                installed_count,
                shown.len(),
                hidden.len()
            );
            for a in shown.iter().take(10) {
                let kind = types
                    .as_ref()
                    .and_then(|t| t.app_type(a.app_id))
                    .map_or("?".to_string(), |t| t.label().to_string());
                println!("    {:>8}  [{kind}] {}", a.app_id.get(), a.name);
            }
            if shown.len() > 10 {
                println!("    … and {} more", shown.len() - 10);
            }
            // Anything filtered out is worth showing in full: a wrongly-excluded game is a bug
            // the user would otherwise just never notice.
            for a in &hidden {
                let kind = types
                    .as_ref()
                    .and_then(|t| t.app_type(a.app_id))
                    .map_or("blocklist".to_string(), |t| t.label().to_string());
                println!("    [hidden: {kind}] {:>8}  {}", a.app_id.get(), a.name);
            }
        }
        Err(e) => eprintln!("  {e}"),
    }

    // The unit tests build fixtures for the shapes we thought of. This measures coverage across
    // every appid Steam has actually cached, which is the only way to tell "the resolver works"
    // from "the resolver works on the two apps I picked".
    println!("\n== librarycache (Steam's own default art) ==");
    {
        use griddle_core::steam::LibraryCache;
        let cache = LibraryCache::new(&install, types.as_ref());
        let dir = install.library_cache_dir();
        println!("  {}", dir.display());

        let ids: Vec<u32> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
            .collect();
        println!("  {} cached appid directories", ids.len());

        for asset in AssetType::EDITABLE {
            let mut hits = 0usize;
            let mut nested = 0usize;
            for id in &ids {
                if let Some(p) = cache.resolve(griddle_core::appid::AppId::new(*id), asset) {
                    hits += 1;
                    // A hit whose parent is not the appid dir came through the sha1 layout,
                    // which only the appinfo index can reach.
                    if p.parent().and_then(|d| d.file_name())
                        != Some(std::ffi::OsStr::new(&id.to_string()))
                    {
                        nested += 1;
                    }
                }
            }
            println!(
                "    {:<13} {:>5} / {} resolved  ({} via a sha1 subdirectory)",
                asset.label(),
                hits,
                ids.len(),
                nested
            );
        }
    }

    // What the "Current" tab and the reset action both read. If `existing` cannot see a file
    // that is plainly on disk, every downstream symptom ("reset does nothing", "shows Not set")
    // follows from here rather than from the UI.
    println!("\n== custom artwork in grid/ ==");
    {
        let grid_dir = install.grid_dir(acct.id);
        println!("  {}", grid_dir.display());
        let grid = GridDir::new(grid_dir.clone());

        // Every appid that has at least one file in grid/, taken from the filenames themselves
        // so this does not depend on the library list being right.
        let mut ids: Vec<u32> = std::fs::read_dir(&grid_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_str()?.to_owned();
                let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
                digits.parse::<u32>().ok()
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();

        for id in ids {
            let app = griddle_core::appid::AppId::new(id);
            let found: Vec<String> = AssetType::EDITABLE
                .into_iter()
                .flat_map(|asset| {
                    grid.existing(app, asset)
                        .into_iter()
                        .filter_map(|p| Some(format!("{asset}={}", p.file_name()?.to_str()?)))
                        .collect::<Vec<_>>()
                })
                .collect();
            println!("  {:>10}  {}", id, found.join("  "));
        }
    }

    println!("\n== steam process ==");
    let procs = griddle_core::steam::process::running();
    if procs.is_empty() {
        println!("  not running — shortcuts.vdf is safe to write");
    } else {
        for p in &procs {
            println!("    {:<22} pid {}", p.name, p.pid);
        }
        println!("  running — shortcuts.vdf writes would be silently discarded");
    }

    println!("\n== non-steam shortcuts ==");
    let sc_path = install.shortcuts_vdf(acct.id);
    println!("  {}", sc_path.display());
    match griddle_core::steam::Shortcuts::load_or_empty(&sc_path) {
        Ok(sc) => {
            println!("  {} shortcut(s), round-trip verified", sc.len());
            for s in sc.iter() {
                println!(
                    "    {:>10}  {}",
                    s.app_id().map_or("?".into(), |a| a.to_string()),
                    s.app_name().unwrap_or("<unnamed>")
                );
                if let Some(icon) = s.icon_path() {
                    // The stored value may be quoted; icon_path() gives the filesystem form.
                    println!(
                        "                icon: {icon} [{}]",
                        if std::path::Path::new(icon).is_file() {
                            "present"
                        } else {
                            "MISSING"
                        }
                    );
                }
                if !s.tags().is_empty() {
                    println!("                tags: {}", s.tags().join(", "));
                }
            }
        }
        // A parse or round-trip failure here is exactly what must block a write, so it is
        // worth seeing on a real machine rather than only in tests.
        Err(e) => println!("  {e}"),
    }

    println!("\n== existing custom artwork ==");
    let grid = GridDir::new(install.grid_dir(acct.id));
    println!("  {}", grid.path().display());
    if !grid.path().is_dir() {
        println!("  (no grid directory — no custom art has ever been set)");
        return;
    }

    // Collect the appids that already have art, by reading filenames.
    let mut ids: Vec<u32> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(grid.path()) {
        for e in entries.flatten() {
            let Some(stem) = e
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
            else {
                continue;
            };
            let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = digits.parse::<u32>()
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
    }
    ids.sort_unstable();

    for id in &ids {
        let app = griddle_core::appid::AppId::new(*id);
        let mut have = Vec::new();
        for t in AssetType::EDITABLE {
            let files = grid.existing(app, t);
            match files.len() {
                0 => {}
                1 => have.push(format!("{t}")),
                // More than one means an ambiguous pair Steam resolves unpredictably.
                n => have.push(format!("{t} (⚠ {n} files)")),
            }
        }
        let kind = if app.is_shortcut_range() {
            "shortcut"
        } else {
            "steam app"
        };
        println!("  {:>10} [{kind}] {}", id, have.join(", "));
        if let Ok(Some(pos)) = grid.read_logo_position(app) {
            println!(
                "             logo position: {:?} {}x{}",
                pos.pinned_position, pos.width_pct, pos.height_pct
            );
        }
    }
}
