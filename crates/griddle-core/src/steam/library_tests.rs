//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    // boundary-ok: test fixture written into a tempdir
    std::fs::write(p, content).unwrap();
}

fn manifest(appid: u32, name: &str, flags: u32, dir: &str) -> String {
    format!(
        "\"AppState\"\n{{\n\t\"appid\" \"{appid}\"\n\t\"name\" \"{name}\"\n\
         \t\"StateFlags\" \"{flags}\"\n\t\"installdir\" \"{dir}\"\n\
         \t\"SizeOnDisk\" \"12345\"\n}}\n"
    )
}

/// The scalar-sibling case, in the exact shape this machine's file has.
#[test]
fn skips_the_contentstatsid_scalar() {
    let t = tempfile::tempdir().unwrap();
    write(
        t.path(),
        "config/libraryfolders.vdf",
        r#"
"libraryfolders"
{
	"contentstatsid"		"7785519366728146050"
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps" { "228980" "491869131" "1091500" "91231172278" }
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps" { }
	}
}
"#,
    );
    let s = SteamInstall::at(t.path());
    let folders = library_folders(&s).unwrap();
    assert_eq!(
        folders.len(),
        2,
        "the scalar must be skipped, both libraries kept"
    );
    assert_eq!(
        folders[0].path,
        PathBuf::from(r"C:\Program Files (x86)\Steam")
    );
    assert_eq!(folders[0].apps.len(), 2);
    assert_eq!(folders[1].path, PathBuf::from(r"D:\SteamLibrary"));
}

#[test]
fn a_missing_libraryfolders_still_yields_the_steam_root() {
    let t = tempfile::tempdir().unwrap();
    let s = SteamInstall::at(t.path());
    let folders = library_folders(&s).unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].path, t.path());
}

#[test]
fn parses_a_manifest() {
    let t = tempfile::tempdir().unwrap();
    write(
        t.path(),
        "steamapps/appmanifest_1091500.acf",
        &manifest(1_091_500, "Cyberpunk 2077", 4, "Cyberpunk 2077"),
    );
    let app = parse_app_manifest(
        &t.path().join("steamapps/appmanifest_1091500.acf"),
        t.path(),
    )
    .unwrap()
    .unwrap();

    assert_eq!(app.app_id.get(), 1_091_500);
    assert_eq!(app.name, "Cyberpunk 2077");
    assert!(app.is_fully_installed());
    assert!(
        app.install_path().ends_with("common/Cyberpunk 2077")
            || app.install_path().ends_with(r"common\Cyberpunk 2077")
    );
}

/// `StateFlags` is a **bitfield**, so it must be tested with a mask, not equality.
///
/// `6` is `StateFullyInstalled | StateUpdateRequired` — installed *and* update-pending,
/// which is a playable game. This machine's FINAL FANTASY TACTICS reads `6`.
/// Measured on a real install, 2026-07-27. Note the trap: `6` looks like it should mean
/// "downloading", and a test asserting that passes against correct code being read wrongly.
#[test]
fn state_flags_are_a_bitfield_not_an_enum() {
    let t = tempfile::tempdir().unwrap();
    write(
        t.path(),
        "steamapps/appmanifest_1.acf",
        &manifest(1, "Installed", 4, "a"),
    );
    write(
        t.path(),
        "steamapps/appmanifest_2.acf",
        &manifest(2, "Installed, update pending", 6, "b"),
    );
    write(
        t.path(),
        "steamapps/appmanifest_3.acf",
        &manifest(3, "Not installed", 2, "c"),
    );
    write(
        t.path(),
        "steamapps/appmanifest_4.acf",
        &manifest(4, "Download queued", 1026, "d"),
    );
    write(
        t.path(),
        "config/libraryfolders.vdf",
        &format!(
            "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} }}",
            t.path().display().to_string().replace('\\', "\\\\")
        ),
    );

    let s = SteamInstall::at(t.path());
    let apps = installed_apps(&s).unwrap();
    assert_eq!(apps.len(), 4, "all four have manifests");

    let installed = |id: u32| {
        apps.iter()
            .find(|a| a.app_id.get() == id)
            .unwrap()
            .is_fully_installed()
    };
    assert!(installed(1), "4 = StateFullyInstalled");
    assert!(
        installed(2),
        "6 = installed AND update-pending — still playable"
    );
    assert!(!installed(3), "2 = update required, bit 4 clear");
    assert!(!installed(4), "1026 = queued, bit 4 clear");
}

#[test]
fn a_corrupt_manifest_does_not_empty_the_library() {
    let t = tempfile::tempdir().unwrap();
    write(
        t.path(),
        "steamapps/appmanifest_1.acf",
        &manifest(1, "Good", 4, "g"),
    );
    write(
        t.path(),
        "steamapps/appmanifest_2.acf",
        "\"AppState\" { \"appid\" \"unterminated",
    );
    write(
        t.path(),
        "config/libraryfolders.vdf",
        &format!(
            "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} }}",
            t.path().display().to_string().replace('\\', "\\\\")
        ),
    );

    let s = SteamInstall::at(t.path());
    let apps = installed_apps(&s).unwrap();
    assert_eq!(apps.len(), 1, "the good manifest must survive the bad one");
    assert_eq!(apps[0].name, "Good");
}

#[test]
fn ignores_files_that_are_not_manifests() {
    let t = tempfile::tempdir().unwrap();
    write(
        t.path(),
        "steamapps/appmanifest_1.acf",
        &manifest(1, "Real", 4, "r"),
    );
    write(
        t.path(),
        "steamapps/appmanifest_1.acf.bak",
        &manifest(9, "Backup", 4, "b"),
    );
    write(t.path(), "steamapps/readme.txt", "nope");
    write(
        t.path(),
        "config/libraryfolders.vdf",
        &format!(
            "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} }}",
            t.path().display().to_string().replace('\\', "\\\\")
        ),
    );

    let s = SteamInstall::at(t.path());
    let apps = installed_apps(&s).unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name, "Real");
}

#[test]
fn non_utf8_names_survive_lossily() {
    let t = tempfile::tempdir().unwrap();
    // The trademark sign, as it appears in Street Fighter 6's real manifest.
    write(
        t.path(),
        "steamapps/appmanifest_1364780.acf",
        &manifest(1_364_780, "Street Fighter™ 6", 4, "sf6"),
    );
    let app = parse_app_manifest(
        &t.path().join("steamapps/appmanifest_1364780.acf"),
        t.path(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(app.name, "Street Fighter™ 6");
}

#[test]
fn known_non_games_are_recognised() {
    assert!(is_known_non_game(AppId::new(228_980)));
    assert!(!is_known_non_game(AppId::new(1_091_500)));
}
