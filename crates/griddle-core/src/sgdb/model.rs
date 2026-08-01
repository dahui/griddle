//! SteamGridDB API v2 response types.
//!
//! Every field here was read off a **real response** on 2026-07-30 with a real key, not from
//! documentation. `[VERIFIED-BOX 2026-07-30]`
//!
//! # Three envelope shapes, not one
//!
//! | Endpoint | Shape |
//! |---|---|
//! | `/games/steam/{appid}` | `{success, data: {…}}` — a single object |
//! | `/search/autocomplete/{term}` | `{success, data: [{…}]}` — an array, **no pagination** |
//! | `/grids`, `/heroes`, `/logos`, `/icons` | `{success, page, total, limit, data: [{…}]}` |
//!
//! [`Envelope`] covers all three by making the pagination fields optional.
//!
//! # Defensive deserialisation
//!
//! Only `id` and `url` are required on an asset — without them it cannot be displayed or
//! applied, so a missing one is genuinely fatal for that item. Everything else carries
//! `#[serde(default)]`, so a field SteamGridDB renames or drops costs one attribute rather
//! than the whole response. Unknown fields are ignored by serde already, so additions are free.

use serde::{Deserialize, Serialize};

/// Who uploaded an asset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Author {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub steam64: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

/// One piece of artwork.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Asset {
    pub id: u64,
    /// Full-size download. Always present; this is the thing we actually apply.
    pub url: String,

    /// Preview image. For icons this is a *rendered PNG* at a chosen size rather than the
    /// original — e.g. `…/icon/<hash>/32/256x256.png` against a `.ico` `url`.
    #[serde(default)]
    pub thumb: Option<String>,

    /// **Zero is legal.** Icons routinely report `0x0` — an observed real response has
    /// `"width": 0, "height": 0` for a `.ico`. Never divide by these or use them to compute an
    /// aspect ratio without checking.
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,

    #[serde(default)]
    pub style: Option<String>,
    /// `image/png`, `image/vnd.microsoft.icon`, `image/webp`, …
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    /// Nullable in real responses.
    #[serde(default)]
    pub notes: Option<String>,

    #[serde(default)]
    pub nsfw: bool,
    #[serde(default)]
    pub humor: bool,
    #[serde(default)]
    pub epilepsy: bool,
    #[serde(default)]
    pub lock: bool,

    #[serde(default)]
    pub score: i64,
    #[serde(default)]
    pub upvotes: i64,
    #[serde(default)]
    pub downvotes: i64,

    #[serde(default)]
    pub author: Author,
}

impl Asset {
    /// The best URL to show in a grid of results, falling back to the full asset.
    pub fn preview_url(&self) -> &str {
        self.thumb.as_deref().unwrap_or(&self.url)
    }

    /// True when this carries any content flag a user may have filtered on.
    pub fn is_flagged(&self) -> bool {
        self.nsfw || self.humor || self.epilepsy
    }

    /// `600x900`, or `None` when the dimensions are the placeholder zeros icons use.
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        (self.width > 0 && self.height > 0).then_some((self.width, self.height))
    }

    /// File extension implied by the mime type.
    ///
    /// Note this is **not** necessarily what we write to disk: animated WebP is deliberately
    /// saved as `.png`, because Chromium sniffs content and Steam's own code hardcodes `"png"`.
    pub fn extension_from_mime(&self) -> Option<&'static str> {
        match self.mime.as_deref()? {
            "image/png" | "image/apng" => Some("png"),
            "image/jpeg" => Some("jpg"),
            "image/webp" => Some("webp"),
            "image/gif" => Some("gif"),
            "image/vnd.microsoft.icon" | "image/x-icon" => Some("ico"),
            _ => None,
        }
    }
}

/// A game as SteamGridDB knows it. Its `id` is an SGDB id, **not** a Steam appid.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Game {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    /// Unix seconds. Absent for some entries.
    #[serde(default)]
    pub release_date: Option<i64>,
    /// Platforms this entry is linked to — `["steam"]`, sometimes empty.
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub verified: bool,
}

/// One page of assets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetPage {
    pub assets: Vec<Asset>,
    /// 0-based, as the API reports it.
    pub page: u32,
    /// Total matching assets across all pages. Drives infinite scroll.
    pub total: u32,
    pub limit: u32,
}

impl AssetPage {
    /// Whether another page exists, derived from `total` rather than guessed from a short page.
    pub fn has_more(&self) -> bool {
        if self.limit == 0 {
            return false;
        }
        let seen = u64::from(self.page + 1) * u64::from(self.limit);
        seen < u64::from(self.total)
    }
}

