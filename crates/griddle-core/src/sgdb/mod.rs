//! The SteamGridDB API v2 client.
//!
//! - [`key`] — the API key, wrapped so it cannot leak through `Debug`, `Display` or serde.
//! - [`model`] — response types, every field read off a real response.
//! - [`query`] — which endpoint to ask and with what filters.
//! - [`client`] — the HTTP client. **The only place the key is used.**
//!
//! The key never leaves Rust. The frontend searches over `invoke` rather than holding it, and
//! rate limiting only works as a guarantee if it lives in exactly one place.

pub mod client;
pub mod key;
pub mod model;
pub mod query;

pub use client::{Client, Config, Target};
pub use key::ApiKey;
pub use model::{Asset, AssetPage, Author, Game};
pub use query::{
    ASSET_TYPE_FILTERS, AssetKind, AssetQuery, Dimensions, FilterParams, PAGE_LIMIT, QueryError,
    Tri,
};
