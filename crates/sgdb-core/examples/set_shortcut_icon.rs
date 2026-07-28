//! S9 — read/modify/write `shortcuts.vdf` with our own codec.
//!
//! ```powershell
//! cargo run -p sgdb-core --example set_shortcut_icon -- <path> <new-icon-path>
//! cargo run -p sgdb-core --example set_shortcut_icon -- <path> --show
//! ```
//!
//! This is both the S9 experiment and an end-to-end test of `vdf::binary`: the round-trip
//! unit tests prove we can reproduce a file byte-for-byte, but only a real modify-and-write
//! proves Steam still accepts what we produce.
//!
//! ⚠️ **Steam must be fully shut down.** It holds `shortcuts.vdf` in memory and rewrites it on
//! exit, so a write while it runs is silently discarded — the failure mode this whole exercise
//! exists to characterise. In `sgdb-core` proper this is enforced by a `SteamStopped` token
//! that only `steam::process` can mint; here the caller is responsible.
//!
//! Writes atomically (temp → rename) and refuses to touch a file it cannot parse.

use sgdb_core::vdf::binary::{self, Entry, Value};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: set_shortcut_icon <shortcuts.vdf> <new-icon-path> | --show");
        std::process::exit(2);
    };

    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("read {path}: {e}");
            std::process::exit(1);
        }
    };

    let mut doc = match binary::parse(&bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("parse failed, refusing to write: {e}");
            std::process::exit(1);
        }
    };

    // Confirm we can reproduce the input exactly before modifying it. If this fails the codec
    // is wrong and we must not write anything.
    if binary::write(&doc) != bytes {
        eprintln!("round-trip mismatch — refusing to write");
        std::process::exit(1);
    }
    println!("parsed {} bytes, round-trip verified", bytes.len());

    let show_only = args.get(2).map(|s| s == "--show").unwrap_or(true);

    // shortcuts -> "0" -> icon
    let Some(shortcuts) = doc
        .entries
        .iter_mut()
        .find(|e| e.key == b"shortcuts")
        .and_then(|e| match &mut e.value {
            Value::Map(m) => Some(m),
            _ => None,
        })
    else {
        eprintln!("no `shortcuts` map");
        std::process::exit(1);
    };

    let Some(first) = shortcuts.first_mut().and_then(|e| match &mut e.value {
        Value::Map(m) => Some(m),
        _ => None,
    }) else {
        eprintln!("no shortcut entries");
        std::process::exit(1);
    };

    let current = first
        .iter()
        .find(|e| e.key.eq_ignore_ascii_case(b"icon"))
        .and_then(|e| e.value.as_str())
        .unwrap_or("<none>")
        .to_string();
    println!("current icon: {current}");

    if show_only {
        return;
    }

    let Some(new_icon) = args.get(2) else { return };

    match first.iter_mut().find(|e| e.key.eq_ignore_ascii_case(b"icon")) {
        Some(e) => e.value = Value::Str(new_icon.as_bytes().to_vec()),
        None => first.push(Entry {
            key: b"icon".to_vec(),
            value: Value::Str(new_icon.as_bytes().to_vec()),
        }),
    }

    // Atomic write: temp in the same directory, then rename over the target.
    let out = binary::write(&doc);
    let tmp = format!("{path}.sgdbtmp");
    if let Err(e) = std::fs::write(&tmp, &out) {
        eprintln!("write temp: {e}");
        std::process::exit(1);
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        eprintln!("rename: {e}");
        let _ = std::fs::remove_file(&tmp);
        std::process::exit(1);
    }

    println!("wrote {} bytes (was {})", out.len(), bytes.len());
    println!("new icon: {new_icon}");
}
