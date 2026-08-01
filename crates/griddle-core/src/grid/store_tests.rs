//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;
use crate::logo::PinnedPosition;

fn dir() -> (tempfile::TempDir, GridDir) {
    let t = tempfile::tempdir().unwrap();
    let g = GridDir::new(t.path());
    g.ensure().unwrap();
    (t, g)
}

const APP: AppId = AppId::new(4_048_848_997);

#[test]
fn writes_the_expected_filename() {
    let (_t, g) = dir();
    let r = g.apply(APP, AssetType::Capsule, "png", b"data").unwrap();
    assert!(r.written.ends_with("4048848997p.png"));
    assert_eq!(std::fs::read(&r.written).unwrap(), b"data");
    assert!(r.removed.is_empty());
}

/// The core safety property: exactly one file per asset, never an ambiguous pair.
#[test]
fn replacing_a_jpg_with_a_png_removes_the_jpg() {
    let (_t, g) = dir();
    g.apply(APP, AssetType::Capsule, "jpg", b"old").unwrap();
    assert_eq!(g.existing(APP, AssetType::Capsule).len(), 1);

    let r = g.apply(APP, AssetType::Capsule, "png", b"new").unwrap();
    assert_eq!(r.removed.len(), 1, "the .jpg must be removed");
    assert!(r.removed[0].ends_with("4048848997p.jpg"));

    let remaining = g.existing(APP, AssetType::Capsule);
    assert_eq!(remaining.len(), 1, "exactly one file may remain");
    assert!(remaining[0].ends_with("4048848997p.png"));
}

#[test]
fn cleans_up_a_pre_existing_ambiguous_pair() {
    let (t, g) = dir();
    // Simulate a directory another tool left in a bad state.
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(t.path().join("4048848997p.png"), b"a").unwrap();
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(t.path().join("4048848997p.jpg"), b"b").unwrap();
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(t.path().join("4048848997p.jpeg"), b"c").unwrap();
    assert_eq!(g.existing(APP, AssetType::Capsule).len(), 3);

    let r = g.apply(APP, AssetType::Capsule, "png", b"new").unwrap();
    assert_eq!(r.removed.len(), 2, "the two non-target siblings go");
    assert_eq!(g.existing(APP, AssetType::Capsule).len(), 1);
    assert_eq!(std::fs::read(&r.written).unwrap(), b"new");
}

#[test]
fn assets_do_not_disturb_each_other() {
    let (_t, g) = dir();
    g.apply(APP, AssetType::Capsule, "png", b"cap").unwrap();
    g.apply(APP, AssetType::Hero, "jpg", b"hero").unwrap();
    g.apply(APP, AssetType::Header, "png", b"head").unwrap();

    assert_eq!(g.existing(APP, AssetType::Capsule).len(), 1);
    assert_eq!(g.existing(APP, AssetType::Hero).len(), 1);
    assert_eq!(g.existing(APP, AssetType::Header).len(), 1);
}

#[test]
fn applying_a_logo_creates_a_default_position() {
    let (_t, g) = dir();
    assert_eq!(g.read_logo_position(APP).unwrap(), None);

    let r = g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
    assert!(
        r.logo_position_created.is_some(),
        "a logo without a position may not render"
    );

    let pos = g.read_logo_position(APP).unwrap().unwrap();
    assert_eq!(pos.pinned_position, PinnedPosition::BottomLeft);
    assert_eq!(pos.width_pct, 50.0);
    assert_eq!(pos.height_pct, 50.0);
}

#[test]
fn applying_a_logo_preserves_an_existing_position() {
    let (_t, g) = dir();
    let custom = LogoPosition {
        pinned_position: PinnedPosition::CenterCenter,
        width_pct: 33.0,
        height_pct: 44.0,
    };
    g.write_logo_position(APP, custom).unwrap();

    let r = g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
    assert!(
        r.logo_position_created.is_none(),
        "must not overwrite the user's placement"
    );
    assert_eq!(g.read_logo_position(APP).unwrap().unwrap(), custom);
}

