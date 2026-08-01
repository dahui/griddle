//! Tests for the `appinfo.vdf` reader.
//!
//! Their own file because they were 575 of this module's 1200 lines: scrolling past the fixture
//! builders to reach the parser was the normal experience of opening it.

use super::*;

/// A KV tree, for fixtures whose shape is the point of the test.
enum Node<'a> {
    Str(&'a str, &'a str),
    Map(&'a str, Vec<Node<'a>>),
}

/// Build a one-app v29 file from a declarative `common` tree.
///
/// The string table is derived from the keys actually used, so a test can nest as deeply as
/// it likes without hand-maintaining indices — which is what made the existing fixtures too
/// laborious to extend, and is why the deeper paths went untested.
fn build_tree(app_id: u32, common: Vec<Node<'_>>) -> Vec<u8> {
    fn collect<'a>(nodes: &[Node<'a>], into: &mut Vec<&'a str>) {
        for n in nodes {
            match n {
                Node::Str(k, _) => into.push(k),
                Node::Map(k, kids) => {
                    into.push(k);
                    collect(kids, into);
                }
            }
        }
    }

    let mut strings: Vec<&str> = vec!["appinfo", "common"];
    collect(&common, &mut strings);
    strings.dedup();

    let idx = |s: &str| {
        strings
            .iter()
            .position(|x| *x == s)
            .map(|i| i as u32)
            .unwrap_or(0)
    };

    fn emit(nodes: &[Node<'_>], out: &mut Vec<u8>, idx: &dyn Fn(&str) -> u32) {
        for n in nodes {
            match n {
                Node::Str(k, v) => {
                    out.push(T_STRING);
                    out.extend(idx(k).to_le_bytes());
                    out.extend(v.as_bytes());
                    out.push(0);
                }
                Node::Map(k, kids) => {
                    out.push(T_MAP);
                    out.extend(idx(k).to_le_bytes());
                    emit(kids, out, idx);
                    out.push(T_END);
                }
            }
        }
    }

    let mut blob = Vec::new();
    blob.push(T_MAP);
    blob.extend(idx("appinfo").to_le_bytes());
    blob.push(T_MAP);
    blob.extend(idx("common").to_le_bytes());
    emit(&common, &mut blob, &idx);
    blob.push(T_END); // common
    blob.push(T_END); // appinfo

    let mut payload = Vec::new();
    payload.extend(1u32.to_le_bytes());
    payload.extend(0u32.to_le_bytes());
    payload.extend(0u64.to_le_bytes());
    payload.extend([0u8; 20]);
    payload.extend(0u32.to_le_bytes());
    payload.extend([0u8; 20]);
    payload.extend(&blob);

    let mut body = Vec::new();
    body.extend(app_id.to_le_bytes());
    body.extend((payload.len() as u32).to_le_bytes());
    body.extend(&payload);
    body.extend(0u32.to_le_bytes());

    let mut table = Vec::new();
    table.extend((strings.len() as u32).to_le_bytes());
    for s in &strings {
        table.extend(s.as_bytes());
        table.push(0);
    }

    let mut data = Vec::new();
    data.extend(MAGIC_V29.to_le_bytes());
    data.extend(1u32.to_le_bytes());
    data.extend(((4 + 4 + 8 + body.len()) as i64).to_le_bytes());
    data.extend(&body);
    data.extend(&table);
    data
}

/// `library_assets_full/<slot>/image/english = <path>`, the real nesting.
fn asset_slot<'a>(slot: &'a str, path: &'a str) -> Node<'a> {
    Node::Map(
        slot,
        vec![Node::Map("image", vec![Node::Str("english", path)])],
    )
}

