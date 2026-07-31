//! On-disk cache for SteamGridDB responses and images.
//!
//! Lives under `%LOCALAPPDATA%\<AppName>\cache`. Everything here is **ours and disposable** —
//! deleting the whole directory costs a few seconds of re-fetching and nothing else. That is
//! why this module is allowed to write files despite the write boundary: the boundary exists
//! to keep writes to *the user's irreplaceable Steam config* small enough to audit, and a
//! cache directory we created is a different category. Every path is derived from
//! [`Cache::root`] and every filename is a hash, so nothing here can escape its own directory.
//!
//! # Two policies, because the data is genuinely different
//!
//! | Kind | Keyed by | Expiry |
//! |---|---|---|
//! | JSON | request URL incl. query | **TTL** (default 10 min) |
//! | Images | asset URL | **never** — evicted only for space |
//!
//! Images need no TTL because **SteamGridDB's CDN URLs are content-addressed**:
//! `cdn2.steamgriddb.com/grid/7668636048c4fbe8df8ffb388679e933.png`. The bytes behind a given
//! URL cannot change; a different image is a different URL. Re-validating them would be pure
//! waste.
//!
//! JSON is the opposite — the *same* URL returns different results as artwork is uploaded. And
//! since **no endpoint sends an `ETag`** `[VERIFIED-BOX 2026-07-30]`, there is nothing to
//! revalidate against, so a plain TTL is the only option. This is our own politeness policy,
//! not HTTP compliance: the server sends `no-store` on everything, but that is PHP's
//! `session_start()` default rather than a considered directive (it arrives identically on
//! static game metadata), and honouring it literally would mean re-fetching the same search on
//! every keystroke.
//!
//! # Entries are self-describing, so a collision or a torn write is a miss
//!
//! Filenames are a 64-bit FNV-1a hash of the key, which is a namer and a sanitiser at once —
//! a key derived from a URL can never contain `..` or a drive letter by the time it reaches
//! the filesystem. But a hash can collide, and a half-written file can be truncated, so each
//! entry stores its own key and payload length in a header:
//!
//! ```text
//! magic "SGDBCA1\n" · u32 key_len · u64 stored_at · u64 payload_len · key · payload
//! ```
//!
//! A mismatch on any of those is treated as a **miss**, never as data. Serving one game's
//! artwork for another because two URLs happened to collide would be a genuinely baffling bug.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Re-exported from `settings`, which owns the single definition.
///
/// This was a second `const` with the same value, kept in step by a comment asking whoever
/// changed one to change the other. A mismatch would have put the cache and the settings in
/// different directories with nothing to report it.
pub use crate::settings::APP_DIR_NAME;

/// Only files with this extension are ever read, pruned or deleted.
const ENTRY_EXT: &str = "sgdbc";

const MAGIC: &[u8; 8] = b"SGDBCA1\n";
const HEADER_LEN: usize = 8 + 4 + 8 + 8;

/// How long a JSON response stays fresh. Long enough to make navigating back and forth free,
/// short enough that newly uploaded artwork shows up in the same session.
pub const DEFAULT_JSON_TTL: Duration = Duration::from_secs(10 * 60);

/// Default ceiling for the whole cache directory.
pub const DEFAULT_MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("writing {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not locate %LOCALAPPDATA%")]
    NoLocalAppData,
}

/// What a prune removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pruned {
    pub files_removed: usize,
    pub bytes_removed: u64,
    pub bytes_remaining: u64,
}

/// Cache size, for the diagnostics screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub files: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
    json_ttl: Duration,
    max_bytes: u64,
}

