//! The S9 harness, now running on `steam::shortcuts` and `steam::process` rather than on
//! hand-rolled code.
//!
//! ```powershell
//! # Read-only. Always safe, works while Steam is running.
//! cargo run -p griddle-core --example set_shortcut_icon
//!
//! # Write. Refuses unless Steam is stopped; --shutdown will stop and restart it.
//! cargo run -p griddle-core --example set_shortcut_icon -- --appid 4048848997 --icon C:\path\to.ico
//! cargo run -p griddle-core --example set_shortcut_icon -- --appid 4048848997 --icon C:\to.ico --shutdown
//! ```
//!
//! S9 proved the choreography works with a throwaway implementation. This is the same
//! experiment driven through the real library, so what ships is what was tested:
//!
//! - the round-trip check refuses to write a file we cannot reproduce,
//! - `SteamStopped` cannot be obtained without Steam actually being down,
//! - the pristine file is preserved at `shortcuts.vdf.sgdb-orig` before the first write,
//! - and the result is read back and compared.
//!
//! This writes to a **real** `shortcuts.vdf`. It keeps the original, but a backup you have
//! not tested restoring is not a backup — `--restore` exists for that reason.

use griddle_core::appid::AppId;
use griddle_core::steam::{account, locate, process, shortcuts::Shortcuts};
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    if flag("--help") || flag("-h") {
        eprintln!(
            "usage: set_shortcut_icon [--appid <id> --icon <path> [--shutdown]] [--restore] \
             [--path <shortcuts.vdf>]"
        );
        return;
    }

    // Resolve the file: an explicit --path, else the signed-in account's.
    let (path, install) = match value("--path") {
        Some(p) => (std::path::PathBuf::from(p), None),
        None => {
            let install = match locate::locate() {
                Ok(i) => i,
                Err(e) => return die(&format!("locate Steam: {e}")),
            };
            let acct = match account::resolve(&install) {
                Ok(a) => a,
                Err(e) => return die(&format!("resolve account: {e}")),
            };
            (install.shortcuts_vdf(acct.id), Some(install))
        }
    };
    println!("file: {}", path.display());

    if flag("--restore") {
        return restore(&path, install.as_ref());
    }

    let mut sc = match Shortcuts::load_or_empty(&path) {
        Ok(s) => s,
        // The round-trip guard fires here, before any edit exists to lose.
        Err(e) => return die(&format!("{e}")),
    };
    println!("parsed, round-trip verified, {} shortcut(s)", sc.len());

    for s in sc.iter() {
        println!(
            "  {:>10}  {:<28} icon: {}",
            s.app_id().map_or("?".into(), |a| a.to_string()),
            s.app_name().unwrap_or("<unnamed>"),
            s.icon().unwrap_or("<none>")
        );
    }

    let (Some(appid), Some(icon)) = (value("--appid"), value("--icon")) else {
        println!("\n(read-only; pass --appid and --icon to write)");
        return;
    };
    let Ok(appid) = appid.parse::<u32>().map(AppId::new) else {
        return die("--appid must be an unsigned integer (the form used in grid/ filenames)");
    };

    // Getting the token is the only way to reach `save`. With --shutdown we stop Steam to
    // obtain one; otherwise the user must already have closed it.
    let token = if flag("--shutdown") {
        let Some(install) = install.as_ref() else {
            return die("--shutdown needs a located Steam install (drop --path)");
        };
        println!("\nshutting Steam down…");
        match process::shutdown(install, process::DEFAULT_SHUTDOWN_TIMEOUT) {
            Ok(t) => {
                println!("  stopped");
                t
            }
            Err(e) => return die(&format!("{e}")),
        }
    } else {
        match process::verify_stopped() {
            Ok(t) => t,
            Err(e) => return die(&format!("{e}")),
        }
    };

    let change = match sc.set_icon(appid, &icon) {
        Ok(c) => c,
        Err(e) => return die(&format!("{e}")),
    };
    println!(
        "\nicon: {} -> {} ({})",
        change.previous.as_deref().unwrap_or("<none>"),
        change.applied,
        if change.quoted {
            "quoted, matching the file"
        } else {
            "bare, matching the file"
        }
    );

    match sc.save(&token) {
        Ok(saved) => {
            println!("wrote {} bytes", saved.bytes_written);
            if let Some(b) = saved.backup_created {
                println!("original preserved at {}", b.display());
            }
        }
        Err(e) => return die(&format!("save: {e}")),
    }

    if flag("--shutdown")
        && let Some(install) = install.as_ref()
    {
        println!("\nrelaunching Steam…");
        if let Err(e) = process::launch(install) {
            return die(&format!("relaunch: {e}"));
        }
        // The registry pid appears well before the client is usable, so wait on the helpers.
        let up = process::wait_until_running(Duration::from_secs(90), 3);
        println!("  {}", if up { "running" } else { "still starting" });

        // The point of S9: does our file survive a full Steam startup, or does Steam reject
        // it and rewrite from its own state? Read it back *after* the client is up.
        match Shortcuts::load(&path) {
            Ok(after) => {
                let icon = after
                    .find(appid)
                    .and_then(|s| s.icon())
                    .unwrap_or("<gone>")
                    .to_string();
                println!("after restart, icon reads: {icon}");
                println!(
                    "{}",
                    if icon == change.applied {
                        "✅ Steam accepted and kept our file"
                    } else {
                        "FAIL: Steam rewrote it — the value did not survive"
                    }
                );
            }
            Err(e) => println!("re-read failed: {e}"),
        }
    }
}

/// Put the pristine `.sgdb-orig` back.
///
/// Restoring also needs Steam stopped — after a relaunch Steam holds the modified shortcuts
/// in memory and would overwrite the restored file on exit. `[VERIFIED-BOX 2026-07-27]`
fn restore(path: &std::path::Path, install: Option<&locate::SteamInstall>) {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".sgdb-orig");
    let backup = path.with_file_name(name);
    if !backup.is_file() {
        return die(&format!("no backup at {}", backup.display()));
    }

    if let Err(e) = process::verify_stopped() {
        return die(&format!(
            "{e}\n(restoring needs Steam down too — it would otherwise overwrite \
             the restored file on exit)"
        ));
    }

    match std::fs::copy(&backup, path) {
        Ok(n) => println!("restored {n} bytes from {}", backup.display()),
        Err(e) => return die(&format!("restore: {e}")),
    }
    if let Some(install) = install {
        println!(
            "Steam is stopped; relaunch with: {}",
            install.steam_exe().display()
        );
    }
}

fn die(msg: &str) {
    eprintln!("error: {msg}");
    std::process::exit(1);
}