/// Build a v29 file with a string table, mirroring the real layout.
fn build_v29(apps: &[(u32, &str, &str, &str)]) -> Vec<u8> {
    let strings: Vec<&str> = vec!["appinfo", "appid", "common", "type", "name", "clienticon"];
    let idx = |s: &str| {
        strings
            .iter()
            .position(|x| *x == s)
            .map(|i| i as u32)
            .unwrap_or(0)
    };

    let mut body = Vec::new();
    for (id, ty, name, icon) in apps {
        // The KV blob.
        let mut blob = Vec::new();
        blob.push(T_MAP);
        blob.extend(idx("appinfo").to_le_bytes());
        blob.push(T_INT32);
        blob.extend(idx("appid").to_le_bytes());
        blob.extend(id.to_le_bytes());
        blob.push(T_MAP);
        blob.extend(idx("common").to_le_bytes());
        blob.push(T_STRING);
        blob.extend(idx("type").to_le_bytes());
        blob.extend(ty.as_bytes());
        blob.push(0);
        blob.push(T_STRING);
        blob.extend(idx("name").to_le_bytes());
        blob.extend(name.as_bytes());
        blob.push(0);
        blob.push(T_STRING);
        blob.extend(idx("clienticon").to_le_bytes());
        blob.extend(icon.as_bytes());
        blob.push(0);
        blob.push(T_END); // common
        blob.push(T_END); // appinfo
        blob.push(T_END); // blob terminator, as the real file has

        let mut payload = Vec::new();
        payload.extend(1u32.to_le_bytes()); // info_state
        payload.extend(0x6a15_4cb4u32.to_le_bytes()); // last_updated
        payload.extend(0u64.to_le_bytes()); // pics_token
        payload.extend([0xAAu8; 20]); // sha1_text
        payload.extend(0x019d_8256u32.to_le_bytes()); // change_number
        payload.extend([0xBBu8; 20]); // sha1_data (v28+)
        payload.extend(&blob);

        body.extend(id.to_le_bytes());
        body.extend((payload.len() as u32).to_le_bytes());
        body.extend(&payload);
    }
    body.extend(0u32.to_le_bytes()); // appid 0 terminates

    let mut table = Vec::new();
    table.extend((strings.len() as u32).to_le_bytes());
    for s in &strings {
        table.extend(s.as_bytes());
        table.push(0);
    }

    let mut out = Vec::new();
    out.extend(MAGIC_V29.to_le_bytes());
    out.extend(1u32.to_le_bytes()); // universe
    let table_offset = (4 + 4 + 8 + body.len()) as i64;
    out.extend(table_offset.to_le_bytes());
    out.extend(&body);
    out.extend(&table);
    out
}

#[test]
fn parses_a_v29_file_resolving_keys_through_the_string_table() {
    let data = build_v29(&[
        (620, "Game", "Portal 2", "abc123"),
        (228980, "Tool", "Steamworks Common Redistributables", ""),
    ]);
    let info = parse(&data).unwrap();

    assert_eq!(info.version, Version::V29);
    assert_eq!(info.universe, 1);
    assert_eq!(info.skipped, 0);
    assert_eq!(info.apps.len(), 2);
    assert!(
        info.aligned,
        "the entry list must end exactly where the string table begins"
    );

    let portal = info.apps.get(&620).unwrap();
    assert_eq!(portal.common.app_type.as_deref(), Some("Game"));
    assert_eq!(portal.common.name.as_deref(), Some("Portal 2"));
    assert_eq!(portal.common.client_icon.as_deref(), Some("abc123"));
    assert_eq!(portal.change_number, 0x019d_8256);

    let tool = info.apps.get(&228980).unwrap();
    assert_eq!(tool.common.app_type.as_deref(), Some("Tool"));
}

#[test]
fn an_unknown_magic_is_a_named_error_so_callers_can_degrade() {
    let mut data = build_v29(&[(620, "Game", "Portal 2", "x")]);
    data[0..4].copy_from_slice(&0x0756_4499u32.to_le_bytes());
    assert_eq!(
        parse(&data).unwrap_err(),
        Error::UnsupportedVersion { magic: 0x0756_4499 }
    );
}

