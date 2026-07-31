//! `userdata/<accountid>/config/localconfig.vdf` — the closest offline proxy for "my library".
//!
//! **READ-ONLY.** Steam owns this file and rewrites it on exit, exactly like `shortcuts.vdf`.
//! This module contains no write and must never acquire a `boundary-ok:` annotation.
//!
//! # Why this file, and what it is not
//!
//! There is **no offline list of games you own**. `licensecache` is encrypted, and this project
//! deliberately does not use the Steam Web API. What this file has is every app the client has
//! stored local config for — in practice, everything played or configured. On this box that is
//! **518 appids against 51 installed `appmanifest` files**, which is the difference between a
//! library list and a list of what happens to be on disk right now.
//!
//! Call it "all games" in the UI, but do not call it ownership: it will miss something you own
//! and never launched, and it can hold something you no longer have a license for.
//!
//! # The finder predicate
//!
//! `UserLocalConfigStore` → `Software` → `Valve` → `Steam` → `apps`, each looked up
//! case-insensitively. **A child is a Steam app iff its key parses as `u32` and its value is a
//! map.** `[VERIFIED-BOX 2026-07-30: 519 children, 0 scalar siblings]`
//!
//! # 🔴 One key is negative, and it is a non-Steam shortcut
//!
//! The 519th key on this box is `-246118299` — the **signed** form of `0xF1548865`, the
//! EmulationStationDE shortcut whose appid CLAUDE.md already records. `shortcuts.vdf` is
//! authoritative for those, so this file's copy is skipped; including it would produce a
//! duplicate row in the library under a different name. `u32::from_str` refuses it for free,
//! which is the behaviour we want and is why there is no explicit sign check.
//!
//! # 🔴 `LastPlayed` has a sentinel that is not a date
//!
//! Eight entries here read `86400` — 1970-01-02, one day after the Unix epoch, and about
//! thirty-three years before Steam existed. Treating those as real timestamps puts eight games
//! at the top of a "recently played" sort in 1970. Anything at or below
//! [`STEAM_LAUNCH_EPOCH`] is reported as "never played" instead.

use crate::appid::AppId;
use crate::steam::locate::SteamInstall;
use crate::vdf::text;
use std::path::PathBuf;

/// Steam's public launch, 2003-09-12. No genuine `LastPlayed` predates it.
///
/// Used as a floor rather than testing for the exact `86400` sentinel, because `0` appears too
/// (6 entries here) and any other pre-Steam value would be just as wrong.
pub const STEAM_LAUNCH_EPOCH: u64 = 1_063_324_800;

/// The nested path to the app map. Each segment is matched case-insensitively.
const APPS_PATH: [&str; 5] = ["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: text::Error,
    },
}

/// One app Steam holds local config for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRecord {
    pub app_id: AppId,
    /// Unix seconds. `None` when absent (6 of 519 here) or before Steam existed (8 more).
    pub last_played: Option<u64>,
    pub playtime_minutes: Option<u32>,
}

/// Every app in `localconfig.vdf`, in file order.
pub fn known_apps(install: &SteamInstall, account_id: u32) -> Result<Vec<AppRecord>, Error> {
    let path = install.localconfig_vdf(account_id);
    let raw = std::fs::read_to_string(&path).map_err(|source| Error::Read {
        path: path.clone(),
        source,
    })?;
    let doc = text::parse(&raw).map_err(|source| Error::Parse {
        path: path.clone(),
        source,
    })?;

    // Walk the nested path. A missing level is not an error — an account that has never launched
    // anything legitimately has no `apps` map, and that is an empty library, not a failure.
    let mut entries: &[text::Entry] = &doc.entries;
    for segment in APPS_PATH {
        match text::get(entries, segment).and_then(text::Value::as_map) {
            Some(next) => entries = next,
            None => {
                tracing::debug!(segment, "localconfig.vdf has no {segment} map");
                return Ok(Vec::new());
            }
        }
    }

    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        // Skips two things at once: scalar siblings (the documented KV1 hazard) and the
        // negative key belonging to a non-Steam shortcut.
        let Ok(id) = entry.key.parse::<u32>() else {
            continue;
        };
        let Some(fields) = entry.value.as_map() else {
            continue;
        };
        out.push(AppRecord {
            app_id: AppId::new(id),
            last_played: read_u64(fields, "LastPlayed").filter(|t| *t > STEAM_LAUNCH_EPOCH),
            playtime_minutes: read_u64(fields, "Playtime").and_then(|v| u32::try_from(v).ok()),
        });
    }
    Ok(out)
}

/// [`known_apps`], reduced to an empty list on any failure, with the reason logged.
///
/// Mirrors `AppTypes::load_or_none`. A missing or unreadable `localconfig.vdf` must degrade the
/// "all games" scope to "installed only", never fail the whole library load — the installed list
/// comes from somewhere else entirely and is still perfectly good.
pub fn known_apps_or_empty(install: &SteamInstall, account_id: u32) -> Vec<AppRecord> {
    match known_apps(install, account_id) {
        Ok(apps) => apps,
        Err(e) => {
            tracing::warn!(error = %e, "localconfig.vdf unavailable; 'all games' will show only installed apps");
            Vec::new()
        }
    }
}