/// The JSON wrapper every endpoint uses.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Envelope<T> {
    #[serde(default)]
    pub success: bool,
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub total: Option<u32>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from a real `/grids/steam/620` response. `[VERIFIED-BOX 2026-07-30]`
    const REAL_GRID: &str = r#"{
      "success": true, "page": 0, "total": 424, "limit": 2,
      "data": [{
        "id": 103243, "score": 0, "style": "alternate", "width": 600, "height": 900,
        "nsfw": false, "humor": false, "notes": null, "mime": "image/png", "language": "en",
        "url": "https://cdn2.steamgriddb.com/grid/7668636048c4fbe8df8ffb388679e933.png",
        "thumb": "https://cdn2.steamgriddb.com/thumb/7668636048c4fbe8df8ffb388679e933.jpg",
        "lock": false, "epilepsy": false, "upvotes": 0, "downvotes": 0,
        "author": {"name": "Reiisen", "steam64": "76561198275966827",
                   "avatar": "https://avatars.steamstatic.com/19141b_medium.jpg"}
      }]
    }"#;

    #[test]
    fn parses_a_real_grid_response() {
        let env: Envelope<Vec<Asset>> = serde_json::from_str(REAL_GRID).unwrap();
        assert!(env.success);
        assert_eq!(env.total, Some(424));
        assert_eq!(env.page, Some(0));

        let assets = env.data.unwrap();
        assert_eq!(assets.len(), 1);
        let a = &assets[0];
        assert_eq!(a.id, 103243);
        assert_eq!(a.dimensions(), Some((600, 900)));
        assert_eq!(a.author.name, "Reiisen");
        assert_eq!(a.notes, None);
        assert_eq!(a.extension_from_mime(), Some("png"));
        assert!(!a.is_flagged());
    }

    /// A real `/icons/steam/620` entry: 0x0 dimensions and a `.ico` with a rendered PNG thumb.
    #[test]
    fn an_icon_with_zero_dimensions_parses_and_reports_no_dimensions() {
        let json = r#"{
          "id": 22093, "score": 0, "style": "custom", "width": 0, "height": 0,
          "nsfw": false, "humor": false, "notes": null,
          "mime": "image/vnd.microsoft.icon", "language": "en",
          "url": "https://cdn2.steamgriddb.com/icon/8d37be.ico",
          "thumb": "https://cdn2.steamgriddb.com/icon/8d37be/32/256x256.png",
          "lock": false, "epilepsy": false, "upvotes": 0, "downvotes": 0,
          "author": {"name": "Pixelguin"}
        }"#;
        let a: Asset = serde_json::from_str(json).unwrap();
        assert_eq!(
            a.dimensions(),
            None,
            "0x0 must not be reported as real dimensions"
        );
        assert_eq!(a.width, 0);
        assert_eq!(a.extension_from_mime(), Some("ico"));
        assert!(a.preview_url().ends_with(".png"));
    }

    #[test]
    fn an_asset_survives_every_optional_field_going_missing() {
        // The property that matters: SteamGridDB dropping or renaming a field must not make
        // the whole response unreadable.
        let a: Asset = serde_json::from_str(r#"{"id": 1, "url": "https://x/y.png"}"#).unwrap();
        assert_eq!(a.id, 1);
        assert_eq!(a.preview_url(), "https://x/y.png");
        assert_eq!(a.dimensions(), None);
        assert_eq!(a.author.name, "");
        assert!(!a.is_flagged());
    }

    #[test]
    fn an_asset_without_a_url_is_rejected() {
        // Without a url there is nothing to apply, so this one really is fatal.
        assert!(serde_json::from_str::<Asset>(r#"{"id": 1}"#).is_err());
    }

    #[test]
    fn unknown_fields_are_ignored_so_api_additions_are_free() {
        let a: Asset =
            serde_json::from_str(r#"{"id":1,"url":"u","brand_new_field":{"a":[1,2]}}"#).unwrap();
        assert_eq!(a.id, 1);
    }

    #[test]
    fn parses_a_real_game_lookup_and_a_search_hit() {
        let game: Envelope<Game> = serde_json::from_str(
            r#"{"success":true,"data":{"id":17830,"name":"Portal 2",
                "release_date":1303084800,"types":["steam"],"verified":true}}"#,
        )
        .unwrap();
        let g = game.data.unwrap();
        assert_eq!(g.id, 17830);
        assert_eq!(g.name, "Portal 2");
        assert_eq!(g.types, ["steam"]);

        // A real search hit with an empty `types` array — must not be mistaken for missing.
        let hit: Game =
            serde_json::from_str(r#"{"id":1699,"name":"Portal: Still Alive","types":[]}"#).unwrap();
        assert!(hit.types.is_empty());
        assert!(!hit.verified);
    }

    #[test]
    fn a_short_page_does_not_mean_the_end_of_the_results() {
        // A page can come back with far fewer than `limit` items while hundreds remain. Any
        // logic that concludes "short page, therefore done" strands the rest of a large game's
        // artwork — which is exactly how a browser ends up showing 12 of 400 and stopping.
        let one: Asset =
            serde_json::from_str(r#"{"id":1,"url":"https://cdn2.steamgriddb.com/grid/x.png"}"#)
                .unwrap();
        let short = AssetPage {
            assets: vec![one],
            page: 0,
            total: 424,
            limit: 50,
        };
        assert_eq!(short.assets.len(), 1, "premise: this page really is short");
        assert!(
            short.has_more(),
            "a page far shorter than the limit must not end pagination"
        );
    }

    #[test]
    fn has_more_is_derived_from_total_not_from_a_short_page() {
        // 424 total at 50 per page: pages 0..7 have more, page 8 does not.
        let page = |p: u32| AssetPage {
            assets: Vec::new(),
            page: p,
            total: 424,
            limit: 50,
        };
        assert!(page(0).has_more());
        assert!(page(7).has_more());
        assert!(!page(8).has_more(), "9 * 50 = 450 >= 424");

        // Exactly-full last page must not offer a phantom next page.
        let exact = AssetPage {
            assets: Vec::new(),
            page: 1,
            total: 100,
            limit: 50,
        };
        assert!(!exact.has_more());

        // A zero limit must not divide by zero or loop forever.
        let degenerate = AssetPage {
            assets: Vec::new(),
            page: 0,
            total: 10,
            limit: 0,
        };
        assert!(!degenerate.has_more());
    }
}