#[test]
fn one_corrupt_entry_is_skipped_and_the_rest_still_parse() {
    // The property that matters: a length-prefixed entry lets the reader resync, so a bad
    // blob costs one app rather than the whole library.
    let mut data = build_v29(&[
        (620, "Game", "Portal 2", "a"),
        (440, "Game", "Team Fortress 2", "b"),
    ]);

    // Corrupt the *first* entry's blob with an unknown type marker. Its payload starts at
    // 16 (header) + 8 (appid+size) + 60 (fixed metadata) = 84.
    data[84] = 0x7F;

    let info = parse(&data).unwrap();
    assert_eq!(info.skipped, 1, "the damaged entry must be counted");
    assert_eq!(info.apps.len(), 1, "the healthy entry must survive");
    assert!(
        info.apps.contains_key(&440),
        "resync must land on the next entry exactly"
    );
}

#[test]
fn a_nested_map_inside_common_cannot_overwrite_the_captured_fields() {
    // `common` really does contain sub-maps (`name_localized`) with a `name` key in them.
    // Capturing those would rename every app to its localised alias.
    let strings = ["appinfo", "common", "type", "name", "name_localized"];
    let idx = |s: &str| strings.iter().position(|x| *x == s).unwrap_or(0) as u32;

    let mut blob = Vec::new();
    blob.push(T_MAP);
    blob.extend(idx("appinfo").to_le_bytes());
    blob.push(T_MAP);
    blob.extend(idx("common").to_le_bytes());
    blob.push(T_STRING);
    blob.extend(idx("type").to_le_bytes());
    blob.extend(b"Game\0");
    blob.push(T_STRING);
    blob.extend(idx("name").to_le_bytes());
    blob.extend(b"Real Name\0");
    blob.push(T_MAP);
    blob.extend(idx("name_localized").to_le_bytes());
    blob.push(T_STRING);
    blob.extend(idx("name").to_le_bytes());
    blob.extend(b"Localised Name\0");
    blob.push(T_END); // name_localized
    blob.push(T_END); // common
    blob.push(T_END); // appinfo

    let mut payload = Vec::new();
    payload.extend(1u32.to_le_bytes());
    payload.extend(0u32.to_le_bytes());
    payload.extend(0u64.to_le_bytes());
    payload.extend([0u8; 20]);
    payload.extend(0u32.to_le_bytes());
    payload.extend([0u8; 20]);
    payload.extend(&blob);

    let mut body = Vec::new();
    body.extend(70u32.to_le_bytes());
    body.extend((payload.len() as u32).to_le_bytes());
    body.extend(&payload);
    body.extend(0u32.to_le_bytes());

    let mut table = Vec::new();
    table.extend((strings.len() as u32).to_le_bytes());
    for s in strings {
        table.extend(s.as_bytes());
        table.push(0);
    }

    let mut data = Vec::new();
    data.extend(MAGIC_V29.to_le_bytes());
    data.extend(1u32.to_le_bytes());
    data.extend(((4 + 4 + 8 + body.len()) as i64).to_le_bytes());
    data.extend(&body);
    data.extend(&table);

    let info = parse(&data).unwrap();
    let app = info.apps.get(&70).unwrap();
    assert_eq!(app.common.name.as_deref(), Some("Real Name"));
    assert_eq!(app.common.app_type.as_deref(), Some("Game"));

    // A control for the same property, from the other direction. The assertions above are
    // all negative — they pass just as well if nothing under `common` were captured at all,
    // which is exactly how a broken fixture hides. This half asserts that a *deeper* path
    // still gets captured while `name_localized/name` does not, so the two cannot both be
    // explained by "capture never ran".
    let data = build_tree(
        70,
        vec![
            Node::Str("name", "Real Name"),
            Node::Map("name_localized", vec![Node::Str("name", "Localised Name")]),
            Node::Map(
                "library_assets_full",
                vec![asset_slot("library_capsule", "library_600x900.jpg")],
            ),
        ],
    );
    let info = parse(&data).unwrap();
    let common = &info.apps.get(&70).unwrap().common;
    assert_eq!(common.name.as_deref(), Some("Real Name"));
    assert_eq!(
        common.library_asset("library_capsule", "english"),
        Some("library_600x900.jpg"),
        "control: capture really is running on nested paths",
    );
}

