//! Spike gate **S7**: byte-identical round-trip of a *real* `shortcuts.vdf`.
//!
//! The synthetic fixture in `vdf::binary`'s unit tests is committed to git and proves the
//! codec's shape. This proves it against a file Steam actually wrote — which is the only
//! evidence that counts, because every surprising property of this format so far (the fourth
//! `0x08`, the mixed path separators, the signed appid) was discovered by reading a real
//! file rather than a specification.
//!
//! The real file is **not** committed: it contains home directory paths. Point the test at
//! it explicitly.
//!
//! ```powershell
//! $env:SGDB_REAL_SHORTCUTS = "C:\Program Files (x86)\Steam\userdata\16274804\config\shortcuts.vdf"
//! cargo test -p sgdb-core --test real_shortcuts -- --nocapture
//! ```
//!
//! Read-only. This test never writes to the Steam directory.

use sgdb_core::vdf::binary;

/// Skips rather than fails when the env var is unset, so CI (which has no Steam install)
/// stays green while a developer with a real client gets the stronger check.
fn real_file() -> Option<(std::path::PathBuf, Vec<u8>)> {
    let path = std::env::var_os("SGDB_REAL_SHORTCUTS")?;
    let path = std::path::PathBuf::from(path);
    match std::fs::read(&path) {
        Ok(bytes) => Some((path, bytes)),
        Err(e) => panic!(
            "SGDB_REAL_SHORTCUTS is set but unreadable: {} ({e})",
            path.display()
        ),
    }
}

#[test]
fn round_trips_byte_for_byte() {
    let Some((path, original)) = real_file() else {
        eprintln!("skipped: set SGDB_REAL_SHORTCUTS to run the S7 gate");
        return;
    };

    let doc = match binary::parse(&original) {
        Ok(doc) => doc,
        Err(e) => panic!("failed to parse {}: {e}", path.display()),
    };
    let rewritten = binary::write(&doc);

    if rewritten != original {
        // Locate the first divergence — a length-only message is useless for a binary format.
        let at = original
            .iter()
            .zip(&rewritten)
            .position(|(a, b)| a != b)
            .unwrap_or(original.len().min(rewritten.len()));
        panic!(
            "round-trip diverged at byte {at}: original {:02x?} vs rewritten {:02x?} \
             (lengths {} vs {})",
            &original[at.saturating_sub(8)..(at + 8).min(original.len())],
            &rewritten[at.saturating_sub(8)..(at + 8).min(rewritten.len())],
            original.len(),
            rewritten.len(),
        );
    }

    eprintln!(
        "S7 PASS: {} bytes round-tripped exactly, {} file-level terminator(s)",
        original.len(),
        doc.trailing_terminators
    );
}

/// Records what the real file actually contains, so a future Steam update that changes the
/// shape shows up as a test failure rather than a mystery. Prints an inventory when run with
/// `--nocapture`.
#[test]
fn reports_the_real_shortcut_inventory() {
    let Some((_, bytes)) = real_file() else {
        eprintln!("skipped: set SGDB_REAL_SHORTCUTS to run the S7 gate");
        return;
    };

    let doc = match binary::parse(&bytes) {
        Ok(doc) => doc,
        Err(e) => panic!("parse failed: {e}"),
    };

    let Some(shortcuts) = binary::get(&doc.entries, "shortcuts").and_then(|v| v.as_map()) else {
        panic!("no top-level `shortcuts` map — the file shape has changed");
    };

    eprintln!("{} shortcut(s):", shortcuts.len());
    for entry in shortcuts {
        let Some(fields) = entry.value.as_map() else {
            continue;
        };
        let appid = binary::get(fields, "appid").and_then(|v| v.as_i32());
        let name = binary::get(fields, "AppName")
            .or_else(|| binary::get(fields, "appname"))
            .and_then(|v| v.as_str())
            .unwrap_or("<none>");

        match appid {
            // Grid artwork filenames use the unsigned form of this signed field.
            Some(id) => eprintln!(
                "  [{}] {name}  appid={id} (grid name: {})",
                entry.key.escape_ascii(),
                id as u32
            ),
            None => eprintln!("  [{}] {name}  appid=<missing>", entry.key.escape_ascii()),
        }

        // The appid must be in the high-bit-set range Steam assigns to non-Steam shortcuts.
        // If this ever fails, the "never compute an appid, always read it" rule needs
        // revisiting — see the disproven CRC32 folklore in CLAUDE.md.
        if let Some(id) = appid {
            assert!(
                (id as u32) & 0x8000_0000 != 0,
                "shortcut appid {id} does not have the high bit set",
            );
        }
    }
}
