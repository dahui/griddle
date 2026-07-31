//! The SteamGridDB API key, wrapped so it cannot leak by accident.
//!
//! # Why this is a type and not a `String`
//!
//! The key is a **per-user secret**. Every v2 endpoint 401s without one (verified: both a bad
//! key and no key return 401 with an empty body), so one *will* get pasted into a terminal, a
//! log line or a test during development. The repo already defends git with
//! `scripts/check-secrets.sh`; this defends the running process.
//!
//! [`ApiKey`] has a hand-written `Debug` that prints a fingerprint instead of the value, and
//! **deliberately implements neither `Display` nor `Serialize`**. That combination means:
//!
//! - `tracing::info!(?key)` and `{:?}` print `ApiKey(a5f1…, 32 chars)`, never the secret;
//! - `format!("{key}")` does not compile;
//! - it cannot be serialised into a settings file or an RPC response by mistake.
//!
//! Reading it out requires [`ApiKey::expose`], which is named to be conspicuous at a call site
//! and in review.
//!
//! # 🔴 Do not ship a key with the application
//!
//! decky-steamgriddb hardcodes one, and using it elsewhere is stated to get you banned. It now
//! returns **401** anyway `[VERIFIED-BOX 2026-07-27]` — a shared secret inside a distributed
//! binary gets scraped, abused, and revoked, and then every install breaks at once. Each user
//! supplies their own.

use std::fmt;

/// Length of the keys SteamGridDB issues today, used only for a readability hint.
const OBSERVED_KEY_LEN: usize = 32;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeyError {
    #[error("the API key is empty")]
    Empty,

    #[error("the API key contains whitespace — check for a stray line break or a copied label")]
    ContainsWhitespace,

    #[error("the API key contains control characters")]
    ContainsControl,
}

/// A SteamGridDB API key.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// Validate and wrap a key.
    ///
    /// Tolerant on purpose about *shape*: a leading `Bearer ` is stripped (people copy it from
    /// docs) and surrounding whitespace is trimmed. But **the 32-hex form is not enforced**,
    /// only noted — that is today's observed format, not a documented guarantee, and rejecting
    /// a future key format would brick the app for no safety benefit. The checks that remain
    /// are the ones that catch real paste accidents.
    pub fn new(raw: impl AsRef<str>) -> Result<Self, KeyError> {
        let trimmed = raw.as_ref().trim();

        // Split on the first whitespace rather than matching a literal `"Bearer "` prefix.
        // Trimming first eats the separating space, so `strip_prefix("Bearer ")` would fail on
        // the input `"Bearer "` and leave the word itself behind as the "key" — which is how
        // this originally accepted a 6-character key called `Bearer`. Splitting also makes the
        // match case-insensitive for free.
        let trimmed = match trimmed.split_once(char::is_whitespace) {
            Some((first, rest)) if first.eq_ignore_ascii_case("bearer") => rest.trim(),
            // The label on its own with no key after it. Reporting "empty" is far more use
            // than sending `Authorization: Bearer Bearer` and surfacing the resulting 401 as
            // "your key was rejected".
            None if trimmed.eq_ignore_ascii_case("bearer") => "",
            _ => trimmed,
        };

        if trimmed.is_empty() {
            return Err(KeyError::Empty);
        }
        if trimmed.chars().any(char::is_whitespace) {
            return Err(KeyError::ContainsWhitespace);
        }
        if trimmed.chars().any(char::is_control) {
            return Err(KeyError::ContainsControl);
        }

        if trimmed.len() != OBSERVED_KEY_LEN || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            // Accepted anyway — see the doc comment. Logged without the value.
            tracing::debug!(
                len = trimmed.len(),
                "API key is not the usual 32-hex shape; accepting it regardless"
            );
        }

        Ok(ApiKey(trimmed.to_owned()))
    }

    /// Read the secret. Named to be visible in review and at the call site.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// The `Authorization` header value.
    pub fn bearer(&self) -> String {
        format!("Bearer {}", self.0)
    }

    /// A short, non-reversible hint for diagnostics: the first four characters and the length.
    ///
    /// Enough to tell "the wrong key is configured" from "the key is missing" while reading a
    /// log, without putting the secret in it.
    pub fn fingerprint(&self) -> String {
        let head: String = self.0.chars().take(4).collect();
        format!("{head}…, {} chars", self.0.len())
    }
}

// No `Display`, and no `Serialize`. Both omissions are load-bearing: they turn an accidental
// leak into a compile error rather than a runtime surprise.
impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ApiKey({})", self.fingerprint())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    /// Not a real key — 32 hex characters that spell nothing and match no known secret.
    const FAKE: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn debug_never_contains_the_secret() {
        let key = ApiKey::new(FAKE).unwrap();
        let rendered = format!("{key:?}");
        assert!(!rendered.contains(FAKE), "Debug leaked the key: {rendered}");
        assert!(rendered.contains("0123…"), "{rendered}");
        assert!(rendered.contains("32 chars"), "{rendered}");
    }

    #[test]
    fn a_pasted_bearer_prefix_is_stripped() {
        // Copying the header out of the API docs is the obvious mistake to absorb.
        assert_eq!(
            ApiKey::new(format!("Bearer {FAKE}")).unwrap().expose(),
            FAKE
        );
        assert_eq!(
            ApiKey::new(format!("bearer {FAKE}")).unwrap().expose(),
            FAKE
        );
        assert_eq!(ApiKey::new(format!("  {FAKE}\r\n")).unwrap().expose(), FAKE);
    }

    #[test]
    fn the_bearer_header_is_built_correctly() {
        assert_eq!(
            ApiKey::new(FAKE).unwrap().bearer(),
            format!("Bearer {FAKE}")
        );
    }

    #[test]
    fn paste_accidents_are_rejected_with_specific_reasons() {
        assert_eq!(ApiKey::new(""), Err(KeyError::Empty));
        assert_eq!(ApiKey::new("   "), Err(KeyError::Empty));
        // Regression: this once returned Ok with the literal word "Bearer" as the key, because
        // trimming ran before the prefix strip.
        assert_eq!(ApiKey::new("Bearer "), Err(KeyError::Empty));
        assert_eq!(ApiKey::new("Bearer"), Err(KeyError::Empty));
        assert_eq!(
            ApiKey::new("abc def"),
            Err(KeyError::ContainsWhitespace),
            "a space usually means a label got copied too"
        );
        assert_eq!(ApiKey::new("abc\u{7}def"), Err(KeyError::ContainsControl));
    }

    #[test]
    fn an_unusual_shape_is_accepted_rather_than_rejected() {
        // The 32-hex form is observed, not guaranteed. Refusing a future format would break
        // the app for every user at once, which is far worse than accepting an odd string and
        // letting the server return 401.
        let odd = ApiKey::new("sgdb_live_ZZZZ-not-hex-at-all").unwrap();
        assert_eq!(odd.expose(), "sgdb_live_ZZZZ-not-hex-at-all");
    }

    #[test]
    fn fingerprints_differ_between_keys_but_reveal_nothing_usable() {
        let a = ApiKey::new(FAKE).unwrap();
        let b = ApiKey::new("fedcba9876543210fedcba9876543210").unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
        assert!(a.fingerprint().len() < 20);
    }
}
