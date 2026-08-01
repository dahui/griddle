//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;
use crate::steam::process::SteamStopped;

/// A file shaped like the real one on this machine: mixed key casing, quoted path values,
/// mixed separators in `StartDir`, a `tags` submap, and the extra file-level terminator.
/// `[VERIFIED-BOX 2026-07-27]` No real paths, so it is safe to commit.
fn realistic() -> Vec<u8> {
    let mut v = Vec::new();
    v.push(0x00);
    v.extend(b"shortcuts\0");
    v.push(0x00);
    v.extend(b"0\0");
    v.push(0x02);
    v.extend(b"appid\0");
    v.extend((-246_118_299i32).to_le_bytes());
    v.push(0x01);
    v.extend(b"appname\0EmulationStationDE\0");
    v.push(0x01);
    v.extend(b"exe\0\"C:\\Windows\\System32\\cmd.exe\" /k start\0");
    v.push(0x01);
    v.extend(b"StartDir\0\"C:\\Users\\test\\AppData/Roaming/Emu\"\0");
    v.push(0x01);
    v.extend(b"icon\0\"C:\\icons\\Old.ico\"\0");
    v.push(0x01);
    v.extend(b"ShortcutPath\0\0");
    v.push(0x02);
    v.extend(b"IsHidden\0");
    v.extend(0i32.to_le_bytes());
    v.push(0x00);
    v.extend(b"tags\0");
    v.push(0x01);
    v.extend(b"0\0favorite\0");
    v.push(0x08); // tags
    v.push(0x08); // "0"
    v.push(0x08); // shortcuts
    v.push(0x08); // file-level
    v
}

const APP: AppId = AppId::new(4_048_848_997);

fn loaded() -> Shortcuts {
    Shortcuts::from_bytes(PathBuf::from("shortcuts.vdf"), realistic()).unwrap()
}

#[test]
fn reads_the_shortcut_through_case_insensitive_lookups() {
    let s = loaded();
    let sc = s.find(APP).unwrap();
    assert_eq!(sc.app_name(), Some("EmulationStationDE"));
    // `StartDir` is CamelCase in the file, queried lowercase here.
    assert_eq!(
        sc.start_dir(),
        Some("\"C:\\Users\\test\\AppData/Roaming/Emu\"")
    );
    assert_eq!(sc.index(), b"0");
    assert_eq!(sc.tags(), vec!["favorite"]);
    assert!(!sc.is_hidden());
}

#[test]
fn icon_path_strips_the_stored_quotes_but_icon_does_not() {
    let s = loaded();
    let sc = s.find(APP).unwrap();
    assert_eq!(sc.icon(), Some("\"C:\\icons\\Old.ico\""));
    assert_eq!(sc.icon_path(), Some("C:\\icons\\Old.ico"));
}

#[test]
fn an_untouched_document_is_not_modified() {
    let s = loaded();
    assert!(!s.is_modified());
    assert_eq!(s.to_bytes(), realistic());
}

#[test]
fn setting_an_icon_matches_the_files_quoting_convention() {
    let mut s = loaded();
    let change = s.set_icon(APP, "C:\\icons\\New.ico").unwrap();
    assert!(change.quoted, "this file quotes its paths");
    assert_eq!(change.applied, "\"C:\\icons\\New.ico\"");
    assert_eq!(change.previous.as_deref(), Some("\"C:\\icons\\Old.ico\""));
    assert_eq!(s.find(APP).unwrap().icon_path(), Some("C:\\icons\\New.ico"));
}

#[test]
fn an_already_quoted_input_is_not_double_quoted() {
    let mut s = loaded();
    let change = s.set_icon(APP, "\"C:\\icons\\New.ico\"").unwrap();
    assert_eq!(change.applied, "\"C:\\icons\\New.ico\"");
}

#[test]
fn an_unquoted_file_stays_unquoted() {
    // Steam itself writes bare paths; only some third-party tools quote them. Whichever
    // this file uses, we must not switch it.
    let mut unquoted = Vec::new();
    unquoted.push(0x00);
    unquoted.extend(b"shortcuts\0");
    unquoted.push(0x00);
    unquoted.extend(b"0\0");
    unquoted.push(0x02);
    unquoted.extend(b"appid\0");
    unquoted.extend((-246_118_299i32).to_le_bytes());
    unquoted.push(0x01);
    unquoted.extend(b"exe\0C:\\Games\\game.exe\0");
    unquoted.push(0x01);
    unquoted.extend(b"icon\0\0"); // present but empty
    unquoted.push(0x08);
    unquoted.push(0x08);
    unquoted.push(0x08);

    let mut s = Shortcuts::from_bytes(PathBuf::from("s.vdf"), unquoted).unwrap();
    let change = s.set_icon(APP, "C:\\icons\\New.ico").unwrap();
    assert!(
        !change.quoted,
        "an empty icon must fall back to the `exe` convention, which is bare here"
    );
    assert_eq!(change.applied, "C:\\icons\\New.ico");
}

