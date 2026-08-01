//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;

#[test]
fn normalizes_the_hkcu_form() {
    // Exactly what HKCU stores on this machine.
    let p = normalize("c:/program files (x86)/steam");
    if cfg!(windows) {
        assert_eq!(p, PathBuf::from(r"c:\program files (x86)\steam"));
    }
    // Casing is left alone on purpose.
    assert!(p.to_string_lossy().contains("program files"));
}

#[test]
fn normalize_trims_trailing_separators_and_whitespace() {
    let a = normalize("  C:/Steam/  ");
    let b = normalize("C:/Steam");
    assert_eq!(a, b);
}

#[test]
fn derives_every_path_from_the_root() {
    let s = SteamInstall::at("/steam");
    assert!(s.grid_dir(16_274_804).ends_with("grid"));
    assert!(
        s.grid_dir(16_274_804)
            .to_string_lossy()
            .contains("16274804")
    );
    assert!(s.shortcuts_vdf(16_274_804).ends_with("shortcuts.vdf"));
    assert!(s.localconfig_vdf(16_274_804).ends_with("localconfig.vdf"));
    assert!(s.cef_sentinel().ends_with(".cef-enable-remote-debugging"));
    assert!(s.library_cache_dir().ends_with("librarycache"));
}

#[test]
fn library_folders_prefers_the_config_copy() {
    let t = tempfile::tempdir().unwrap();
    let s = SteamInstall::at(t.path());
    // With neither present, fall back to the steamapps path rather than failing.
    assert!(
        s.library_folders_vdf()
            .to_string_lossy()
            .contains("steamapps")
    );

    std::fs::create_dir_all(t.path().join("config")).unwrap();
    // boundary-ok: test fixture written into a tempdir
    std::fs::write(t.path().join("config").join("libraryfolders.vdf"), "").unwrap();
    assert!(s.library_folders_vdf().to_string_lossy().contains("config"));
}

#[test]
fn an_invalid_explicit_override_is_an_error_not_a_fallback() {
    let t = tempfile::tempdir().unwrap();
    let empty = t.path().join("not-steam");
    std::fs::create_dir_all(&empty).unwrap();

    let got = locate_with(Some(empty.as_os_str()));
    assert!(
        matches!(got, Err(Error::OverrideInvalid(_))),
        "an explicit override that is wrong must fail loudly, got {got:?}",
    );
}

#[test]
fn a_valid_override_is_accepted() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("userdata")).unwrap();

    let install = locate_with(Some(t.path().as_os_str())).unwrap();
    assert_eq!(install.source(), Source::EnvOverride);
    assert_eq!(install.root(), t.path());
}

#[test]
fn an_override_with_a_trailing_slash_still_resolves() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("userdata")).unwrap();
    let with_slash = format!("{}/", t.path().display());

    let install = locate_with(Some(std::ffi::OsStr::new(&with_slash))).unwrap();
    assert_eq!(install.root(), t.path());
}
