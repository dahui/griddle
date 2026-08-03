//! The game list: three offline sources, plus one live upgrade when Steam is running.
//!
//! # The ladder
//!
//! 1. **Installed manifests** — `appmanifest_*.acf`. Authoritative for what is on disk.
//! 2. **`localconfig.vdf`** — the offline "all games" source, and where playtimes come from.
//! 3. **`shortcuts.vdf`** — non-Steam shortcuts, always shown whatever the scope.
//! 4. **Steam's own `collectionStore`, over CDP** — only for the All-games scope, and only when
//!    Steam is up. See [`merge_live_apps`].
//!
//! Rungs 1–3 need nothing but files, which is what keeps the app useful with Steam closed. Rung 4
//! exists because the offline "all games" list is a **proxy**: `localconfig` records what was
//! configured on this PC, not what is owned, so it misses games bought and never launched here.
//! Measured on a real library, that is 391 apps and about 200 real games.

use super::{Res, first_existing, parse_asset_type, path_string};
use crate::state::AppState;
use griddle_core::appid::AppId;
use griddle_core::cdp::{Endpoint, SteamJs};
use griddle_core::grid::store::GridDir;
use griddle_core::settings::{LibraryScope, LibrarySort};
use griddle_core::steam::{LibraryCache, apptype, library, localconfig, shortcuts::Shortcuts};
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct LibraryEntry {
    pub app_id: u32,
    pub name: String,
    /// Whether [`name`](Self::name) is a real name or a stand-in built from the appid.
    ///
    /// Now a **degraded-mode signal only**: an app with no `appinfo.vdf` name is one the account
    /// no longer holds and is dropped from the list entirely, so the only rows that reach the UI
    /// unnamed are those from an unreadable `appinfo.vdf` (where nothing is dropped, by design)
    /// and a shortcut with no `appname`.
    ///
    /// Kept rather than removed because that is exactly when the UI most needs to explain
    /// itself: a synthesised name with nothing to say for it reads as artwork that failed to
    /// load rather than as a label.
    pub named: bool,
    /// `"steam"` or `"shortcut"`.
    pub kind: &'static str,
    pub app_type: Option<String>,
    /// Absolute path to existing custom art for this asset type, if any. Rendered through
    /// Tauri's `asset:` protocol.
    pub current_art: Option<String>,
    /// Absolute path to *Steam's own* art for this slot, when there is one on disk.
    ///
    /// The second rung of the ladder the UI walks: custom art → this → Steam's CDN → a
    /// placeholder. Always `None` for shortcuts, which have no librarycache entry.
    pub steam_art: Option<String>,
    /// False for an app `localconfig.vdf` knows about that has no installed manifest.
    pub installed: bool,
    /// Unix seconds, absent when never played. See [`localconfig`] on the 1970 sentinel.
    pub last_played: Option<u64>,
    pub playtime_minutes: Option<u32>,
}

/// Whether an app that only `localconfig.vdf` remembers should be left out of the library.
///
/// **An app `localconfig.vdf` knows and `appinfo.vdf` does not is one the account no longer
/// holds** — a refunded purchase, or a demo or beta Steam has withdrawn. Confirmed against a real
/// library: all 29 such apps there were exactly that. `[VERIFIED-BOX 2026-07-31]` `localconfig` is
/// a record of what was *configured*, never of what is *owned* — there is no offline ownership
/// list, `licensecache` being encrypted — and absence from appinfo is the closest signal there is.
///
/// `appinfo_loaded` is the whole safety story, which is why it is a parameter rather than
/// something this function works out. When `appinfo.vdf` cannot be read, **every** app looks
/// nameless, and dropping on that would cut the All-games scope down to installed apps and
/// shortcuts — surfacing as "some of my games are missing", which this codebase treats as the
/// hardest kind of bug to report. No appinfo means no opinion.
///
/// Installed apps never reach this: they are named from their `appmanifest` and are already in
/// the map by the time `localconfig` is walked.
fn is_disowned(appinfo_loaded: bool, appinfo_name: Option<&str>) -> bool {
    appinfo_loaded && appinfo_name.is_none()
}

/// Whether a row survives being reconciled against Steam's live library list.
///
/// Only ever called when that list was fetched successfully — absence from a list that failed to
/// load means nothing, and treating it as an answer would empty the library the moment Steam was
/// closed.
///
/// **`installed` wins outright.** A game whose files are on disk is a fact, and no list gets to
/// contradict it. Steam listing everything the account holds is an assumption; the `.acf` on disk
/// is not. If those two ever disagree, showing a game the user can launch beats hiding it, which
/// is the same asymmetry the whole module runs on.
const fn keep_after_live_reconcile(installed: bool, in_live_list: bool) -> bool {
    installed || in_live_list
}

