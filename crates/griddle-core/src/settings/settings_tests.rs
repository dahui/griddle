//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;

// Every test that uses this is `cfg(windows)`, because storing a key needs DPAPI.
#[cfg(windows)]
const FAKE_KEY: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn defaults_round_trip_through_json() {
    let s = Settings::default();
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
}

#[test]
fn an_empty_object_loads_as_defaults() {
    // Forward compatibility: a file from an older build, or a hand-edited one, must not
    // fail to parse and cost the user every setting they have.
    let s: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(s, Settings::default());
}

#[test]
fn unknown_fields_from_a_newer_build_are_ignored() {
    // `live_apply` is here on purpose: it was a real field until live apply stopped being
    // optional, so any existing settings file still has it. A removed field must be as
    // harmless as one from the future.
    let s: Settings = serde_json::from_str(
        r#"{"live_apply": true, "library_scope": "all", "some_future_field": [1,2,3]}"#,
    )
    .unwrap();
    assert_eq!(s.library_scope, LibraryScope::All);
}

#[test]
fn a_missing_file_loads_as_defaults_without_creating_anything() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));
    assert_eq!(store.load().unwrap(), Settings::default());
    assert!(!store.path().exists(), "load must not create the file");
}

#[test]
fn save_then_load_preserves_everything() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("nested").join("settings.json"));

    let mut s = Settings {
        library_scope: LibraryScope::All,
        library_sort: LibrarySort::RecentlyPlayed,
        ..Default::default()
    };
    let _ = s.zoom.insert("Capsule".into(), 1.75);
    let _ = s.game_overrides.insert(
        620,
        GameOverride {
            id: 17830,
            name: Some("Portal 2".into()),
        },
    );
    s.filters = Some(FilterState {
        untagged: true,
        humor: true,
        styles: vec!["alternate".into()],
        dimensions: vec!["600x900".into()],
        animated: true,
        statik: true,
        ..Default::default()
    });
    s.tabs.order = vec!["Hero".into(), "Capsule".into()];

    store.save(&s).unwrap();
    assert_eq!(store.load().unwrap(), s);
    assert!(store.path().is_file(), "parent directories must be created");
}

#[test]
fn filter_state_serialises_static_not_statik() {
    // The field cannot be called `static` in Rust, so the JSON key is a rename — and the
    // TypeScript side sends `static`. A mismatch here would not fail anything loudly: serde
    // would simply take the default and the user's static/animated choice would vanish on
    // every reload.
    let json = serde_json::to_string(&FilterState {
        statik: true,
        ..Default::default()
    })
    .unwrap();

    assert!(json.contains("\"static\":true"), "{json}");
    assert!(!json.contains("statik"), "{json}");

    // The other direction, which is the one that actually breaks: a payload written by the
    // frontend must deserialise.
    let back: FilterState = serde_json::from_str(r#"{"static":true,"animated":false}"#).unwrap();
    assert!(back.statik);
    assert!(!back.animated);
}

#[test]
fn a_pre_m4_per_type_filter_map_is_carried_across_not_read_as_all_false() {
    // The dangerous migration. Filters were stored per asset type and are one shared set
    // now. Without the shim serde reads the old map as a `FilterState` with every field
    // missing — all-`false`, which is not the app's defaults, and looks to the user like
    // they had switched every content filter off themselves.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(
        store.path(),
        r#"{"version":1,"filters":{
             "grid_p":{"untagged":true,"humor":true,"styles":["alternate"],"animated":true,"static":true},
             "hero":{"untagged":false}
           }}"#,
    )
    .unwrap(); // boundary-ok: test fixture

    let loaded = store.load().unwrap();
    let Some(filters) = loaded.filters else {
        panic!("the old per-type map must carry across, not vanish");
    };

    // Premise and behaviour together: the carried values are grid_p's, and they are the
    // ones that would have been lost. All-false would satisfy none of these.
    assert!(filters.untagged);
    assert!(filters.humor);
    assert!(filters.statik);
    assert_eq!(filters.styles, vec!["alternate".to_owned()]);
}

#[test]
fn the_current_flat_filter_shape_is_not_mistaken_for_the_old_map() {
    // The control for the shim's discriminator. A current `FilterState` has booleans and
    // arrays in it; the old shape had objects. Both must round-trip correctly, or the shim
    // would quietly eat every filter the user sets from now on.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(
        store.path(),
        r#"{"version":1,"filters":{"untagged":false,"adult":true,"styles":["blurred"],"static":true}}"#,
    )
    .unwrap(); // boundary-ok: test fixture

    let filters = store.load().unwrap().filters.unwrap();
    assert!(!filters.untagged);
    assert!(filters.adult);
    assert!(filters.statik);
    assert_eq!(filters.styles, vec!["blurred".to_owned()]);
}

