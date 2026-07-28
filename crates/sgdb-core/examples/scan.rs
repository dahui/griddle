//! Run the offline layer against the real Steam installation.
//!
//! ```powershell
//! cargo run -p sgdb-core --example scan
//! ```
//!
//! Read-only. Unit tests use synthetic fixtures, which prove the parsers handle the shapes we
//! *thought* of; this proves they handle the machine we actually have.
//!
//! Set `SGDB_STEAM_PATH` to point at a different installation.

use sgdb_core::grid::{AssetType, GridDir};
use sgdb_core::steam::{account, library, locate};

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

    println!("\n== installed apps ==");
    match library::installed_apps(&install) {
        Ok(apps) => {
            let installed: Vec<_> = apps.iter().filter(|a| a.is_fully_installed()).collect();
            let games: Vec<_> = installed
                .iter()
                .filter(|a| !library::is_known_non_game(a.app_id))
                .collect();
            println!(
                "  {} manifests, {} fully installed, {} after dropping known tools",
                apps.len(),
                installed.len(),
                games.len()
            );
            for a in games.iter().take(10) {
                println!("    {:>8}  {}", a.app_id.get(), a.name);
            }
            if games.len() > 10 {
                println!("    … and {} more", games.len() - 10);
            }
            // Anything filtered out is worth showing: a wrongly-excluded game is a bug the
            // user would otherwise just not notice.
            for a in installed
                .iter()
                .filter(|a| library::is_known_non_game(a.app_id))
            {
                println!("    [tool] {:>8}  {}", a.app_id.get(), a.name);
            }
        }
        Err(e) => eprintln!("  {e}"),
    }

    println!("\n== steam process ==");
    let procs = sgdb_core::steam::process::running();
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
    match sgdb_core::steam::Shortcuts::load_or_empty(&sc_path) {
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
        let app = sgdb_core::appid::AppId::new(*id);
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