#[test]
fn editing_preserves_field_order_key_casing_and_everything_else() {
    let mut s = loaded();
    let _ = s.set_icon(APP, "C:\\icons\\New.ico").unwrap();
    let after = s.to_bytes();

    let doc = binary::parse(&after).unwrap();
    let shortcuts = binary::get(&doc.entries, "shortcuts")
        .and_then(Value::as_map)
        .unwrap();
    let fields = binary::get(shortcuts, "0").and_then(Value::as_map).unwrap();
    let keys: Vec<String> = fields
        .iter()
        .map(|e| String::from_utf8_lossy(&e.key).into_owned())
        .collect();
    assert_eq!(
        keys,
        [
            "appid",
            "appname",
            "exe",
            "StartDir",
            "icon",
            "ShortcutPath",
            "IsHidden",
            "tags"
        ],
        "field order and the file's original key casing must both survive an edit"
    );

    // The mixed separators in StartDir are exactly the sort of thing a careless writer
    // "fixes".
    let sd = binary::get(fields, "StartDir")
        .and_then(Value::as_str)
        .unwrap();
    assert_eq!(sd, "\"C:\\Users\\test\\AppData/Roaming/Emu\"");
    // And the file-level terminator count is unchanged.
    assert_eq!(doc.trailing_terminators, 1);
}

#[test]
fn only_the_icon_bytes_differ_after_setting_an_icon() {
    let mut s = loaded();
    let _ = s.set_icon(APP, "C:\\icons\\Old.ico").unwrap();
    // Same length of value, different content -> same length overall.
    let after = s.to_bytes();
    assert_eq!(after.len(), realistic().len());
}

#[test]
fn clearing_an_icon_empties_the_field_rather_than_removing_it() {
    let mut s = loaded();
    let previous = s.clear_icon(APP).unwrap();
    assert_eq!(previous.as_deref(), Some("\"C:\\icons\\Old.ico\""));

    let sc_bytes = s.to_bytes();
    let doc = binary::parse(&sc_bytes).unwrap();
    let shortcuts = binary::get(&doc.entries, "shortcuts")
        .and_then(Value::as_map)
        .unwrap();
    let fields = binary::get(shortcuts, "0").and_then(Value::as_map).unwrap();
    assert_eq!(
        binary::get(fields, "icon").and_then(Value::as_bytes),
        Some(&b""[..]),
        "the key must still be present"
    );
}

#[test]
fn an_unknown_appid_is_a_named_error() {
    let mut s = loaded();
    let err = s.set_icon(AppId::new(12345), "x.ico").unwrap_err();
    assert!(matches!(err, Error::NotFound(id, _) if id == AppId::new(12345)));
}

#[test]
fn a_file_we_cannot_reproduce_is_refused_before_any_edit() {
    // Trailing garbage the codec would drop. Better to refuse than to write back a file
    // missing bytes we did not understand.
    let mut bad = realistic();
    bad.push(0x99);
    let err = Shortcuts::from_bytes(PathBuf::from("s.vdf"), bad).unwrap_err();
    // A parse error here, but the important property is that it is an error at all.
    assert!(matches!(
        err,
        Error::Parse { .. } | Error::RoundTripMismatch { .. }
    ));
}

#[test]
fn a_missing_file_loads_as_an_empty_document() {
    let t = tempfile::tempdir().unwrap();
    let s = Shortcuts::load_or_empty(t.path().join("shortcuts.vdf")).unwrap();
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);
    assert!(s.find(APP).is_none());
}

#[test]
fn scalar_siblings_among_the_numbered_keys_are_skipped_not_fatal() {
    // The defect that breaks third-party libraryfolders parsers, applied here defensively.
    let mut v = Vec::new();
    v.push(0x00);
    v.extend(b"shortcuts\0");
    v.push(0x01);
    // `\x00` spelled out: `\0` followed by a digit reads as an octal escape.
    v.extend(b"contentstatsid\x00778551\0"); // a scalar where a map is expected
    v.push(0x00);
    v.extend(b"0\0");
    v.push(0x02);
    v.extend(b"appid\0");
    v.extend((-246_118_299i32).to_le_bytes());
    v.push(0x08);
    v.push(0x08);
    v.push(0x08);

    let s = Shortcuts::from_bytes(PathBuf::from("s.vdf"), v).unwrap();

    // Prove the premise before asserting the behaviour: the map really must contain two
    // children, one of them a scalar. Without this the test would still pass against a
    // fixture that never had a scalar sibling in it at all — which is exactly what a
    // mis-escaped `\0` had produced here.
    let children = s.shortcuts_map().unwrap();
    assert_eq!(children.len(), 2, "fixture must have a scalar sibling");
    assert!(
        children
            .iter()
            .any(|e| e.key == b"contentstatsid" && e.value.as_map().is_none()),
        "fixture's scalar sibling is not shaped as intended: {children:?}"
    );

    assert_eq!(s.len(), 1, "the scalar must be skipped, not counted");
    assert_eq!(s.find(APP).unwrap().app_id(), Some(APP));
}

