//! Valve KeyValues codecs.
//!
//! - [`binary`] — binary KV1, used by `shortcuts.vdf`. Read **and** write, byte-exact.
//! - [`text`] — text KV1, used by `libraryfolders.vdf`, `appmanifest_*.acf`,
//!   `loginusers.vdf`. Read-only.
//! - [`appinfo`] — `appcache/appinfo.vdf`. Read-only, and **not** the same format as
//!   [`binary`]: from v29 its KV keys are u32 indices into a string table rather than
//!   NUL-terminated strings.

pub mod appinfo;
pub mod binary;
pub mod text;
