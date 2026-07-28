//! Custom artwork in `userdata/<accountid>/config/grid/`.
//!
//! - [`names`] — pure filename rules and [`names::AssetType`].
//! - [`store`] — the only artwork writer: sibling cleanup, atomic write, logo-position sidecar.

pub mod names;
pub mod store;

pub use names::AssetType;
pub use store::GridDir;