/// The library list, scoped to installed apps or to everything Steam knows about.
///
/// Shortcuts are never scoped away: a non-Steam shortcut is always "installed" in the only sense
/// that matters here, and hiding them behind a toggle they have nothing to do with would be
/// baffling.
///
/// `scope` and `sort` are **parameters, not reads of `Settings`**. Persisting a view preference
/// and reloading the list are separate round trips, so reading the stored value here would race
/// the write: picking "Recently played" reloaded the list before the setting had landed and it
/// came back in the old order, which looked like the control did nothing.
#[tauri::command]
pub async fn library(
    state: State<'_, AppState>,
    asset_type: String,
    scope: LibraryScope,
    sort: LibrarySort,
) -> Res<Vec<LibraryEntry>> {
    let asset = parse_asset_type(&asset_type)?;
    let ctx = state.steam()?;
    let grid = GridDir::new(ctx.install.grid_dir(ctx.account.id));
    let steam_art = LibraryCache::new(&ctx.install, ctx.app_types.as_ref());

    // Keyed by appid so the "all games" pass can skip anything already installed without a
    // linear scan, and so a duplicate appid across the two sources cannot produce two rows.
    let mut steam_apps: BTreeMap<u32, LibraryEntry> = BTreeMap::new();

    // Installed Steam apps. One corrupt manifest never empties the list.
    match library::installed_apps(&ctx.install) {
        Ok(apps) => {
            for app in apps.iter().filter(|a| a.is_fully_installed()) {
                if !apptype::include_in_library(ctx.app_types.as_ref(), app.app_id) {
                    continue;
                }
                let _ = steam_apps.insert(
                    app.app_id.get(),
                    LibraryEntry {
                        app_id: app.app_id.get(),
                        // The manifest name, not appinfo's: it is what the user installed, and
                        // it is present even for the apps appinfo has never heard of.
                        name: app.name.clone(),
                        named: true,
                        kind: "steam",
                        app_type: ctx
                            .app_types
                            .as_ref()
                            .and_then(|t| t.app_type(app.app_id))
                            .map(|t| t.label().to_owned()),
                        current_art: first_existing(&grid, app.app_id, asset),
                        steam_art: path_string(steam_art.resolve(app.app_id, asset)),
                        installed: true,
                        last_played: None,
                        playtime_minutes: None,
                    },
                );
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not enumerate installed apps"),
    }

    // `localconfig.vdf` serves two purposes: it is the "all games" source, and it carries the
    // playtimes for installed games too, so it is read either way.
    let known = localconfig::known_apps_or_empty(&ctx.install, ctx.account.id);
    let mut dropped = 0usize;
    for record in &known {
        let id = record.app_id;
        let appinfo_name = ctx.app_types.as_ref().and_then(|t| t.name(id));

        // Refunded games and withdrawn demos — see `is_disowned`. This reverses the project's
        // usual "unknown means show it" direction, deliberately and on the library owner's
        // say-so: these are not games missing from the list, they are games no longer in the
        // account, and every one of them is unnamed and artless.
        let disowned = is_disowned(ctx.app_types.is_some(), appinfo_name);
        if scope == LibraryScope::All && disowned && !steam_apps.contains_key(&id.get()) {
            dropped += 1;
            continue;
        }

        if scope == LibraryScope::All
            && !steam_apps.contains_key(&id.get())
            && apptype::include_in_library(ctx.app_types.as_ref(), id)
        {
            let _ = steam_apps.insert(
                id.get(),
                LibraryEntry {
                    app_id: id.get(),
                    // Only reachable when `appinfo.vdf` could not be read at all, since a
                    // nameless app is otherwise dropped above. `Steam app <id>` reads as a label
                    // rather than as art that failed to load, which is how `Unknown app <id>`
                    // was reported. The id stays *inside* the name on purpose: the filter box
                    // matches on `name`, so the row is still findable by typing the number.
                    name: appinfo_name
                        .map_or_else(|| format!("Steam app {}", id.get()), str::to_owned),
                    named: appinfo_name.is_some(),
                    kind: "steam",
                    app_type: ctx
                        .app_types
                        .as_ref()
                        .and_then(|t| t.app_type(id))
                        .map(|t| t.label().to_owned()),
                    current_art: first_existing(&grid, id, asset),
                    steam_art: path_string(steam_art.resolve(id, asset)),
                    installed: false,
                    last_played: None,
                    playtime_minutes: None,
                },
            );
        }
        // Merge playtimes onto whichever group the app landed in.
        if let Some(entry) = steam_apps.get_mut(&id.get()) {
            entry.last_played = record.last_played;
            entry.playtime_minutes = record.playtime_minutes;
        }
    }

    // The tripwire for the paragraph above. If this ever starts hiding real games, the count is
    // the first place to look — and a count that suddenly jumps is the signal that `appinfo.vdf`
    // changed shape rather than that the account did.
    if dropped > 0 {
        tracing::info!(
            dropped,
            of = known.len(),
            "skipped apps absent from appinfo.vdf (refunded, or withdrawn demos and betas)"
        );
    }

    // The upgrade rung: everything above is what Steam's files remember, and it is a proxy that
    // misses ~200 real games. When Steam is up, its own library list knows them.
    if scope == LibraryScope::All {
        merge_live_apps(&state, &grid, &steam_art, asset, &mut steam_apps).await;
    }

    let mut entries: Vec<LibraryEntry> = steam_apps.into_values().collect();

    // Non-Steam shortcuts. Read-only here; the file is only written for icons.
    match Shortcuts::load_or_empty(ctx.install.shortcuts_vdf(ctx.account.id)) {
        Ok(sc) => {
            for s in sc.iter() {
                let Some(id) = s.app_id() else { continue };
                entries.push(LibraryEntry {
                    app_id: id.get(),
                    name: s.app_name().unwrap_or("(unnamed shortcut)").to_owned(),
                    // A shortcut with no `appname` is the same situation, from a different file.
                    named: s.app_name().is_some(),
                    kind: "shortcut",
                    app_type: None,
                    current_art: first_existing(&grid, id, asset),
                    // No librarycache entry exists for a non-Steam appid, so do not stat for one.
                    steam_art: None,
                    installed: true,
                    last_played: None,
                    playtime_minutes: None,
                });
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not read shortcuts.vdf"),
    }

    sort_entries(&mut entries, sort);
    Ok(entries)
}

/// Reconcile the offline list against the games the running Steam client actually holds.
///
/// It **adds** what `localconfig` never knew, and **removes** what the account no longer holds.
/// Both directions come from the same fetch, because both are the same question: what is in this
/// library right now?
///
/// # Why this exists
///
/// `localconfig.vdf` records what has been *configured* on this PC, never what is owned, so the
/// All-games scope has always missed games bought and never launched here. **Measured: 391 apps,
/// about 200 of them real games** `[VERIFIED-BOX 2026-08-02]`. Steam's own `collectionStore` has
/// them, and `griddle_core::cdp::SteamJs::library_apps` reads it.
///
/// # The pruning half, and the claim that made it necessary
///
/// `is_disowned` calls an app gone when `appinfo.vdf` cannot name it, on the documented
/// reasoning that *"Steam drops an app from appinfo once it stops being yours."* **That is false.**
/// Assassin's Creed Shadows, refunded, is present in `localconfig.vdf` **and** carries a name in
/// `appinfo.vdf`, so the predicate keeps it and it has been showing in the library the whole time.
/// `appinfo.vdf` is a global metadata cache, not a per-account one, and nothing obliges Steam to
/// evict from it.
///
/// The live list settles it directly: Steam does not list a refunded game in your library. So when
/// this fetch succeeds, **absence from it is the ownership signal**, and it is a far stronger one
/// than absence from appinfo. When it fails, the appinfo heuristic stays as the floor — weak, but
/// better than nothing, and it is all that exists offline.
///
/// **Installed apps are never pruned**, whatever the live list says. Files on disk are a fact, and
/// a list that could hide a game the user can launch right now is worse than one that shows a
/// refund. That asymmetry is the same one the rest of this module runs on.
///
/// # Every failure here is silent, and that is the design
///
/// **Steam being closed is the ordinary case, not an error.** The offline list is already
/// complete enough to use, so a connection failure, a missing store, or a Steam build that moved
/// `collectionStore` must all leave the list exactly as it was rather than surface a message
/// about a thing the user did not ask for and cannot act on. All three log at `debug` or `warn`
/// and return.
///
/// This is the same ladder shape as `apply_asset`: try live, fall back to files. The difference
/// is that here the fallback ran *first*, because it is the cheap one.
///
/// # What it deliberately does not do
///
/// - **It never replaces a row.** Anything already in the map came from an `appmanifest` or from
///   `localconfig`, both of which carry facts this list does not — installed state, playtimes —
///   and clobbering them to gain a `display_name` would trade information for nothing.
/// - **It never adds a shortcut.** Steam types those `1073741824`, and `shortcuts.vdf` owns their
///   name and icon further down. Adding one here would produce a duplicate row under Steam's
///   unsigned appid.
/// - **It applies the same type filter as the offline path**, through
///   `AppType::from_steam_enum` into the one `belongs_in_library` policy. On the measured library
///   that removes 181 Tools, 10 soundtracks and a video from the 869.
async fn merge_live_apps(
    state: &AppState,
    grid: &GridDir,
    steam_art: &LibraryCache<'_>,
    asset: griddle_core::grid::names::AssetType,
    steam_apps: &mut BTreeMap<u32, LibraryEntry>,
) {
    let mut js = match SteamJs::connect(&state.http, &Endpoint::default()).await {
        Ok((js, _)) => js,
        // Overwhelmingly "Steam is not running", which is not worth a warning on every load.
        Err(e) => {
            tracing::debug!(error = %e, "no live Steam library; using the offline list");
            return;
        }
    };

    let live = match js.library_apps().await {
        Ok(Some(apps)) => apps,
        // The store was reachable but not there. That *is* worth a warning: it means a Steam
        // build moved something, and the count below is how anyone would notice.
        Ok(None) => {
            tracing::warn!("Steam's collectionStore was not found; using the offline list");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "could not read Steam's library; using the offline list");
            return;
        }
    };

    let before = steam_apps.len();
    let mut skipped_type = 0usize;
    for app in &live {
        let ty = apptype::AppType::from_steam_enum(app.app_type);

        // `shortcuts.vdf` owns these, further down.
        if ty.label() == "Shortcut" {
            continue;
        }
        if !ty.belongs_in_library() {
            skipped_type += 1;
            continue;
        }
        let id = AppId::new(app.app_id);
        // The hardcoded blocklist still applies: it is the floor under both paths.
        if library::is_known_non_game(id) {
            continue;
        }

        // Only ever fills gaps. See the doc comment.
        let _ = steam_apps
            .entry(app.app_id)
            .or_insert_with(|| LibraryEntry {
                app_id: app.app_id,
                name: app.name.clone(),
                // Steam gave us the name it displays, so this is as named as a row gets.
                named: !app.name.is_empty(),
                kind: "steam",
                app_type: Some(ty.label().to_owned()),
                current_art: first_existing(grid, id, asset),
                steam_art: path_string(steam_art.resolve(id, asset)),
                // Not in an `appmanifest`, so not installed. An installed app was already inserted
                // by the pass above and is never reached here.
                installed: false,
                // By construction these are absent from `localconfig`, so there is no playtime to
                // have. They sort last under both play-based sorts, which is correct: never played.
                last_played: None,
                playtime_minutes: None,
            });
    }
    let added = steam_apps.len() - before;

    // The pruning half. `live_ids` is every appid Steam listed, *before* the type filter above —
    // a Tool the account owns is still owned, and keying the prune on the filtered set would drop
    // rows for the wrong reason.
    let live_ids: std::collections::HashSet<u32> = live.iter().map(|a| a.app_id).collect();
    let mut pruned: Vec<String> = Vec::new();
    steam_apps.retain(|id, entry| {
        let keep = keep_after_live_reconcile(entry.installed, live_ids.contains(id));
        if !keep {
            pruned.push(format!("{} ({id})", entry.name));
        }
        keep
    });

    // Named, not just counted. This removes rows a user can see, so "which ones" has to be
    // answerable from a log rather than by re-deriving it — and a name here is how anyone would
    // spot the day it starts eating games that are genuinely owned.
    if pruned.is_empty() {
        tracing::info!(
            live = live.len(),
            added,
            skipped_type,
            "merged Steam's live library list"
        );
    } else {
        tracing::info!(
            live = live.len(),
            added,
            skipped_type,
            pruned = pruned.len(),
            titles = %pruned.join(", "),
            "merged Steam's live library list; dropped apps the account no longer holds"
        );
    }
}

/// Order the list, always breaking ties by name.
///
/// The name tiebreak is what keeps the order stable: hundreds of apps share a `None` playtime,
/// and without it they would shuffle between loads for no visible reason.
fn sort_entries(entries: &mut [LibraryEntry], sort: LibrarySort) {
    // Case-insensitive, so "Portal" and "portal" sit together rather than in separate blocks.
    entries.sort_by(|a, b| match sort {
        LibrarySort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        // Descending, and never-played sorts last rather than first — `None` would otherwise
        // win a plain descending comparison on `Option`.
        LibrarySort::RecentlyPlayed => b
            .last_played
            .unwrap_or(0)
            .cmp(&a.last_played.unwrap_or(0))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        LibrarySort::MostPlayed => b
            .playtime_minutes
            .unwrap_or(0)
            .cmp(&a.playtime_minutes.unwrap_or(0))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_appinfo_has_never_heard_of_is_dropped() {
        // Refunded purchases and withdrawn demos keep their `localconfig` entry forever. They
        // are unnamed and artless, and the account no longer holds them.
        assert!(is_disowned(true, None));

        // The control: an app appinfo *can* name is kept. Without this the test would pass just
        // as well against a function that dropped everything.
        assert!(!is_disowned(true, Some("Portal 2")));
    }

    #[test]
    fn nothing_is_dropped_when_appinfo_could_not_be_read() {
        // The guard that keeps the rule above safe. With no readable `appinfo.vdf` every app
        // looks nameless, so dropping on that alone would cut the All-games scope down to
        // installed apps and shortcuts — "some of my games are missing", which is the hardest
        // kind of bug for a user to report and the one this codebase most wants to avoid.
        assert!(!is_disowned(false, None));

        // And it must not depend on the name being absent: with no appinfo there is no opinion
        // to have, whatever else is true.
        assert!(!is_disowned(false, Some("Portal 2")));
    }

    #[test]
    fn steams_live_list_drops_an_app_the_account_no_longer_holds() {
        // The case that prompted this: Assassin's Creed Shadows was refunded, but it sits in
        // `localconfig.vdf` *and* carries a name in `appinfo.vdf`, so `is_disowned` keeps it.
        // Steam's own library does not list it, and that is the signal to act on.
        assert!(!keep_after_live_reconcile(false, false));

        // The control. Without it this would pass just as well against a function that dropped
        // everything not installed, which would delete ~200 uninstalled games from the list.
        assert!(keep_after_live_reconcile(false, true));
    }

    #[test]
    fn an_installed_game_is_never_pruned_whatever_steam_says() {
        // Files on disk outrank the list. If Steam ever returns a partial library -- mid-sync,
        // offline mode, a family-sharing quirk -- the failure must not be "the game I am playing
        // vanished from Griddle".
        assert!(keep_after_live_reconcile(true, false));
        assert!(keep_after_live_reconcile(true, true));
    }

    #[test]
    fn the_library_sort_is_stable_and_never_played_sorts_last() {
        fn entry(name: &str, last_played: Option<u64>, minutes: Option<u32>) -> LibraryEntry {
            LibraryEntry {
                app_id: 1,
                name: name.to_owned(),
                named: true,
                kind: "steam",
                app_type: None,
                current_art: None,
                steam_art: None,
                installed: true,
                last_played,
                playtime_minutes: minutes,
            }
        }

        let mut entries = vec![
            entry("Zeta", Some(100), Some(5)),
            entry("alpha", None, None),
            entry("Beta", Some(900), Some(1)),
        ];

        sort_entries(&mut entries, LibrarySort::Name);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["alpha", "Beta", "Zeta"], "case-insensitive by name");

        sort_entries(&mut entries, LibrarySort::RecentlyPlayed);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // `None` must sort last. A naive descending sort on Option<u64> puts None first, which
        // would fill the top of "recently played" with games never launched.
        assert_eq!(names, ["Beta", "Zeta", "alpha"]);

        sort_entries(&mut entries, LibrarySort::MostPlayed);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Zeta", "Beta", "alpha"]);
    }

    #[test]
    fn every_entry_with_the_same_key_sorts_deterministically() {
        // Hundreds of apps share a `None` playtime. Without the name tiebreak their order would
        // depend on the input order and shuffle between loads for no visible reason.
        let make = || {
            vec![
                LibraryEntry {
                    app_id: 2,
                    name: "Bravo".to_owned(),
                    named: true,
                    kind: "steam",
                    app_type: None,
                    current_art: None,
                    steam_art: None,
                    installed: true,
                    last_played: None,
                    playtime_minutes: None,
                },
                LibraryEntry {
                    app_id: 1,
                    name: "Alpha".to_owned(),
                    named: true,
                    kind: "steam",
                    app_type: None,
                    current_art: None,
                    steam_art: None,
                    installed: false,
                    last_played: None,
                    playtime_minutes: None,
                },
            ]
        };

        let mut a = make();
        let mut b = make();
        b.reverse();
        sort_entries(&mut a, LibrarySort::RecentlyPlayed);
        sort_entries(&mut b, LibrarySort::RecentlyPlayed);

        let names = |v: &[LibraryEntry]| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&a), names(&b));
        assert_eq!(names(&a), ["Alpha", "Bravo"]);
    }
}