fn read_u64(entries: &[text::Entry], key: &str) -> Option<u64> {
    text::get(entries, key)?.as_str()?.trim().parse().ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    /// Wrap an `apps` body in the five levels of nesting the real file uses.
    fn wrap(apps_body: &str) -> String {
        format!(
            r#"
"UserLocalConfigStore"
{{
    "Software"
    {{
        "Valve"
        {{
            "Steam"
            {{
                "apps"
                {{
{apps_body}
                }}
            }}
        }}
    }}
}}
"#
        )
    }

    fn parse_apps(body: &str) -> Vec<AppRecord> {
        let tmp = tempfile::tempdir().unwrap();
        let install = SteamInstall::at(tmp.path());
        let path = install.localconfig_vdf(1);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap(); // boundary-ok: test fixture
        std::fs::write(&path, wrap(body)).unwrap(); // boundary-ok: test fixture
        known_apps(&install, 1).unwrap()
    }

    #[test]
    fn reads_appids_playtimes_and_last_played() {
        let apps = parse_apps(
            r#"
                "220" { "LastPlayed" "1732255181" "Playtime" "442" }
                "440" { "LastPlayed" "1365058800" "Playtime" "1914" }
        "#,
        );

        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].app_id, AppId::new(220));
        assert_eq!(apps[0].last_played, Some(1_732_255_181));
        assert_eq!(apps[0].playtime_minutes, Some(442));
    }

    #[test]
    fn a_negative_key_is_skipped_because_it_is_a_non_steam_shortcut() {
        // -246118299 == 0xF1548865, the EmulationStationDE shortcut. shortcuts.vdf owns those,
        // and taking this one too would duplicate the row.
        let body = r#"
                "620" { "LastPlayed" "1700000000" "Playtime" "466" }
                "-246118299" { "LastPlayed" "1700000001" }
        "#;

        // Premise, asserted before the behaviour: the fixture really does contain two children
        // and one of them really is that negative key. Without this the test passes just as
        // happily against a fixture where the negative key was never written.
        let doc = text::parse(&wrap(body)).unwrap();
        let mut entries: &[text::Entry] = &doc.entries;
        for segment in APPS_PATH {
            entries = text::get(entries, segment).unwrap().as_map().unwrap();
        }
        assert_eq!(entries.len(), 2, "premise: two children in the apps map");
        assert!(
            entries.iter().any(|e| e.key == "-246118299"),
            "premise: the negative key is present in the fixture",
        );

        let apps = parse_apps(body);
        assert_eq!(apps.len(), 1, "the shortcut's entry must be skipped");
        assert_eq!(apps[0].app_id, AppId::new(620));
    }

    #[test]
    fn pre_steam_last_played_values_are_reported_as_never_played() {
        // 86400 is 1970-01-02 and appears 8 times on this box; 0 appears 6 more. Both would
        // otherwise sort to the top of "recently played".
        let apps = parse_apps(
            r#"
                "1" { "LastPlayed" "86400" }
                "2" { "LastPlayed" "0" }
                "3" { "LastPlayed" "1732255181" }
        "#,
        );

        assert_eq!(
            apps[0].last_played, None,
            "the 86400 sentinel is not a date"
        );
        assert_eq!(apps[1].last_played, None);
        // The control: a real timestamp still survives, so the filter is not simply eating
        // every value.
        assert_eq!(apps[2].last_played, Some(1_732_255_181));
    }

    #[test]
    fn an_app_with_no_playtime_fields_is_still_an_app() {
        // Appid 7 on this box is exactly this shape: a map holding only a `cloud` sub-map.
        // Dropping it would quietly shrink the library.
        let apps = parse_apps(
            r#"
                "7" { "cloud" { "last_sync_state" "synchronized" } }
        "#,
        );
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].app_id, AppId::new(7));
        assert_eq!(apps[0].last_played, None);
        assert_eq!(apps[0].playtime_minutes, None);
    }

    #[test]
    fn a_scalar_sibling_among_the_numbered_keys_is_skipped() {
        // Not observed in this file, but it is the documented KV1 hazard that libraryfolders.vdf
        // does exhibit, and it costs one line to be immune to it.
        let apps = parse_apps(
            r#"
                "620" { "Playtime" "466" }
                "somescalar" "1234"
                "770" { "Playtime" "12" }
        "#,
        );
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].app_id, AppId::new(620));
        assert_eq!(apps[1].app_id, AppId::new(770));
    }

    #[test]
    fn a_missing_file_degrades_to_an_empty_list_rather_than_failing_the_library() {
        let tmp = tempfile::tempdir().unwrap();
        let install = SteamInstall::at(tmp.path());
        assert!(
            known_apps(&install, 1).is_err(),
            "premise: the file is absent"
        );
        assert!(known_apps_or_empty(&install, 1).is_empty());
    }

    #[test]
    fn an_account_that_has_launched_nothing_has_no_apps_map() {
        let tmp = tempfile::tempdir().unwrap();
        let install = SteamInstall::at(tmp.path());
        let path = install.localconfig_vdf(1);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap(); // boundary-ok: test fixture
        std::fs::write(&path, "\"UserLocalConfigStore\"\n{\n}\n").unwrap(); // boundary-ok: test fixture

        // A missing level is an empty library, not an error — the file is well-formed.
        assert_eq!(known_apps(&install, 1).unwrap(), Vec::new());
    }
}
