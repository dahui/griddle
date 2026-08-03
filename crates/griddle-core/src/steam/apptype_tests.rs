//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;

#[test]
fn known_types_parse_case_insensitively() {
    assert_eq!(AppType::parse("Game"), AppType::Game);
    assert_eq!(AppType::parse("game"), AppType::Game);
    assert_eq!(AppType::parse("  TOOL  "), AppType::Tool);
    assert_eq!(AppType::parse("DLC"), AppType::Dlc);
}

#[test]
fn an_unknown_type_keeps_its_text_and_is_still_shown() {
    // The asymmetry that matters: a Steam category we have never seen must not silently
    // remove a game from the user's library.
    let t = AppType::parse("Holodeck");
    assert_eq!(t, AppType::Other("Holodeck".into()));
    assert_eq!(t.label(), "Holodeck");
    assert!(
        t.belongs_in_library(),
        "unknown must resolve toward showing"
    );
}

#[test]
fn tools_and_dlc_are_hidden_but_games_and_apps_are_not() {
    for t in ["Game", "Application", "Demo", "Beta", "Mod"] {
        assert!(AppType::parse(t).belongs_in_library(), "{t} should show");
    }
    for t in ["Tool", "DLC", "Music", "Video", "Config", "Hardware"] {
        assert!(!AppType::parse(t).belongs_in_library(), "{t} should hide");
    }
}

#[test]
fn with_no_appinfo_at_all_everything_but_the_blocklist_is_shown() {
    // The degraded path: it must still hide the redistributables we know by id, and must
    // not hide anything else.
    assert!(include_in_library(None, AppId::new(620)));
    assert!(include_in_library(None, AppId::new(1_004_640)));
    assert!(!include_in_library(None, AppId::new(228_980)));
}

/// A minimal v29 file, built the same way `vdf::appinfo`'s own tests do.
fn types_for(apps: &[(u32, &str)]) -> AppTypes {
    let strings = ["appinfo", "common", "type"];
    let idx = |s: &str| strings.iter().position(|x| *x == s).unwrap_or(0) as u32;

    let mut body = Vec::new();
    for (id, ty) in apps {
        let mut blob = Vec::new();
        blob.push(0x00);
        blob.extend(idx("appinfo").to_le_bytes());
        blob.push(0x00);
        blob.extend(idx("common").to_le_bytes());
        blob.push(0x01);
        blob.extend(idx("type").to_le_bytes());
        blob.extend(ty.as_bytes());
        blob.push(0);
        blob.push(0x08);
        blob.push(0x08);

        let mut payload = Vec::new();
        payload.extend(1u32.to_le_bytes());
        payload.extend(0u32.to_le_bytes());
        payload.extend(0u64.to_le_bytes());
        payload.extend([0u8; 20]);
        payload.extend(0u32.to_le_bytes());
        payload.extend([0u8; 20]);
        payload.extend(&blob);

        body.extend(id.to_le_bytes());
        body.extend((payload.len() as u32).to_le_bytes());
        body.extend(&payload);
    }
    body.extend(0u32.to_le_bytes());

    let mut table = Vec::new();
    table.extend((strings.len() as u32).to_le_bytes());
    for s in strings {
        table.extend(s.as_bytes());
        table.push(0);
    }

    let mut data = Vec::new();
    data.extend(appinfo::MAGIC_V29.to_le_bytes());
    data.extend(1u32.to_le_bytes());
    data.extend(((4 + 4 + 8 + body.len()) as i64).to_le_bytes());
    data.extend(&body);
    data.extend(&table);

    AppTypes {
        info: appinfo::parse(&data).unwrap(),
        path: PathBuf::from("appinfo.vdf"),
    }
}

#[test]
fn a_tool_is_hidden_once_appinfo_is_available() {
    let t = types_for(&[(620, "Game"), (1234, "Tool")]);
    assert_eq!(t.app_type(AppId::new(620)), Some(AppType::Game));
    assert_eq!(t.app_type(AppId::new(1234)), Some(AppType::Tool));

    assert!(include_in_library(Some(&t), AppId::new(620)));
    assert!(
        !include_in_library(Some(&t), AppId::new(1234)),
        "appinfo must be able to hide a tool the blocklist has never heard of"
    );
}

#[test]
fn an_app_missing_from_appinfo_is_still_shown() {
    let t = types_for(&[(620, "Game")]);
    assert_eq!(t.app_type(AppId::new(999_999)), None);
    assert!(
        include_in_library(Some(&t), AppId::new(999_999)),
        "absence from the cache must never hide an installed game"
    );
}

#[test]
fn the_blocklist_still_applies_even_when_appinfo_calls_it_a_game() {
    // Belt and braces: if appinfo ever labels a redistributable "Game", the id-based
    // floor must still win.
    let t = types_for(&[(228_980, "Game")]);
    assert!(!include_in_library(Some(&t), AppId::new(228_980)));
}

#[test]
fn steams_numeric_app_types_map_onto_the_same_policy_as_the_string_ones() {
    // The point of `from_steam_enum` is that the live list and the offline list cannot disagree
    // about what a thing is. Each of these ordinals was confirmed on a real library by the names
    // carrying it -- see the function's doc comment for which app proved which.
    assert_eq!(AppType::from_steam_enum(1), AppType::Game);
    assert_eq!(AppType::from_steam_enum(2), AppType::Application);
    assert_eq!(AppType::from_steam_enum(4), AppType::Tool);
    assert_eq!(AppType::from_steam_enum(2048), AppType::Video);
    assert_eq!(AppType::from_steam_enum(8192), AppType::Music);
    assert_eq!(AppType::from_steam_enum(65536), AppType::Beta);

    // And the decisions match what the string path already produces, which is the property that
    // actually matters. Asserting the enum alone would pass against a mapping that hid games.
    for (numeric, text) in [
        (1u32, "Game"),
        (2, "Application"),
        (4, "Tool"),
        (2048, "Video"),
        (8192, "Music"),
        (65536, "Beta"),
    ] {
        assert_eq!(
            AppType::from_steam_enum(numeric).belongs_in_library(),
            AppType::parse(text).belongs_in_library(),
            "numeric {numeric} and textual {text} must reach the same verdict"
        );
    }
}

#[test]
fn an_unrecognised_numeric_type_is_shown_not_hidden() {
    // Same failure direction as the rest of this module: a Steam release that invents a new
    // EAppType must not make those apps vanish from the library.
    let unknown = AppType::from_steam_enum(1 << 20);
    assert!(unknown.belongs_in_library());
    assert!(
        unknown.label().contains("1048576"),
        "an unknown value must survive verbatim into diagnostics, not collapse to a boolean"
    );
}

#[test]
fn shortcuts_are_typed_so_the_live_list_can_skip_them() {
    // `shortcuts.vdf` owns non-Steam shortcuts -- their name, their icon, and their signed
    // appid. Without this the live merge would add a second row under Steam's unsigned form.
    assert_eq!(AppType::from_steam_enum(1_073_741_824).label(), "Shortcut");
}
