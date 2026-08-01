//! The game list, assembled from Steam's three offline sources: installed manifests,
//! `localconfig.vdf`, and `shortcuts.vdf`.

use super::{Res, first_existing, parse_asset_type, path_string};
use crate::state::AppState;
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
    /// load, which is how it was reported.
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
/// 🔴 **An app `localconfig.vdf` knows and `appinfo.vdf` does not is one the account no longer
/// holds** — a refunded purchase, or a demo or beta Steam has withdrawn. Confirmed against a real
/// library: all 29 such apps there were exactly that. `[VERIFIED-BOX 2026-07-31]` `localconfig` is
/// a record of what was *configured*, never of what is *owned* — there is no offline ownership
/// list, `licensecache` being encrypted — and absence from appinfo is the closest signal there is.
///
/// 🔴 `appinfo_loaded` is the whole safety story, which is why it is a parameter rather than
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
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
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
        // 🔴 The guard that keeps the rule above safe. With no readable `appinfo.vdf` every app
        // looks nameless, so dropping on that alone would cut the All-games scope down to
        // installed apps and shortcuts — "some of my games are missing", which is the hardest
        // kind of bug for a user to report and the one this codebase most wants to avoid.
        assert!(!is_disowned(false, None));

        // And it must not depend on the name being absent: with no appinfo there is no opinion
        // to have, whatever else is true.
        assert!(!is_disowned(false, Some("Portal 2")));
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
