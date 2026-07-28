//! Valve KeyValues codecs.
//!
//! - [`binary`] — binary KV1, used by `shortcuts.vdf`. Read **and** write, byte-exact.
//! - [`text`] — text KV1, used by `libraryfolders.vdf`, `appmanifest_*.acf`,
//!   `loginusers.vdf`. Read-only.

pub mod binary;
pub mod text;
