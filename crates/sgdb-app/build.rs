//! Embeds the Big Picture injection bundle into the binary.
//!
//! `apps/bpm` builds to a single self-contained IIFE — no code splitting, no dynamic import,
//! because the whole thing is handed to CDP `Runtime.evaluate` as one string.
//!
//! If the bundle hasn't been built (fresh clone, or `cargo check` without bun), we emit a stub
//! that throws a legible error rather than failing the Rust build. Keeping `cargo check`
//! working without a JS toolchain is worth the small amount of ceremony.

use std::path::PathBuf;

const STUB: &str = r#"(() => {
  throw new Error("BPM bundle was not built. Run: bun run build:bpm");
})();"#;

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    let bundle = manifest_dir.join("../../apps/bpm/dist/bpm.js");

    println!("cargo:rerun-if-changed={}", bundle.display());
    println!("cargo:rerun-if-env-changed=SGDB_BPM_BUNDLE");

    let contents = match std::fs::read_to_string(&bundle) {
        Ok(js) => js,
        Err(_) => {
            println!(
                "cargo:warning=BPM bundle not found at {} — embedding a stub. \
                 Run `bun run build:bpm` for a working Big Picture injection.",
                bundle.display()
            );
            STUB.to_string()
        }
    };

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into())).join("bpm.js");
    if let Err(e) = std::fs::write(&out, contents) {
        panic!("failed to stage BPM bundle at {}: {e}", out.display());
    }

    tauri_build::build();
}
