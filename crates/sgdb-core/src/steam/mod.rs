//! Reading the local Steam installation.
//!
//! - [`locate`] — find Steam; normalise the registry's lowercase/forward-slash paths.
//! - [`account`] — which `userdata/<accountid>` we are editing.
//! - [`library`] — library folders and installed apps.
//! - [`process`] — is Steam running, and stopping it. The only minter of [`SteamStopped`].
//! - [`shortcuts`] — non-Steam shortcuts. **The only writer in this subtree.**
//!
//! Everything else here is read-only. [`shortcuts::Shortcuts::save`] cannot be called without
//! a [`SteamStopped`] token, because Steam rewrites that file from memory on exit and a write
//! while it runs is silently discarded.

pub mod account;
pub mod apptype;
pub mod library;
pub mod locate;
pub mod process;
pub mod shortcuts;

pub use account::Account;
pub use apptype::{AppType, AppTypes};
pub use library::{InstalledApp, LibraryFolder};
pub use locate::SteamInstall;
pub use process::{SteamProcess, SteamStopped};
pub use shortcuts::{Shortcut, Shortcuts};
