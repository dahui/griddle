//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;

#[test]
fn parses_an_appmanifest() {
    // Shape and values taken from the real appmanifest_228980.acf on this machine.
    let src = r#"
"AppState"
{
	"appid"		"228980"
	"Universe"		"1"
	"LauncherPath"		"C:\\Program Files (x86)\\Steam\\steam.exe"
	"name"		"Steamworks Common Redistributables"
	"StateFlags"		"4"
	"installdir"		"Steamworks Shared"
	"SizeOnDisk"		"491869131"
	"UserConfig"
	{
	}
	"MountedConfig"
	{
		"language"		"english"
	}
}
"#;
    let doc = parse(src).unwrap();
    let app = get(&doc.entries, "AppState").unwrap().as_map().unwrap();
    assert_eq!(get(app, "appid").unwrap().as_u32(), Some(228980));
    assert_eq!(get(app, "StateFlags").unwrap().as_u32(), Some(4));
    assert_eq!(
        get(app, "installdir").unwrap().as_str(),
        Some("Steamworks Shared")
    );
    // Escaped backslashes must collapse to single ones.
    assert_eq!(
        get(app, "LauncherPath").unwrap().as_str(),
        Some(r"C:\Program Files (x86)\Steam\steam.exe")
    );
    // An empty map is a map, not a string.
    assert_eq!(get(app, "UserConfig").unwrap().as_map().unwrap().len(), 0);
    assert_eq!(
        get(app, "MountedConfig").unwrap().as_map().unwrap().len(),
        1
    );
}

#[test]
fn case_insensitive_lookup() {
    let doc = parse(r#""AppState" { "AppID" "7" }"#).unwrap();
    let app = get(&doc.entries, "appstate").unwrap().as_map().unwrap();
    assert_eq!(get(app, "appid").unwrap().as_u32(), Some(7));
}

/// The scalar-among-numbered-keys case that breaks naive parsers.
#[test]
fn tolerates_a_scalar_sibling_among_numbered_library_entries() {
    let src = r#"
"libraryfolders"
{
	"contentstatsid"		"7785519366728146050"
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"228980"		"491869131"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps"
		{
		}
	}
}
"#;
    let doc = parse(src).unwrap();
    let lf = get(&doc.entries, "libraryfolders")
        .unwrap()
        .as_map()
        .unwrap();
    assert_eq!(
        lf.len(),
        3,
        "the scalar sibling must be preserved, not dropped"
    );

    // The consumer pattern: skip children that are not maps.
    let paths: Vec<&str> = lf
        .iter()
        .filter_map(|e| e.value.as_map())
        .filter_map(|m| get(m, "path")?.as_str())
        .collect();
    assert_eq!(paths, [r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);

    // And the scalar is still readable if anyone wants it.
    assert_eq!(
        get(lf, "contentstatsid").unwrap().as_str(),
        Some("7785519366728146050")
    );
}

#[test]
fn reads_the_nested_apps_map() {
    let src = r#""libraryfolders" { "0" { "apps" { "220" "1234" "440" "5678" } } }"#;
    let doc = parse(src).unwrap();
    let lf = get(&doc.entries, "libraryfolders")
        .unwrap()
        .as_map()
        .unwrap();
    let zero = get(lf, "0").unwrap().as_map().unwrap();
    let apps = get(zero, "apps").unwrap().as_map().unwrap();
    assert_eq!(apps.len(), 2);
    assert_eq!(get(apps, "440").unwrap().as_u64(), Some(5678));
}

#[test]
fn skips_comments() {
    let src = r#"
// leading comment
"root"   // trailing comment
{
"a" "1"   // another
// whole line
"b" "2"
}
"#;
    let doc = parse(src).unwrap();
    let root = get(&doc.entries, "root").unwrap().as_map().unwrap();
    assert_eq!(root.len(), 2);
    assert_eq!(get(root, "b").unwrap().as_str(), Some("2"));
}

#[test]
fn handles_platform_conditionals() {
    let doc = parse(r#""root" { "a" "1" [$WIN32] "b" "2" }"#).unwrap();
    let root = get(&doc.entries, "root").unwrap().as_map().unwrap();
    assert_eq!(root.len(), 2, "the conditional must not be read as a key");
    assert_eq!(get(root, "b").unwrap().as_str(), Some("2"));
}

#[test]
fn handles_unquoted_tokens() {
    let doc = parse("root { key value }").unwrap();
    let root = get(&doc.entries, "root").unwrap().as_map().unwrap();
    assert_eq!(get(root, "key").unwrap().as_str(), Some("value"));
}

#[test]
fn preserves_duplicate_keys_and_order() {
    // Order matters for `libraryfolders`; duplicates are legal in KV1.
    let doc = parse(r#""r" { "k" "1" "k" "2" }"#).unwrap();
    let r = get(&doc.entries, "r").unwrap().as_map().unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].value.as_str(), Some("1"));
    assert_eq!(r[1].value.as_str(), Some("2"));
    // `get` returns the first, matching Steam's own behaviour.
    assert_eq!(get(r, "k").unwrap().as_str(), Some("1"));
}

#[test]
fn escapes_inside_quoted_strings() {
    let doc = parse(r#""r" { "s" "a\"b\\c\nd" }"#).unwrap();
    let r = get(&doc.entries, "r").unwrap().as_map().unwrap();
    assert_eq!(get(r, "s").unwrap().as_str(), Some("a\"b\\c\nd"));
}

#[test]
fn rejects_unterminated_string() {
    assert!(matches!(
        parse(r#""r" { "k" "unterminated "#),
        Err(Error::UnterminatedString { .. })
    ));
}

#[test]
fn rejects_unbalanced_close() {
    assert!(matches!(
        parse(r#""r" { "k" "v" } }"#),
        Err(Error::UnexpectedClose { .. })
    ));
}

#[test]
fn rejects_missing_close() {
    assert!(matches!(
        parse(r#""r" { "k" "v" "#),
        Err(Error::UnexpectedEof { .. })
    ));
}

#[test]
fn empty_input_is_an_empty_document() {
    assert_eq!(parse("").unwrap().entries.len(), 0);
    assert_eq!(parse("   \n // just a comment\n").unwrap().entries.len(), 0);
}

#[test]
fn handles_non_utf8_gracefully() {
    // Steam writes game names in whatever encoding it has; a lossy read must not panic.
    let raw = b"\"r\" { \"name\" \"Street Fighter\xe2\x84\xa2 6\" }";
    let doc = parse(&String::from_utf8_lossy(raw)).unwrap();
    let r = get(&doc.entries, "r").unwrap().as_map().unwrap();
    assert_eq!(get(r, "name").unwrap().as_str(), Some("Street Fighter™ 6"));
}
