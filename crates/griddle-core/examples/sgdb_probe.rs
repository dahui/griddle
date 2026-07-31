//! Exercise `sgdb::client` against the live SteamGridDB API.
//!
//! ```powershell
//! $env:SGDB_API_KEY = "<your key>"
//! cargo run -p griddle-core --example sgdb_probe
//! ```
//!
//! Read-only: it fetches metadata and one thumbnail, and writes nothing anywhere.
//!
//! # Why this exists beyond the unit tests
//!
//! The wiremock tests prove the client behaves correctly against responses *we* wrote. They
//! cannot tell us whether the **dimension filter values are real**. Those are a closed set —
//! `?dimensions=1x1` is an HTTP 400, not an empty result `[VERIFIED-BOX 2026-07-30]` — and only
//! `600x900` had been confirmed by hand. Every other value in `query::Dimensions` was inferred,
//! and an inferred one that is wrong turns a whole artwork tab into an error the moment a user
//! opens it.
//!
//! 🔑 The key is read from the environment and **never** from a file in this repo.

use griddle_core::appid::AppId;
use griddle_core::grid::names::AssetType;
use griddle_core::sgdb::{ApiKey, AssetQuery, Client, Dimensions, Target};

const PORTAL_2: u32 = 620;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let Ok(raw) = std::env::var("SGDB_API_KEY") else {
        eprintln!("set SGDB_API_KEY first (it is a per-user secret; never commit it)");
        std::process::exit(2);
    };
    let key = match ApiKey::new(raw) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("bad key: {e}");
            std::process::exit(2);
        }
    };
    // Printed as a fingerprint. If this ever shows the whole key, `ApiKey`'s Debug is broken.
    println!("key: {key:?}");

    let client = match Client::new(key) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("client: {e}");
            std::process::exit(1);
        }
    };

    println!("\n== validate_key ==");
    match client.validate_key().await {
        Ok(()) => println!("  ok"),
        Err(e) => {
            eprintln!("  {e}");
            std::process::exit(1);
        }
    }

    println!("\n== games/steam/{PORTAL_2} ==");
    match client.game_by_steam_appid(AppId::new(PORTAL_2)).await {
        Ok(Some(g)) => println!("  #{} {} (verified={})", g.id, g.name, g.verified),
        Ok(None) => println!("  not on SteamGridDB"),
        Err(e) => println!("  {e}"),
    }

    println!("\n== a game SteamGridDB has never heard of ==");
    match client.game_by_steam_appid(AppId::new(999_999_999)).await {
        Ok(None) => println!("  None — correctly not treated as an error"),
        Ok(Some(g)) => println!("  unexpected hit: {g:?}"),
        Err(e) => println!("  UNEXPECTED ERROR: {e}"),
    }

    println!("\n== search ==");
    match client.search("portal 2").await {
        Ok(hits) => {
            for g in hits.iter().take(4) {
                println!("  #{:<7} {}", g.id, g.name);
            }
        }
        Err(e) => println!("  {e}"),
    }

    // The real point of this probe.
    println!("\n== every asset type, with its dimension filter ==");
    let mut failures = 0;
    for t in [
        AssetType::Capsule,
        AssetType::Header,
        AssetType::Hero,
        AssetType::Logo,
        AssetType::Icon,
    ] {
        let Some((kind, query)) = AssetQuery::for_asset_type(t) else {
            continue;
        };
        let dims: Vec<&str> = query.dimensions.iter().map(|d| d.as_str()).collect();
        match client
            .assets(kind, Target::Steam(AppId::new(PORTAL_2)), &query.limit(3))
            .await
        {
            Ok(page) => println!(
                "  {t:<13} {:<7} {:>5} total, {} returned  dims=[{}]",
                kind.path(),
                page.total,
                page.assets.len(),
                dims.join(",")
            ),
            Err(e) => {
                failures += 1;
                println!(
                    "  {t:<13} {:<7} FAILED: {e}  dims=[{}]",
                    kind.path(),
                    dims.join(",")
                );
            }
        }
    }

    // Each dimension value on its own. In a comma-separated list a bad value is masked by the
    // valid ones next to it, which is exactly how `512x512` and `1024x1024` survived review as
    // icon dimensions until this probe split them apart.
    println!("\n== each dimension value individually ==");
    // Every variant of `Dimensions`, so removing one from the enum removes it from the probe
    // and adding one without probing it is impossible to do quietly.
    let checks: &[(&str, Dimensions)] = &[
        ("grids", Dimensions::D600x900),
        ("grids", Dimensions::D342x482),
        ("grids", Dimensions::D660x930),
        ("grids", Dimensions::D460x215),
        ("grids", Dimensions::D920x430),
        ("heroes", Dimensions::D1920x620),
        ("heroes", Dimensions::D3840x1240),
        ("heroes", Dimensions::D1600x650),
    ];
    for (endpoint, dim) in checks {
        let kind = match *endpoint {
            "grids" => griddle_core::sgdb::AssetKind::Grid,
            _ => griddle_core::sgdb::AssetKind::Hero,
        };
        let q = AssetQuery {
            dimensions: vec![*dim],
            ..Default::default()
        }
        .limit(1);
        match client
            .assets(kind, Target::Steam(AppId::new(PORTAL_2)), &q)
            .await
        {
            Ok(p) => println!(
                "  {endpoint:<7} {:<10} ok ({} total)",
                dim.as_str(),
                p.total
            ),
            Err(e) => {
                failures += 1;
                println!("  {endpoint:<7} {:<10} FAILED: {e}", dim.as_str());
            }
        }
    }

    println!("\n== pagination ==");
    let q = AssetQuery::default().limit(5);
    match client
        .assets(
            griddle_core::sgdb::AssetKind::Grid,
            Target::Steam(AppId::new(PORTAL_2)),
            &q.clone().page(0),
        )
        .await
    {
        Ok(first) => {
            println!(
                "  page 0: {} assets, total {}, has_more={}",
                first.assets.len(),
                first.total,
                first.has_more()
            );
            match client
                .assets(
                    griddle_core::sgdb::AssetKind::Grid,
                    Target::Steam(AppId::new(PORTAL_2)),
                    &q.page(1),
                )
                .await
            {
                Ok(second) => {
                    println!("  page 1: {} assets", second.assets.len());
                    // If `page` were ignored, both pages would be identical — which would make
                    // infinite scroll repeat the first five results forever.
                    let same =
                        first.assets.first().map(|a| a.id) == second.assets.first().map(|a| a.id);
                    println!(
                        "  {}",
                        if same {
                            "🔴 page 1 == page 0 — the `page` parameter is NOT honoured"
                        } else {
                            "✅ page 1 differs from page 0 — pagination works"
                        }
                    );
                    if same {
                        failures += 1;
                    }
                }
                Err(e) => println!("  page 1 failed: {e}"),
            }
        }
        Err(e) => println!("  {e}"),
    }

    println!("\n== download a thumbnail from the CDN ==");
    let q = AssetQuery::default().limit(1);
    match client
        .assets(
            griddle_core::sgdb::AssetKind::Grid,
            Target::Steam(AppId::new(PORTAL_2)),
            &q,
        )
        .await
    {
        Ok(page) => match page.assets.first() {
            Some(a) => match client.download(a.preview_url()).await {
                Ok(bytes) => println!(
                    "  {} -> {} bytes, magic {:02x?}",
                    a.preview_url(),
                    bytes.len(),
                    &bytes[..bytes.len().min(4)]
                ),
                Err(e) => println!("  download failed: {e}"),
            },
            None => println!("  no assets to download"),
        },
        Err(e) => println!("  {e}"),
    }

    println!();
    if failures == 0 {
        println!("all probes passed");
    } else {
        println!("🔴 {failures} probe(s) failed — see above");
        std::process::exit(1);
    }
}
