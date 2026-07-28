//! All logic for the SteamGridDB artwork manager.
//!
//! Architecture rules, enforced by `scripts/gate.ps1` and CI:
//!
//! 1. This crate must not depend on `tauri` or `anyhow`. It is usable headless.
//! 2. **Only `grid::store`, `steam::shortcuts`, and `settings` may write files.** Everything
//!    else is read-only. This project's failure mode is corrupting a user's Steam config, so
//!    the write surface is kept small enough to audit by grep.
//! 3. `steam://flushconfig` is banned outright — it has historically made Steam forget its
//!    library folder locations.

pub mod logo;
pub mod vdf;
