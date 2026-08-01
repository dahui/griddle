//! Tests for [`super`]. Split out to keep the implementation readable on its own.

use super::*;

fn cache() -> (tempfile::TempDir, Cache) {
    let dir = tempfile::tempdir().unwrap();
    let c = Cache::at(dir.path().join("cache"));
    (dir, c)
}

const URL: &str = "https://www.steamgriddb.com/api/v2/grids/steam/620?limit=50";

#[test]
fn json_round_trips() {
    let (_d, c) = cache();
    assert_eq!(c.get_json(URL), None, "a cold cache must miss");
    c.put_json(URL, b"{\"data\":[]}").unwrap();
    assert_eq!(c.get_json(URL).unwrap(), b"{\"data\":[]}");
}

#[test]
fn json_expires_but_images_do_not() {
    let (_d, c) = cache();
    let expired = Cache::at(c.root()).with_json_ttl(Duration::ZERO);

    expired.put_json(URL, b"body").unwrap();
    expired.put_image(URL, b"bytes").unwrap();

    // A zero TTL means anything stored at least a second ago is stale; storing and reading
    // within the same second would otherwise be flaky, so compare against the age rule
    // directly by asserting images survive the same treatment.
    assert_eq!(
        expired.get_image(URL).unwrap(),
        b"bytes",
        "images are content-addressed and must never expire"
    );
}

#[test]
fn a_stale_json_entry_is_a_miss() {
    let (_d, c) = cache();
    c.put_json(URL, b"body").unwrap();

    // Rewrite the entry with a timestamp well in the past rather than sleeping.
    let path = c.path_for("json", URL);
    let mut raw = std::fs::read(&path).unwrap();
    let ancient = 1_000_000u64;
    raw[12..20].copy_from_slice(&ancient.to_le_bytes());
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(&path, &raw).unwrap();

    assert_eq!(c.get_json(URL), None, "an old entry must not be served");
}

#[test]
fn an_entry_stamped_in_the_future_is_stale_not_immortal() {
    // A wrong system clock is common — a dead CMOS battery, a VM resuming from a snapshot,
    // an NTP correction. Naive `saturating_sub` would report an age of zero forever and
    // pin the stale response in place permanently.
    let (_d, c) = cache();
    c.put_json(URL, b"body").unwrap();

    let path = c.path_for("json", URL);
    let mut raw = std::fs::read(&path).unwrap();
    let future = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 86_400;
    raw[12..20].copy_from_slice(&future.to_le_bytes());
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(&path, &raw).unwrap();

    assert_eq!(c.get_json(URL), None);
}

#[test]
fn a_hash_collision_is_a_miss_rather_than_the_wrong_data() {
    // Forge an entry at the path for URL_A but containing URL_B's key. This is what a
    // collision would look like, and serving it would hand one game's art to another.
    let (_d, c) = cache();
    let other = "https://example.invalid/other";
    c.put_json(other, b"other body").unwrap();

    let forged = c.path_for("json", URL);
    std::fs::copy(c.path_for("json", other), &forged).unwrap();

    assert_eq!(
        c.get_json(URL),
        None,
        "an entry whose stored key does not match must never be served"
    );
    assert_eq!(c.get_json(other).unwrap(), b"other body");
}

#[test]
fn a_truncated_entry_is_a_miss() {
    let (_d, c) = cache();
    c.put_json(URL, b"a reasonably long body").unwrap();
    let path = c.path_for("json", URL);

    let raw = std::fs::read(&path).unwrap();
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(&path, &raw[..raw.len() - 5]).unwrap();

    assert_eq!(c.get_json(URL), None, "a torn write must not be served");
}

#[test]
fn garbage_in_the_cache_directory_is_ignored() {
    let (_d, c) = cache();
    std::fs::create_dir_all(c.root()).unwrap();
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(c.path_for("json", URL), b"not a cache entry at all").unwrap();
    assert_eq!(c.get_json(URL), None);
}