#[test]
fn an_absent_filter_key_stays_none_rather_than_becoming_all_false() {
    // `None` means "never customised", which is what lets the frontend apply its own
    // defaults. Collapsing it to `FilterState::default()` here would silently turn every
    // content filter off for a first-run user.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));
    std::fs::write(store.path(), r#"{"version":1}"#).unwrap(); // boundary-ok: test fixture
    assert_eq!(store.load().unwrap().filters, None);
}

#[test]
fn a_pre_m4_bare_override_id_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(
        store.path(),
        r#"{"version":1,"game_overrides":{"620":17830,"440":{"id":123,"name":"TF2"}}}"#,
    )
    .unwrap(); // boundary-ok: test fixture

    let loaded = store.load().unwrap();
    // The old bare-id form keeps working, with no name to show.
    assert_eq!(
        loaded.game_overrides.get(&620),
        Some(&GameOverride {
            id: 17830,
            name: None
        }),
    );
    // The control: the current form parses too, so the shim is not swallowing everything.
    assert_eq!(
        loaded.game_overrides.get(&440),
        Some(&GameOverride {
            id: 123,
            name: Some("TF2".into())
        }),
    );
}

#[test]
fn an_older_settings_file_without_the_m4_keys_still_loads() {
    // `#[serde(default)]` is what makes this true, and it is easy to lose. A settings file
    // written before the library scope existed must not fail to load.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));
    std::fs::write(store.path(), r#"{"version":1,"zoom":{"Hero":2.0}}"#).unwrap(); // boundary-ok: test fixture

    let loaded = store.load().unwrap();
    assert_eq!(
        loaded.zoom.get("Hero"),
        Some(&2.0),
        "the keys that were present must survive"
    );
    assert_eq!(loaded.library_scope, LibraryScope::Installed);
    assert_eq!(loaded.library_sort, LibrarySort::Name);
}

#[test]
fn save_leaves_no_temp_file_behind() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));
    store.save(&Settings::default()).unwrap();

    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("sgdbtmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn a_corrupt_file_is_preserved_rather_than_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(&path, b"{ this is not json").unwrap();
    let store = Store::at(&path);

    assert!(store.load().is_err(), "load must surface the problem");

    let recovered = store.load_or_default();
    assert_eq!(recovered, Settings::default());

    let aside = dir.path().join("settings.json.corrupt");
    assert!(aside.is_file(), "the unreadable file must be kept");
    assert_eq!(
        std::fs::read(&aside).unwrap(),
        b"{ this is not json",
        "and kept verbatim — it may be the only copy of the user's key"
    );
}

// -- key handling. DPAPI is Windows-only, so these are too. -------------------------

#[cfg(windows)]
#[test]
fn the_api_key_is_never_written_in_plaintext() {
    // The single most important assertion in this module: read the bytes actually on disk
    // and confirm the secret is not among them.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));

    let mut s = Settings::default();
    s.set_api_key(&ApiKey::new(FAKE_KEY).unwrap()).unwrap();
    store.save(&s).unwrap();

    let on_disk = std::fs::read(store.path()).unwrap();
    assert!(
        on_disk
            .windows(FAKE_KEY.len())
            .all(|w| w != FAKE_KEY.as_bytes()),
        "the API key was written in plaintext"
    );
    // And it really did store something.
    assert!(String::from_utf8_lossy(&on_disk).contains("api_key_protected"));
}

#[cfg(windows)]
#[test]
fn a_stored_key_round_trips_through_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));

    let mut s = Settings::default();
    assert!(!s.has_api_key());
    assert_eq!(s.api_key().unwrap(), None);

    s.set_api_key(&ApiKey::new(FAKE_KEY).unwrap()).unwrap();
    assert!(s.has_api_key());
    store.save(&s).unwrap();

    let loaded = store.load().unwrap();
    assert_eq!(loaded.api_key().unwrap().unwrap().expose(), FAKE_KEY);
}

#[cfg(windows)]
#[test]
fn clearing_the_key_removes_it_from_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));

    let mut s = Settings::default();
    s.set_api_key(&ApiKey::new(FAKE_KEY).unwrap()).unwrap();
    s.clear_api_key();
    store.save(&s).unwrap();

    assert!(!store.load().unwrap().has_api_key());
    let text = String::from_utf8(std::fs::read(store.path()).unwrap()).unwrap();
    assert!(text.contains("\"api_key_protected\": null"), "{text}");
}

#[cfg(windows)]
#[test]
fn an_undecryptable_key_does_not_take_the_rest_of_the_settings_with_it() {
    // A settings file copied from another Windows account. The user should lose the key,
    // not every preference they have set.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().join("settings.json"));

    let mut s = Settings {
        library_scope: LibraryScope::All,
        ..Default::default()
    };
    s.set_api_key(&ApiKey::new(FAKE_KEY).unwrap()).unwrap();
    let _ = s.zoom.insert("Hero".into(), 2.0);
    store.save(&s).unwrap();

    // Corrupt only the ciphertext.
    let mut damaged = store.load().unwrap();
    damaged.api_key_protected = Some(base64::encode(b"not a real dpapi blob"));
    store.save(&damaged).unwrap();

    let loaded = store.load().unwrap();
    assert_eq!(
        loaded.library_scope,
        LibraryScope::All,
        "settings must still load"
    );
    assert_eq!(loaded.zoom.get("Hero"), Some(&2.0));
    assert!(
        loaded.api_key().is_err(),
        "but the key itself must report a problem"
    );
}