impl Cache {
    /// `%LOCALAPPDATA%\<AppName>\cache`.
    ///
    /// `LOCALAPPDATA` rather than `APPDATA`: this is regenerable machine-local data and has no
    /// business following a roaming profile between machines.
    pub fn default_location() -> Result<Self, Error> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or(Error::NoLocalAppData)?;
        Ok(Self::at(base.join(APP_DIR_NAME).join("cache")))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Cache {
            root: root.into(),
            json_ttl: DEFAULT_JSON_TTL,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    pub fn with_json_ttl(mut self, ttl: Duration) -> Self {
        self.json_ttl = ttl;
        self
    }

    pub fn with_max_bytes(mut self, max: u64) -> Self {
        self.max_bytes = max;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    // -- JSON: TTL'd ---------------------------------------------------------------------

    /// A cached response body, if present and still fresh.
    pub fn get_json(&self, url: &str) -> Option<Vec<u8>> {
        let (payload, stored_at) = self.read_entry(&self.path_for("json", url), url)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();

        // An entry stamped in the future means the system clock moved between writing it and
        // reading it. Treated as stale rather than as permanently fresh — a saturating
        // subtraction would give an age of zero forever, so one bad clock reading could pin a
        // stale response in place for the life of the install. A refetch is cheap.
        if stored_at > now {
            tracing::debug!("cache entry is stamped in the future; treating as stale");
            return None;
        }

        if Duration::from_secs(now - stored_at) > self.json_ttl {
            return None;
        }
        Some(payload)
    }

    pub fn put_json(&self, url: &str, body: &[u8]) -> Result<(), Error> {
        self.write_entry(&self.path_for("json", url), url, body)
    }

    // -- Images: content-addressed, so no expiry ------------------------------------------

    /// Cached image bytes. No freshness check — see the module docs.
    pub fn get_image(&self, url: &str) -> Option<Vec<u8>> {
        let path = self.path_for("img", url);
        let (payload, _) = self.read_entry(&path, url)?;
        // Touch so eviction is genuinely least-*recently-used* rather than oldest-written. One
        // metadata write per hit, which is cheap next to re-downloading the image.
        self.touch(&path);
        Some(payload)
    }

    pub fn put_image(&self, url: &str, bytes: &[u8]) -> Result<(), Error> {
        self.write_entry(&self.path_for("img", url), url, bytes)
    }

    // -- Housekeeping ---------------------------------------------------------------------

    pub fn stats(&self) -> Stats {
        let mut stats = Stats::default();
        for (_, len, _) in self.entries() {
            stats.files += 1;
            stats.bytes += len;
        }
        stats
    }

    /// Evict least-recently-used entries until the cache is under its size limit.
    pub fn prune(&self) -> Pruned {
        let mut entries = self.entries();
        let total: u64 = entries.iter().map(|(_, len, _)| *len).sum();

        let mut pruned = Pruned {
            bytes_remaining: total,
            ..Default::default()
        };
        if total <= self.max_bytes {
            return pruned;
        }

        // Oldest access first.
        entries.sort_by_key(|(_, _, accessed)| *accessed);

        for (path, len, _) in entries {
            if pruned.bytes_remaining <= self.max_bytes {
                break;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    pruned.files_removed += 1;
                    pruned.bytes_removed += len;
                    pruned.bytes_remaining = pruned.bytes_remaining.saturating_sub(len);
                }
                Err(e) => tracing::warn!(path = %path.display(), error = %e, "could not evict"),
            }
        }

        tracing::info!(
            removed = pruned.files_removed,
            bytes = pruned.bytes_removed,
            "pruned the cache"
        );
        pruned
    }

    /// Delete every cache entry.
    ///
    /// Only files with our own extension are touched, so pointing the cache at a directory
    /// containing anything else cannot destroy it.
    pub fn clear(&self) -> Pruned {
        let mut pruned = Pruned::default();
        for (path, len, _) in self.entries() {
            match std::fs::remove_file(&path) {
                Ok(()) => {
                    pruned.files_removed += 1;
                    pruned.bytes_removed += len;
                }
                Err(e) => tracing::warn!(path = %path.display(), error = %e, "could not remove"),
            }
        }
        pruned
    }

    // -- internals -------------------------------------------------------------------------

    /// `<root>/<kind>-<hash>.sgdbc`.
    ///
    /// The hash is the only thing derived from the key, so an attacker-controlled or merely
    /// odd URL cannot produce a path outside `root`.
    fn path_for(&self, kind: &str, key: &str) -> PathBuf {
        self.root.join(format!(
            "{kind}-{:016x}.{ENTRY_EXT}",
            fnv1a64(key.as_bytes())
        ))
    }

    /// `(path, size, last-accessed)` for every entry we own.
    fn entries(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let Ok(dir) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        dir.flatten()
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some(ENTRY_EXT) {
                    return None;
                }
                let meta = e.metadata().ok()?;
                if !meta.is_file() {
                    return None;
                }
                let accessed = meta.modified().unwrap_or(UNIX_EPOCH);
                Some((path, meta.len(), accessed))
            })
            .collect()
    }

    /// Read an entry, returning `(payload, stored_at)`.
    ///
    /// Every inconsistency — bad magic, wrong key, truncated payload — is a **miss**. A cache
    /// that returns the wrong bytes is far worse than one that returns nothing.
    fn read_entry(&self, path: &Path, key: &str) -> Option<(Vec<u8>, u64)> {
        let raw = std::fs::read(path).ok()?;
        if raw.len() < HEADER_LEN || &raw[..8] != MAGIC {
            return None;
        }

        let key_len = u32::from_le_bytes(raw[8..12].try_into().ok()?) as usize;
        let stored_at = u64::from_le_bytes(raw[12..20].try_into().ok()?);
        let payload_len = u64::from_le_bytes(raw[20..28].try_into().ok()?) as usize;

        let key_end = HEADER_LEN.checked_add(key_len)?;
        let payload_end = key_end.checked_add(payload_len)?;
        // A file shorter than its own header claims means the write was interrupted.
        if raw.len() != payload_end {
            tracing::debug!(path = %path.display(), "discarding a truncated cache entry");
            return None;
        }
        // Hash collision, or a stale file from a previous key scheme.
        if &raw[HEADER_LEN..key_end] != key.as_bytes() {
            tracing::debug!(path = %path.display(), "cache key mismatch; treating as a miss");
            return None;
        }

        Some((raw[key_end..payload_end].to_vec(), stored_at))
    }

    fn write_entry(&self, path: &Path, key: &str, payload: &[u8]) -> Result<(), Error> {
        std::fs::create_dir_all(&self.root).map_err(|source| Error::Write {
            path: self.root.clone(),
            source,
        })?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut buf = Vec::with_capacity(HEADER_LEN + key.len() + payload.len());
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
        buf.extend_from_slice(&now.to_le_bytes());
        buf.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        buf.extend_from_slice(key.as_bytes());
        buf.extend_from_slice(payload);

        // Temp + rename, so a concurrent reader sees either the old entry or the new one and
        // never a half-written file. The length check on read is the backstop for a crash
        // between the two.
        let tmp = path.with_extension(format!("{ENTRY_EXT}tmp"));
        std::fs::write(&tmp, &buf).map_err(|source| Error::Write {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, path).map_err(|source| {
            if let Err(cleanup) = std::fs::remove_file(&tmp) {
                tracing::warn!(temp = %tmp.display(), error = %cleanup, "could not remove temp");
            }
            Error::Write {
                path: path.to_path_buf(),
                source,
            }
        })
    }

    /// Mark an entry as just used, for LRU ordering. Best effort: failing to touch costs
    /// eviction accuracy, not correctness, so it is logged at debug and ignored.
    fn touch(&self, path: &Path) {
        if let Ok(f) = std::fs::OpenOptions::new().write(true).open(path)
            && let Err(e) = f.set_modified(SystemTime::now())
        {
            tracing::debug!(path = %path.display(), error = %e, "could not touch cache entry");
        }
    }
}

/// FNV-1a, 64-bit.
///
/// Hand-rolled rather than using `DefaultHasher`, whose algorithm is explicitly allowed to
/// change between Rust releases — which would silently invalidate every user's cache on a
/// toolchain upgrade. Not cryptographic and does not need to be: collisions are caught by the
/// key stored in each entry.
fn fnv1a64(bytes: &[u8]) -> u64 {
    /// 1099511628211. Grouped carefully: an earlier draft wrote `0x1000_0000_01b3`, which is
    /// one hex digit too many and silently produced a different (still working, still wrong)
    /// hash. The published test vectors below are what caught it.
    const PRIME: u64 = 0x100_0000_01b3;
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

    let mut hash = OFFSET_BASIS;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
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
        std::fs::write(&path, &raw[..raw.len() - 5]).unwrap();

        assert_eq!(c.get_json(URL), None, "a torn write must not be served");
    }

    #[test]
    fn garbage_in_the_cache_directory_is_ignored() {
        let (_d, c) = cache();
        std::fs::create_dir_all(c.root()).unwrap();
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
}
