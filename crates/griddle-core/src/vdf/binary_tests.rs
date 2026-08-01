//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;

/// A hand-built document mirroring the shape of the real `shortcuts.vdf`, including the
/// extra file-level terminator. Safe to commit — no real paths.
fn synthetic() -> Vec<u8> {
    let mut v = Vec::new();
    v.push(T_MAP);
    v.extend(b"shortcuts\0");
    v.push(T_MAP);
    v.extend(b"0\0");
    v.push(T_INT32);
    v.extend(b"appid\0");
    v.extend((-246118299i32).to_le_bytes()); // 0xF1548865
    v.push(T_STRING);
    v.extend(b"AppName\0Test App\0");
    v.push(T_STRING);
    // Deliberately mixed separators, as the real file has.
    v.extend(b"StartDir\0C:\\Users\\test/Sub/Dir\0");
    v.push(T_STRING);
    v.extend(b"FlatpakAppID\0\0"); // empty value
    v.push(T_MAP);
    v.extend(b"tags\0");
    v.push(T_STRING);
    v.extend(b"0\0favorite\0");
    v.push(T_END); // close tags
    v.push(T_END); // close "0"
    v.push(T_END); // close shortcuts
    v.push(T_END); // file-level terminator
    v
}

#[test]
fn round_trips_synthetic_byte_for_byte() {
    let input = synthetic();
    let doc = parse(&input).unwrap();
    assert_eq!(write(&doc), input);
}

#[test]
fn counts_the_extra_file_level_terminator() {
    let doc = parse(&synthetic()).unwrap();
    // Three closes are consumed by the maps themselves; the fourth is file-level.
    assert_eq!(doc.trailing_terminators, 1);
}

#[test]
fn reads_appid_as_signed_and_converts_to_the_unsigned_grid_name() {
    let doc = parse(&synthetic()).unwrap();
    let shortcuts = get(&doc.entries, "shortcuts").unwrap().as_map().unwrap();
    let first = get(shortcuts, "0").unwrap().as_map().unwrap();
    let appid = get(first, "appid").unwrap().as_i32().unwrap();

    // The field is signed in the file; grid artwork filenames use the unsigned form.
    // 0xF1548865 -> -246118299 signed -> 4048848997 unsigned. [VERIFIED-BOX 2026-07-27]
    assert_eq!(appid, -246118299);
    assert_eq!(appid as u32, 4_048_848_997);
}

#[test]
fn preserves_mixed_path_separators_verbatim() {
    let doc = parse(&synthetic()).unwrap();
    let shortcuts = get(&doc.entries, "shortcuts").unwrap().as_map().unwrap();
    let first = get(shortcuts, "0").unwrap().as_map().unwrap();
    let start_dir = get(first, "StartDir").unwrap().as_str().unwrap();
    assert_eq!(start_dir, "C:\\Users\\test/Sub/Dir");
}

#[test]
fn preserves_empty_string_values() {
    let doc = parse(&synthetic()).unwrap();
    let shortcuts = get(&doc.entries, "shortcuts").unwrap().as_map().unwrap();
    let first = get(shortcuts, "0").unwrap().as_map().unwrap();
    assert_eq!(get(first, "FlatpakAppID").unwrap().as_bytes().unwrap(), b"");
}

#[test]
fn preserves_entry_order() {
    let doc = parse(&synthetic()).unwrap();
    let shortcuts = get(&doc.entries, "shortcuts").unwrap().as_map().unwrap();
    let first = get(shortcuts, "0").unwrap().as_map().unwrap();
    let keys: Vec<_> = first
        .iter()
        .map(|e| String::from_utf8_lossy(&e.key).into_owned())
        .collect();
    assert_eq!(
        keys,
        ["appid", "AppName", "StartDir", "FlatpakAppID", "tags"]
    );
}

#[test]
fn round_trips_non_utf8_bytes() {
    // Steam writes whatever the user's filesystem gave it. A lossy String conversion
    // here would corrupt the file on write-back.
    let mut v = Vec::new();
    v.push(T_STRING);
    v.extend(b"key\0");
    v.extend([0xff, 0xfe, 0x80]);
    v.push(0);
    v.push(T_END);
    let doc = parse(&v).unwrap();
    assert_eq!(write(&doc), v);
    assert_eq!(get(&doc.entries, "key").unwrap().as_str(), None);
}

#[test]
fn rejects_unknown_type_marker() {
    let err = parse(&[0x42, b'k', 0]).unwrap_err();
    assert_eq!(
        err,
        Error::UnknownMarker {
            marker: 0x42,
            offset: 0
        }
    );
}

#[test]
fn rejects_unterminated_string() {
    let input = [T_STRING, b'k', 0, b'n', b'o', b'e', b'n', b'd'];
    assert!(matches!(
        parse(&input),
        Err(Error::UnterminatedString { .. })
    ));
}

#[test]
fn rejects_truncated_int() {
    let input = [T_INT32, b'k', 0, 0x01, 0x02];
    assert!(matches!(parse(&input), Err(Error::UnexpectedEof { .. })));
}

#[test]
fn rejects_trailing_garbage() {
    let mut input = synthetic();
    input.push(0x99);
    assert!(matches!(
        parse(&input),
        Err(Error::TrailingGarbage { found: 0x99, .. })
    ));
}

#[test]
fn round_trips_an_empty_document() {
    let doc = parse(&[]).unwrap();
    assert!(doc.entries.is_empty());
    assert_eq!(write(&doc), Vec::<u8>::new());
}