// -- the write path ------------------------------------------------------------------

#[test]
fn save_writes_atomically_backs_up_once_and_verifies() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("shortcuts.vdf");
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(&path, realistic()).unwrap();

    let token = SteamStopped::synthetic_for_test();

    let mut s = Shortcuts::load(&path).unwrap();
    let _ = s.set_icon(APP, "C:\\icons\\New.ico").unwrap();
    let saved = s.save(&token).unwrap();

    let backup = path.with_file_name("shortcuts.vdf.sgdb-orig");
    assert_eq!(saved.backup_created.as_deref(), Some(backup.as_path()));
    assert_eq!(
        std::fs::read(&backup).unwrap(),
        realistic(),
        "the backup must be the pristine original"
    );

    // Reload and confirm the edit landed.
    let reloaded = Shortcuts::load(&path).unwrap();
    assert_eq!(
        reloaded.find(APP).unwrap().icon_path(),
        Some("C:\\icons\\New.ico")
    );

    // No temp file left behind.
    let leftovers: Vec<_> = std::fs::read_dir(t.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("sgdbtmp"))
        .collect();
    assert!(leftovers.is_empty(), "left temp files: {leftovers:?}");
}

#[test]
fn the_original_backup_is_never_overwritten_by_a_later_save() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("shortcuts.vdf");
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(&path, realistic()).unwrap();
    let token = SteamStopped::synthetic_for_test();

    let mut first = Shortcuts::load(&path).unwrap();
    let _ = first.set_icon(APP, "C:\\a.ico").unwrap();
    let _ = first.save(&token).unwrap();

    let mut second = Shortcuts::load(&path).unwrap();
    let _ = second.set_icon(APP, "C:\\b.ico").unwrap();
    let saved = second.save(&token).unwrap();

    assert_eq!(
        saved.backup_created, None,
        "a second save must not create another backup"
    );
    assert_eq!(
        std::fs::read(path.with_file_name("shortcuts.vdf.sgdb-orig")).unwrap(),
        realistic(),
        "the pristine original must survive repeated saves"
    );
}

#[test]
fn saving_an_unmodified_document_writes_nothing() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("shortcuts.vdf");
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(&path, realistic()).unwrap();
    let before = std::fs::metadata(&path).unwrap().len();

    let s = Shortcuts::load(&path).unwrap();
    let saved = s.save(&SteamStopped::synthetic_for_test()).unwrap();

    assert_eq!(saved.bytes_written, 0);
    assert_eq!(saved.backup_created, None);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), before);
    assert!(
        !path.with_file_name("shortcuts.vdf.sgdb-orig").exists(),
        "a no-op save must not even create a backup"
    );
}

#[test]
fn a_new_file_saves_without_a_backup() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("shortcuts.vdf");

    let s = Shortcuts::load_or_empty(&path).unwrap();
    let saved = s.save(&SteamStopped::synthetic_for_test()).unwrap();

    assert_eq!(saved.backup_created, None, "nothing existed to preserve");
    assert!(path.is_file());
    // What we wrote must be loadable, and shaped like Steam's own empty file.
    let reloaded = Shortcuts::load(&path).unwrap();
    assert!(reloaded.is_empty());
    assert_eq!(reloaded.to_bytes(), s.to_bytes());
}

#[test]
fn sibling_suffix_keeps_the_vdf_in_the_name() {
    // `with_extension` would give `shortcuts.sgdb-orig`, losing what the file is.
    let p = sibling_with_suffix(Path::new("/a/shortcuts.vdf"), ".sgdb-orig");
    assert_eq!(p.file_name().unwrap(), "shortcuts.vdf.sgdb-orig");
}

#[test]
fn quote_helpers_handle_the_degenerate_cases() {
    assert!(!is_quoted("\""), "a lone quote is not a quoted string");
    assert_eq!(strip_quotes("\""), "\"");
    assert_eq!(strip_quotes(""), "");
    assert_eq!(strip_quotes("\"\""), "");
    assert_eq!(strip_quotes("bare"), "bare");
    assert_eq!(strip_quotes("\"a\"b\""), "a\"b");
}