#[test]
fn library_assets_full_survives_both_disk_shapes() {
    // The two shapes measured on this box: a bare filename for 1945 apps, and a path with a
    // sha1 directory component for 278. Losing the sha1 component would produce a path that
    // does not exist, which is indistinguishable from "this app has no art".
    const FLAT: &str = "library_600x900.jpg";
    const NESTED: &str = "93637c34351160eaa7d7ff0cce69cb4312abb819/library_capsule.jpg";

    // Premise: these fixtures really are the two different shapes. Without this the test
    // still passes if someone "tidies" NESTED into a bare filename.
    assert!(!FLAT.contains('/'), "the flat fixture must have no path");
    assert!(NESTED.contains('/'), "the nested fixture must have one");

    let flat = build_tree(
        620,
        vec![Node::Map(
            "library_assets_full",
            vec![asset_slot("library_capsule", FLAT)],
        )],
    );
    let nested = build_tree(
        1030300,
        vec![Node::Map(
            "library_assets_full",
            vec![
                asset_slot("library_capsule", NESTED),
                asset_slot("library_hero", "70d7e70a/library_hero.jpg"),
            ],
        )],
    );

    let a = parse(&flat).unwrap();
    let common = &a.apps.get(&620).unwrap().common;
    assert_eq!(
        common.library_asset("library_capsule", "english"),
        Some(FLAT)
    );

    let b = parse(&nested).unwrap();
    let common = &b.apps.get(&1030300).unwrap().common;
    assert_eq!(
        common.library_asset("library_capsule", "english"),
        Some(NESTED),
        "the sha1 directory component must survive verbatim"
    );
    assert_eq!(
        common.library_asset("library_hero", "english"),
        Some("70d7e70a/library_hero.jpg"),
        "slots must not overwrite each other"
    );
    assert_eq!(common.library_asset("library_logo", "english"), None);
}

#[test]
fn every_language_is_kept_and_lookup_falls_back() {
    let data = build_tree(
        1091500,
        vec![Node::Map(
            "library_assets_full",
            vec![Node::Map(
                "library_capsule",
                vec![Node::Map(
                    "image",
                    vec![
                        Node::Str("english", "6399de/library_capsule.jpg"),
                        Node::Str("schinese", "e8cc29/library_capsule_schinese.jpg"),
                    ],
                )],
            )],
        )],
    );
    let info = parse(&data).unwrap();
    let common = &info.apps.get(&1091500).unwrap().common;

    // Premise: both languages really were captured, so the fallback below is a fallback and
    // not the only entry there is.
    assert_eq!(
        common
            .library_assets
            .get("library_capsule")
            .map(BTreeMap::len),
        Some(2),
    );

    assert_eq!(
        common.library_asset("library_capsule", "schinese"),
        Some("e8cc29/library_capsule_schinese.jpg"),
        "an exact language match wins",
    );
    assert_eq!(
        common.library_asset("library_capsule", "klingon"),
        Some("6399de/library_capsule.jpg"),
        "an unknown language falls back to english",
    );
}

#[test]
fn a_language_steam_has_no_english_for_still_resolves() {
    // The last fallback rung. An app localized only into one language still has perfectly
    // good artwork, and returning None would render a blank tile.
    let data = build_tree(
        777,
        vec![Node::Map(
            "library_assets_full",
            vec![Node::Map(
                "library_capsule",
                vec![Node::Map(
                    "image",
                    vec![Node::Str("schinese", "only/one.jpg")],
                )],
            )],
        )],
    );
    let info = parse(&data).unwrap();
    let common = &info.apps.get(&777).unwrap().common;

    assert!(
        !common
            .library_assets
            .get("library_capsule")
            .unwrap()
            .contains_key("english"),
        "premise: there is deliberately no english entry",
    );
    assert_eq!(
        common.library_asset("library_capsule", "english"),
        Some("only/one.jpg"),
    );
}

