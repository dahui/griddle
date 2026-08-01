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

// `unwrap`/`expect` are denied workspace-wide, because a panic mid-write is not an acceptable way
// to discover an invariant was wrong. Test assertions are the exception -- panicking *is* how a
// test fails -- and this says so once instead of the 35 times it used to be repeated, one copy
// per test module.
//
// `cfg(test)` rather than a `[lints]` entry: Cargo lint tables cannot be scoped to a build
// profile, and blanket-allowing would take the guard off the shipping code too.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod appid;
pub mod base64;
pub mod browser;
pub mod cache;
pub mod cdp;
pub mod fsutil;
pub mod grid;
pub mod input;
pub mod logo;
pub mod settings;
pub mod sgdb;
pub mod steam;
pub mod vdf;