#[test]
fn clearing_a_logo_also_clears_its_position() {
    let (_t, g) = dir();
    g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
    let removed = g.clear(APP, AssetType::Logo).unwrap();
    assert_eq!(removed.len(), 2, "the .png and the .json");
    assert_eq!(g.read_logo_position(APP).unwrap(), None);
}

/// The Header asset shares a base name with the sidecar; clearing it must not take the
/// logo's placement with it.
#[test]
fn clearing_the_header_leaves_the_logo_position_alone() {
    let (_t, g) = dir();
    g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
    g.apply(APP, AssetType::Header, "png", b"header").unwrap();

    let removed = g.clear(APP, AssetType::Header).unwrap();
    assert_eq!(removed.len(), 1);
    assert!(
        g.read_logo_position(APP).unwrap().is_some(),
        "the .json must survive"
    );
}

#[test]
fn clearing_an_absent_asset_is_not_an_error() {
    let (_t, g) = dir();
    assert_eq!(
        g.clear(APP, AssetType::Hero).unwrap(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn refuses_empty_image_data() {
    let (_t, g) = dir();
    assert!(matches!(
        g.apply(APP, AssetType::Capsule, "png", b""),
        Err(Error::EmptyImage(_))
    ));
}

#[test]
fn refuses_to_write_into_a_missing_directory() {
    let t = tempfile::tempdir().unwrap();
    let g = GridDir::new(t.path().join("does-not-exist"));
    assert!(matches!(
        g.apply(APP, AssetType::Capsule, "png", b"x"),
        Err(Error::MissingDir(_))
    ));
}

#[test]
fn no_temp_files_survive_a_successful_write() {
    let (t, g) = dir();
    g.apply(APP, AssetType::Capsule, "png", b"data").unwrap();
    g.apply(APP, AssetType::Logo, "png", b"logo").unwrap();
    let leftovers: Vec<_> = std::fs::read_dir(t.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("sgdbtmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );
}

#[test]
fn a_corrupt_position_file_degrades_to_none() {
    let (t, g) = dir();
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(t.path().join("4048848997.json"), b"{ not json").unwrap();
    assert_eq!(g.read_logo_position(APP).unwrap(), None);
}

#[test]
fn orphans_finds_art_for_unknown_appids_only() {
    let (_t, g) = dir();
    g.apply(APP, AssetType::Capsule, "png", b"keep").unwrap();
    g.apply(AppId::new(999_999), AssetType::Hero, "png", b"stale")
        .unwrap();

    let orphans = g.orphans(&[APP]).unwrap();
    assert_eq!(orphans.len(), 1);
    assert!(orphans[0].ends_with("999999_hero.png"));
}

/// `Path::ends_with` matches whole components, so compare file names explicitly — a
/// partial suffix like `"_icon.ico"` silently never matches.
fn name_of(p: &Path) -> &str {
    p.file_name().and_then(|s| s.to_str()).unwrap_or("")
}

#[test]
fn customised_apps_lists_each_app_once() {
    let (_t, g) = dir();
    // Six files across two apps. The count the confirmation dialog quotes is *games*, so
    // one app with several slots must not read as several games.
    g.apply(APP, AssetType::Capsule, "png", b"a").unwrap();
    g.apply(APP, AssetType::Hero, "png", b"b").unwrap();
    g.apply(APP, AssetType::Logo, "png", b"c").unwrap();
    let other = AppId::new(620);
    g.apply(other, AssetType::Capsule, "png", b"d").unwrap();

    assert_eq!(g.customised_apps().unwrap(), vec![other, APP]);
}

#[test]
fn customised_apps_ignores_files_that_are_not_artwork() {
    let (t, g) = dir();
    g.apply(APP, AssetType::Capsule, "png", b"a").unwrap();
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(t.path().join("notes.txt"), b"mine").unwrap();
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(t.path().join("README"), b"mine").unwrap();

    // Premise: the bystanders really are there, or this would pass against an empty dir.
    assert_eq!(std::fs::read_dir(t.path()).unwrap().count(), 3);
    assert_eq!(g.customised_apps().unwrap(), vec![APP]);
}

#[test]
fn a_grid_directory_that_was_never_created_has_nothing_to_reset() {
    // Steam only creates `grid/` on the first custom art, so this is an ordinary first-run
    // state. It must not be an error on the screen that exists to report "nothing to do".
    let t = tempfile::tempdir().unwrap();
    let g = GridDir::new(t.path().join("never-created"));
    assert_eq!(g.customised_apps().unwrap(), Vec::new());
}

#[test]
fn the_predicted_count_is_exactly_what_clearing_deletes() {
    // The confirmation dialog quotes `removable`; the reset calls `clear`. If those two
    // ever disagree the user is shown one number and given another — and the direction that
    // matters is under-reporting, which is what "name it before it happens" forbids.
    let (_t, g) = dir();
    g.apply(APP, AssetType::Capsule, "png", b"a").unwrap();
    // Applying a logo with no position writes the sidecar, which is the file a naive count
    // misses: it is a sibling of no slot, yet `clear` removes it.
    g.apply(APP, AssetType::Logo, "png", b"b").unwrap();

    let predicted = g.removable(APP).len();
    // Premise, or this could pass for the wrong reason on a two-file directory.
    assert_eq!(predicted, 3, "capsule + logo + the logo position sidecar");

    let mut actual = 0;
    for asset in AssetType::EDITABLE {
        actual += g.clear(APP, asset).unwrap().len();
    }
    assert_eq!(
        actual, predicted,
        "the quoted count must match the deletion"
    );
}

#[test]
fn a_stranded_logo_position_is_still_counted_and_removed() {
    // The sidecar can outlive the logo — clearing the logo takes it, but a hand-edited or
    // half-migrated directory can leave one behind. It must not become invisible.
    let (t, g) = dir();
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(t.path().join("4048848997.json"), b"{}").unwrap();

    assert_eq!(g.removable(APP).len(), 1);
    assert_eq!(g.clear(APP, AssetType::Logo).unwrap().len(), 1);
    assert!(g.removable(APP).is_empty());
}

#[test]
fn resetting_every_app_leaves_files_it_does_not_own() {
    // The bulk reset composed end to end, and the property that makes it safe: `clear` only
    // ever removes names it builds itself, so anything else in `grid/` survives — the same
    // guarantee `cache::clear` carries, and for the same reason.
    let (t, g) = dir();
    g.apply(APP, AssetType::Capsule, "png", b"a").unwrap();
    g.apply(APP, AssetType::Logo, "png", b"b").unwrap();
    g.apply(AppId::new(620), AssetType::Hero, "png", b"c")
        .unwrap();
    let bystander = t.path().join("my-notes.txt");
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(&bystander, b"not ours").unwrap();

    let mut removed = 0;
    for app in g.customised_apps().unwrap() {
        for asset in AssetType::EDITABLE {
            removed += g.clear(app, asset).unwrap().len();
        }
    }

    // Four: three images plus the logo's position sidecar, which `apply` created.
    assert_eq!(removed, 4);
    assert!(g.customised_apps().unwrap().is_empty());
    assert!(
        bystander.is_file(),
        "a full reset must not delete files it did not write"
    );
}

#[test]
fn icons_consider_the_ico_extension() {
    let (_t, g) = dir();
    g.apply(APP, AssetType::Icon, "ico", b"icon").unwrap();
    assert_eq!(g.existing(APP, AssetType::Icon).len(), 1);
    // Replacing with a .png must remove the .ico.
    let r = g.apply(APP, AssetType::Icon, "png", b"icon2").unwrap();
    assert_eq!(r.removed.len(), 1);
    assert_eq!(name_of(&r.removed[0]), "4048848997_icon.ico");
    assert_eq!(name_of(&r.written), "4048848997_icon.png");
}