#[test]
fn icon_and_clienticon_are_captured_as_separate_fields() {
    // Measured on 1030300: these are genuinely different sha1s. `icon` names the
    // librarycache .jpg; `clienticon` names a .ico under Steam\steam\games. Aliasing them
    // yields a path that does not exist.
    const ICON: &str = "b4a999c1302e3ac123c041fd41bb8a34528c6ab5";
    const CLIENT: &str = "28f5a413a0f1f4b0a0f8b6ff30e1cbb0e5ba9a3d";
    assert_ne!(
        ICON, CLIENT,
        "premise: the fixture uses two distinct values"
    );

    let data = build_tree(
        1030300,
        vec![
            Node::Str("icon", ICON),
            Node::Str("clienticon", CLIENT),
            Node::Str("type", "Game"),
        ],
    );
    let info = parse(&data).unwrap();
    let common = &info.apps.get(&1030300).unwrap().common;

    assert_eq!(common.icon.as_deref(), Some(ICON));
    assert_eq!(common.client_icon.as_deref(), Some(CLIENT));
}

#[test]
fn image2x_is_not_captured_but_image_beside_it_is() {
    // `image2x` sits next to `image` in the real file and none of those files exist on
    // disk here. The control is the `image` sibling: without it, a test that only asserts
    // image2x is absent would also pass if the whole slot failed to parse.
    let data = build_tree(
        620,
        vec![Node::Map(
            "library_assets_full",
            vec![Node::Map(
                "library_capsule",
                vec![
                    Node::Map("image", vec![Node::Str("english", "real.jpg")]),
                    Node::Map("image2x", vec![Node::Str("english", "retina.jpg")]),
                ],
            )],
        )],
    );
    let info = parse(&data).unwrap();
    let common = &info.apps.get(&620).unwrap().common;

    assert_eq!(
        common.library_asset("library_capsule", "english"),
        Some("real.jpg"),
        "control: the `image` sibling really was captured",
    );
    let slot = common.library_assets.get("library_capsule").unwrap();
    assert_eq!(slot.len(), 1, "image2x must not have been merged in");
}

#[test]
fn a_key_index_past_the_table_is_rejected_not_read_out_of_bounds() {
    let keys = Keys {
        table: &[b"appinfo"],
        indexed: true,
    };
    let bytes = 9999u32.to_le_bytes();
    let mut c = Cursor::new(&bytes);
    assert_eq!(
        read_key(&mut c, &keys).unwrap_err(),
        Error::KeyIndexOutOfRange {
            index: 9999,
            count: 1
        }
    );
}

#[test]
fn an_absurd_string_count_is_refused_rather_than_allocated() {
    // A corrupt count must not become a multi-gigabyte Vec::with_capacity.
    let mut data = Vec::new();
    data.extend(u32::MAX.to_le_bytes());
    data.extend(b"short\0");
    let err = read_string_table(&data, 0).unwrap_err();
    assert!(matches!(err, Error::StringTableTooLarge { .. }), "{err:?}");
}

#[test]
fn a_string_table_offset_past_the_end_is_an_error() {
    let data = [0u8; 16];
    assert!(matches!(
        read_string_table(&data, 9_999_999),
        Err(Error::StringTableOutOfRange { .. })
    ));
    assert!(matches!(
        read_string_table(&data, -1),
        Err(Error::StringTableOutOfRange { .. })
    ));
}

#[test]
fn a_truncated_file_errors_instead_of_panicking() {
    let full = build_v29(&[(620, "Game", "Portal 2", "a"), (440, "Game", "TF2", "b")]);
    let complete = parse(&full).unwrap();

    // Every prefix must either error or return a subset — and above all must never panic
    // on an out-of-range slice. This file is 6 MB of binary we do not control.
    for cut in 0..full.len() {
        if let Ok(partial) = parse(&full[..cut]) {
            assert!(
                partial.apps.len() <= complete.apps.len(),
                "truncating at {cut} produced more apps than the whole file"
            );
        }
    }
}

#[test]
fn an_empty_app_list_parses_to_nothing() {
    let data = build_v29(&[]);
    let info = parse(&data).unwrap();
    assert!(info.apps.is_empty());
    assert_eq!(info.skipped, 0);
}