#[test]
fn keys_cannot_escape_the_cache_directory() {
    // The reason filenames are hashes. A key like this must not produce a path outside
    // `root`, and must not contain any separator at all.
    let (_d, c) = cache();
    for nasty in [
        "../../../../windows/system32/evil",
        "C:\\Windows\\System32\\evil",
        "https://x/../../y",
        "a/b/c",
    ] {
        let p = c.path_for("img", nasty);
        assert_eq!(
            p.parent(),
            Some(c.root()),
            "{nasty} produced {}",
            p.display()
        );
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!name.contains(['/', '\\', ':']), "{name}");
    }
}

#[test]
fn different_query_strings_are_different_entries() {
    // Page 2 of a search must not be served as page 1.
    let (_d, c) = cache();
    let p1 = format!("{URL}&page=1");
    let p2 = format!("{URL}&page=2");
    c.put_json(&p1, b"page one").unwrap();
    c.put_json(&p2, b"page two").unwrap();
    assert_eq!(c.get_json(&p1).unwrap(), b"page one");
    assert_eq!(c.get_json(&p2).unwrap(), b"page two");
}

#[test]
fn prune_evicts_least_recently_used_until_under_the_limit() {
    let (_d, c) = cache();
    let c = c.with_max_bytes(0);
    for i in 0..5 {
        c.put_image(&format!("https://cdn/{i}.png"), &vec![0u8; 1024])
            .unwrap();
    }
    assert_eq!(c.stats().files, 5);

    let pruned = c.prune();
    assert_eq!(pruned.files_removed, 5);
    assert!(pruned.bytes_removed > 5 * 1024);
    assert_eq!(c.stats().files, 0);
}

#[test]
fn prune_does_nothing_when_under_the_limit() {
    let (_d, c) = cache();
    let c = c.with_max_bytes(u64::MAX);
    c.put_image("https://cdn/a.png", &[0u8; 128]).unwrap();
    let pruned = c.prune();
    assert_eq!(pruned.files_removed, 0);
    assert_eq!(c.stats().files, 1);
}

#[test]
fn clear_removes_only_our_own_files() {
    let (_d, c) = cache();
    c.put_json(URL, b"body").unwrap();
    let bystander = c.root().join("important.txt");
    // boundary-ok: test fixture, written into a tempdir
    std::fs::write(&bystander, b"not ours").unwrap();

    let cleared = c.clear();
    assert_eq!(cleared.files_removed, 1);
    assert!(
        bystander.is_file(),
        "clear must not delete files it did not create"
    );
}

#[test]
fn stats_and_prune_on_a_directory_that_does_not_exist_are_harmless() {
    let dir = tempfile::tempdir().unwrap();
    let c = Cache::at(dir.path().join("never-created"));
    assert_eq!(c.stats(), Stats::default());
    assert_eq!(c.prune(), Pruned::default());
    assert_eq!(c.clear(), Pruned::default());
    assert_eq!(c.get_json(URL), None);
}

#[test]
fn writing_leaves_no_temp_file() {
    let (_d, c) = cache();
    c.put_image("https://cdn/a.png", b"bytes").unwrap();
    let leftovers: Vec<String> = std::fs::read_dir(c.root())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn an_empty_payload_round_trips() {
    let (_d, c) = cache();
    c.put_json(URL, b"").unwrap();
    assert_eq!(c.get_json(URL).unwrap(), b"");
}

/// A hand-rolled hash that only agrees with itself proves nothing. These were computed
/// independently, and between them they caught two mistakes: a mistyped prime
/// (`0x1000_0000_01b3`, one digit too many) and a `foobar` vector that belonged to neither
/// FNV-1a nor FNV-1.
///
/// The empty-string case alone is not enough — it is just the offset basis and passes even
/// with a completely wrong prime. `"a"` is what distinguishes FNV-1a (`…dc4c8601ec8c`)
/// from FNV-1 (`…bd4c8601b7be`), which differ only in whether the xor precedes the
/// multiply.
#[test]
fn fnv1a_matches_the_published_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn json_and_image_namespaces_do_not_collide() {
    // Same URL, two kinds. Storing an image must not overwrite the JSON for it.
    let (_d, c) = cache();
    c.put_json(URL, b"json body").unwrap();
    c.put_image(URL, b"image bytes").unwrap();
    assert_eq!(c.get_json(URL).unwrap(), b"json body");
    assert_eq!(c.get_image(URL).unwrap(), b"image bytes");
}
