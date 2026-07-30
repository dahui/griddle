//! The SteamGridDB API v2 client. **The only place the API key is used.**
//!
//! # What the server actually does, measured `[VERIFIED-BOX 2026-07-30]`
//!
//! | Probe | Result |
//! |---|---|
//! | `/games/steam/620`, `/grids`, `/heroes`, `/logos`, `/icons`, `/search/autocomplete` | 200 |
//! | bad key, and no key at all | **401, empty body** |
//! | unknown Steam appid | **404, empty body** |
//! | a path that is not an endpoint | **404 with a full HTML page** |
//! | `?dimensions=1x1` | **400** — invalid filter values are rejected, not ignored |
//! | `ETag` on any endpoint | **absent** |
//! | `RateLimit-*` / `Retry-After` | absent |
//!
//! Four of those shape the code:
//!
//! 1. **Error bodies are empty**, so every message here is built from the status code. There
//!    is nothing useful to quote back to the user.
//! 2. **A wrong path returns HTML.** The content type is checked before parsing, and the body
//!    is *never* echoed into an error — otherwise a mis-built URL surfaces as ten kilobytes of
//!    markup in a toast.
//! 3. **No `ETag` anywhere**, so the plan's "ETag revalidation" is not available. Caching has
//!    to be time-based, and that belongs in a `cache` module rather than here.
//! 4. **No rate-limit headers**, so there is nothing to honour reactively. The concurrency cap
//!    and the backoff below are entirely self-imposed.
//!
//! # A note on `Cache-Control: no-store`
//!
//! The server sends `no-store, no-cache, must-revalidate` with `expires: Thu, 19 Nov 1981` and
//! `pragma: no-cache`. That exact combination is PHP's `session_start()` default, not a
//! considered caching policy — it arrives identically on every endpoint, including static
//! game metadata. Treating it as authoritative would mean re-fetching the same search on every
//! keystroke, which is worse for SteamGridDB than a short client-side TTL is. Any cache we add
//! should be modest and documented as our own politeness policy rather than HTTP compliance.
//!
//! # Key hygiene
//!
//! The `Authorization` header is attached **per request**, never as a client default. That is
//! what keeps it off [`Client::download`], which fetches from `cdn2.steamgriddb.com` and needs
//! no auth. A default header would quietly attach the user's secret to every image fetch.

use crate::appid::AppId;
use crate::sgdb::key::ApiKey;
use crate::sgdb::model::{Asset, AssetPage, Envelope, Game};
use crate::sgdb::query::{AssetKind, AssetQuery};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use url::Url;

/// SteamGridDB API v2.
pub const DEFAULT_BASE_URL: &str = "https://www.steamgriddb.com/api/v2";

/// Concurrent in-flight requests. Self-imposed: the server publishes no limit, and hammering a
/// free community API is how a client gets blocked.
pub const DEFAULT_MAX_CONCURRENT: usize = 3;

/// Refuse absurd downloads. The largest real asset seen is a 602 KB animated WebP.
pub const MAX_DOWNLOAD_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP 401 — the key is missing, wrong, or revoked. All three look identical from here
    /// (the body is empty), so the message covers all three.
    #[error("SteamGridDB rejected the API key (HTTP 401). Check it in Settings.")]
    Unauthorized,

    /// HTTP 404 — for a game lookup this usually means "SteamGridDB has no entry", not a bug.
    #[error("not found on SteamGridDB (HTTP 404)")]
    NotFound,

    /// HTTP 400 — typically an invalid filter value.
    #[error("SteamGridDB rejected the request (HTTP 400); a filter value is probably invalid")]
    BadRequest,

    #[error("SteamGridDB rate-limited the request (HTTP 429) and it did not recover")]
    RateLimited,

    #[error("SteamGridDB returned HTTP {status}")]
    Server { status: u16 },

    #[error("network error talking to SteamGridDB: {0}")]
    Network(String),

    #[error("the request to SteamGridDB timed out")]
    Timeout,

    /// Got a 2xx that was not JSON — almost always a mis-built URL landing on the website's
    /// HTML 404 page. The body is deliberately not included.
    #[error("expected JSON from {url} but got {content_type}")]
    NotJson { url: String, content_type: String },

    #[error("could not understand SteamGridDB's response: {0}")]
    Decode(String),

    /// `success: false` in the envelope.
    #[error("SteamGridDB reported failure: {0}")]
    Api(String),

    #[error("download exceeded {MAX_DOWNLOAD_BYTES} bytes")]
    DownloadTooLarge,

    #[error("could not build a request URL: {0}")]
    BadUrl(String),
}

impl Error {
    /// Whether retrying could plausibly help.
    ///
    /// 4xx are the client's fault and will fail identically forever; retrying them just wastes
    /// the user's time and the server's.
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::RateLimited | Error::Server { .. } | Error::Network(_) | Error::Timeout
        )
    }
}

/// Tuning knobs. The defaults are the ones the app ships with.
#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    pub user_agent: String,
    pub timeout: Duration,
    pub max_concurrent: usize,
    pub max_retries: u32,
    /// First backoff step; doubles per attempt.
    pub backoff_base: Duration,
    /// Refuse downloads larger than this. A policy knob rather than a constant so the limit
    /// can actually be exercised in a test without transferring 64 MB.
    pub max_download_bytes: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            base_url: DEFAULT_BASE_URL.to_owned(),
            // A default (bot-ish) UA returns 200 — the feared Cloudflare 403 does not happen
            // on API v2 with a valid bearer token `[VERIFIED-BOX 2026-07-27]`. So this is
            // etiquette and identifiability, not a workaround.
            user_agent: format!(
                "{}/{} (SteamGridDB artwork manager for Windows)",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION")
            ),
            timeout: Duration::from_secs(20),
            max_concurrent: DEFAULT_MAX_CONCURRENT,
            max_retries: 3,
            backoff_base: Duration::from_millis(500),
            max_download_bytes: MAX_DOWNLOAD_BYTES,
        }
    }
}

/// Which game to ask about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// A Steam appid — the API resolves it for us.
    Steam(AppId),
    /// A SteamGridDB game id, from a search result.
    Sgdb(u64),
}

impl Target {
    fn segments(self) -> (&'static str, String) {
        match self {
            Target::Steam(id) => ("steam", id.get().to_string()),
            Target::Sgdb(id) => ("game", id.to_string()),
        }
    }
}

pub struct Client {
    http: reqwest::Client,
    key: ApiKey,
    base: Url,
    permits: Arc<Semaphore>,
    config: Config,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `key` renders as a fingerprint, but spell the derive out rather than relying on it.
        f.debug_struct("Client")
            .field("base", &self.base.as_str())
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl Client {
    pub fn new(key: ApiKey) -> Result<Self, Error> {
        Self::with_config(key, Config::default())
    }

    pub fn with_config(key: ApiKey, config: Config) -> Result<Self, Error> {
        let base = Url::parse(&config.base_url).map_err(|e| Error::BadUrl(e.to_string()))?;

        // No default Authorization header: see the module docs. `download` shares this client
        // and must not carry the user's key to the CDN.
        let http = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(config.timeout)
            .build()
            .map_err(|e| Error::Network(e.to_string()))?;

        Ok(Client {
            http,
            key,
            base,
            permits: Arc::new(Semaphore::new(config.max_concurrent.max(1))),
            config,
        })
    }

    /// Confirm the key works, without caring about the payload.
    ///
    /// The first-run flow needs to tell "this key is wrong" from "you are offline" *before* the
    /// user starts browsing, and those are different errors here.
    pub async fn validate_key(&self) -> Result<(), Error> {
        let url = self.build_url(&["games", "steam", "620"], &[])?;
        let _: Envelope<Game> = self.get_envelope(url).await?;
        Ok(())
    }

    /// Look up a game by Steam appid. `Ok(None)` when SteamGridDB has no entry for it.
    pub async fn game_by_steam_appid(&self, appid: AppId) -> Result<Option<Game>, Error> {
        let url = self.build_url(&["games", "steam", &appid.get().to_string()], &[])?;
        match self.get_envelope::<Game>(url).await {
            Ok(env) => Ok(env.data),
            // A game absent from SteamGridDB is an ordinary outcome, not a failure.
            Err(Error::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Search by name. Used by the "wrong game matched" flow.
    pub async fn search(&self, term: &str) -> Result<Vec<Game>, Error> {
        if term.trim().is_empty() {
            return Ok(Vec::new());
        }
        // `term` is pushed as a path segment, so percent-encoding is handled by `Url` — a
        // title containing `/`, `?` or `#` would otherwise rewrite the path.
        let url = self.build_url(&["search", "autocomplete", term], &[])?;
        let env: Envelope<Vec<Game>> = self.get_envelope(url).await?;
        Ok(env.data.unwrap_or_default())
    }

    /// One page of artwork.
    pub async fn assets(
        &self,
        kind: AssetKind,
        target: Target,
        query: &AssetQuery,
    ) -> Result<AssetPage, Error> {
        let (target_kind, target_id) = target.segments();
        let pairs = query.to_pairs();
        let borrowed: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let url = self.build_url(&[kind.path(), target_kind, &target_id], &borrowed)?;

        let env: Envelope<Vec<Asset>> = self.get_envelope(url).await?;
        Ok(AssetPage {
            assets: env.data.unwrap_or_default(),
            page: env.page.unwrap_or(0),
            total: env.total.unwrap_or(0),
            limit: env.limit.unwrap_or(0),
        })
    }

    /// Fetch asset bytes from the CDN.
    ///
    /// **Sends no `Authorization` header.** Also the reason this exists in Rust at all: the
    /// injected BPM bundle cannot do it. A plain `fetch` from `SharedJSContext` to
    /// `cdn2.steamgriddb.com` fails CORS, and `mode:'no-cors'` yields an opaque response whose
    /// body cannot be read `[VERIFIED-BOX 2026-07-27]`. So Rust downloads and hands over
    /// base64.
    pub async fn download(&self, url: &str) -> Result<Vec<u8>, Error> {
        let parsed = Url::parse(url).map_err(|e| Error::BadUrl(e.to_string()))?;
        let permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| Error::Network("client is shutting down".into()))?;

        let response = self
            .http
            .get(parsed)
            .send()
            .await
            .map_err(Self::classify_transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(Self::classify_status(status.as_u16()));
        }

        // Check the advertised length before buffering, so an implausible asset is refused
        // rather than read into memory first.
        let limit = self.config.max_download_bytes;
        if let Some(len) = response.content_length()
            && len > limit as u64
        {
            return Err(Error::DownloadTooLarge);
        }

        // Check again after reading: `Content-Length` is absent on a chunked response, so the
        // header check alone is an optimisation, not the guarantee.
        let bytes = response.bytes().await.map_err(Self::classify_transport)?;
        drop(permit);

        if bytes.len() > limit {
            return Err(Error::DownloadTooLarge);
        }
        Ok(bytes.to_vec())
    }

    // -- plumbing ------------------------------------------------------------------------

    fn build_url(&self, segments: &[&str], query: &[(&str, &str)]) -> Result<Url, Error> {
        let mut url = self.base.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|()| Error::BadUrl("base URL cannot have path segments".into()))?;
            for s in segments {
                path.push(s);
            }
        }
        if !query.is_empty() {
            let mut qs = url.query_pairs_mut();
            for (k, v) in query {
                qs.append_pair(k, v);
            }
        }
        Ok(url)
    }

    /// Send with the concurrency cap, retries and backoff applied, and check the envelope.
    ///
    /// The `success` flag is checked here rather than trusted. In practice this API signals
    /// failure with a status code and an empty body, so `success: false` on a 2xx has not been
    /// observed — but an envelope field that is parsed and then ignored is exactly the kind of
    /// thing that turns into a silently empty result list later.
    async fn get_envelope<T: DeserializeOwned>(&self, url: Url) -> Result<Envelope<T>, Error> {
        let env: Envelope<T> = self.get_json(url).await?;
        if !env.success {
            let detail = if env.errors.is_empty() {
                "no reason given".to_owned()
            } else {
                env.errors.join("; ")
            };
            return Err(Error::Api(detail));
        }
        Ok(env)
    }

    /// Send with the concurrency cap, retries and backoff applied.
    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T, Error> {
        // The permit is held across retries on purpose: a retrying request is still in flight
        // as far as the server is concerned, and letting retries escape the cap would make the
        // cap meaningless exactly when the server is struggling.
        let permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| Error::Network("client is shutting down".into()))?;

        let mut attempt = 0u32;
        let result = loop {
            match self.get_json_once(&url).await {
                Ok(v) => break Ok(v),
                Err(e) if e.is_retryable() && attempt < self.config.max_retries => {
                    let delay = self.backoff(attempt);
                    tracing::warn!(
                        attempt = attempt + 1,
                        max = self.config.max_retries,
                        delay_ms = delay.as_millis(),
                        error = %e,
                        "retrying SteamGridDB request"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                Err(e) => break Err(e),
            }
        };
        drop(permit);
        result
    }

    async fn get_json_once<T: DeserializeOwned>(&self, url: &Url) -> Result<T, Error> {
        let response = self
            .http
            .get(url.clone())
            // Attached per request. Never a client default — see the module docs.
            .header(reqwest::header::AUTHORIZATION, self.key.bearer())
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(Self::classify_transport)?;

        let status = response.status();
        if !status.is_success() {
            // Bodies are empty on this API's own errors, and HTML on a wrong path. There is
            // nothing worth reading, and plenty worth not showing the user.
            return Err(Self::classify_status(status.as_u16()));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        if !content_type.contains("json") {
            return Err(Error::NotJson {
                url: url.as_str().to_owned(),
                content_type: if content_type.is_empty() {
                    "no content type".into()
                } else {
                    content_type
                },
            });
        }

        let text = response.text().await.map_err(Self::classify_transport)?;
        serde_json::from_str(&text).map_err(|e| Error::Decode(e.to_string()))
    }

    fn classify_status(status: u16) -> Error {
        match status {
            400 => Error::BadRequest,
            401 | 403 => Error::Unauthorized,
            404 => Error::NotFound,
            429 => Error::RateLimited,
            s => Error::Server { status: s },
        }
    }

    fn classify_transport(e: reqwest::Error) -> Error {
        if e.is_timeout() {
            Error::Timeout
        } else {
            Error::Network(e.to_string())
        }
    }

    /// Exponential backoff with jitter.
    ///
    /// The jitter matters more than usual here: the app fires several asset requests at once
    /// when a game is opened, so without it a 429 would sync every retry into another burst.
    /// `SystemTime` supplies the entropy rather than pulling in a random-number dependency for
    /// a few milliseconds of spread.
    fn backoff(&self, attempt: u32) -> Duration {
        let base = self.config.backoff_base.as_millis() as u64;
        let step = base.saturating_mul(1u64 << attempt.min(6));
        let jitter = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::from(d.subsec_nanos()) % (step / 2).max(1))
            .unwrap_or(0);
        Duration::from_millis(step + jitter)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const FAKE_KEY: &str = "0123456789abcdef0123456789abcdef";

    async fn client_for(server: &MockServer) -> Client {
        Client::with_config(
            ApiKey::new(FAKE_KEY).unwrap(),
            Config {
                base_url: format!("{}/api/v2", server.uri()),
                // Keep the tests fast; the retry *logic* is what is under test, not the wait.
                backoff_base: Duration::from_millis(1),
                timeout: Duration::from_secs(5),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn grid_body() -> serde_json::Value {
        serde_json::json!({
            "success": true, "page": 0, "total": 424, "limit": 1,
            "data": [{"id": 103243, "url": "https://cdn2.steamgriddb.com/grid/a.png",
                      "thumb": "https://cdn2.steamgriddb.com/thumb/a.jpg",
                      "width": 600, "height": 900, "mime": "image/png",
                      "author": {"name": "Reiisen"}}]
        })
    }

    #[tokio::test]
    async fn sends_the_bearer_token_and_parses_a_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/grids/steam/620"))
            .and(header(
                "authorization",
                format!("Bearer {FAKE_KEY}").as_str(),
            ))
            .and(query_param("dimensions", "600x900,342x482,660x930"))
            .respond_with(ResponseTemplate::new(200).set_body_json(grid_body()))
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let (kind, query) =
            AssetQuery::for_asset_type(crate::grid::names::AssetType::Capsule).unwrap();
        let page = client
            .assets(kind, Target::Steam(AppId::new(620)), &query)
            .await
            .unwrap();

        assert_eq!(page.total, 424);
        assert_eq!(page.assets.len(), 1);
        assert_eq!(page.assets[0].id, 103243);
    }

    #[tokio::test]
    async fn a_401_is_reported_as_a_key_problem_and_is_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            // The count assertion is the real test: retrying a bad key wastes the user's time
            // and cannot possibly succeed.
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let err = client.validate_key().await.unwrap_err();
        assert!(matches!(err, Error::Unauthorized), "{err:?}");
        assert!(err.to_string().contains("Settings"), "{err}");
    }

    #[tokio::test]
    async fn a_429_is_retried_with_backoff_and_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(2)
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(grid_body()))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let page = client
            .assets(
                AssetKind::Grid,
                Target::Steam(AppId::new(620)),
                &AssetQuery::default(),
            )
            .await
            .unwrap();
        assert_eq!(page.assets.len(), 1);
    }

    #[tokio::test]
    async fn retries_are_bounded_and_end_in_the_real_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            // 1 initial attempt + max_retries(3).
            .expect(4)
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let err = client.validate_key().await.unwrap_err();
        assert!(matches!(err, Error::Server { status: 503 }), "{err:?}");
    }

    #[tokio::test]
    async fn an_html_404_page_is_not_parsed_as_json() {
        // A mis-built URL lands on the website's HTML 404. Verified real behaviour: the body
        // is a full page. It must never reach the user as an error message.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "<!DOCTYPE html><html><head><title>404</title>".as_bytes(),
                "text/html; charset=utf-8",
            ))
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let err = client.validate_key().await.unwrap_err();
        match &err {
            Error::NotJson { content_type, .. } => assert!(content_type.contains("text/html")),
            other => panic!("expected NotJson, got {other:?}"),
        }
        assert!(
            !err.to_string().contains("DOCTYPE"),
            "the HTML body must not be echoed into the error: {err}"
        );
    }

    #[tokio::test]
    async fn an_unknown_game_is_none_rather_than_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let got = client
            .game_by_steam_appid(AppId::new(999_999_999))
            .await
            .unwrap();
        assert_eq!(got, None, "a game SGDB has never heard of is not a failure");
    }

    #[tokio::test]
    async fn a_search_term_with_url_characters_is_encoded_into_the_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            // If the term were interpolated raw, the `/` and `?` would rewrite the path and
            // this mock would never match.
            .and(path("/api/v2/search/autocomplete/Portal%202%3F%20%2Fslash"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"success": true, "data": []})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let hits = client.search("Portal 2? /slash").await.unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn an_empty_search_term_never_hits_the_network() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        assert!(client.search("   ").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn downloads_carry_no_authorization_header() {
        // The key must not reach the CDN. A default header on the client would break this
        // silently, which is why auth is attached per request.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/grid/a.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0x89, b'P', b'N', b'G']))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let bytes = client
            .download(&format!("{}/grid/a.png", server.uri()))
            .await
            .unwrap();
        assert_eq!(bytes, vec![0x89, b'P', b'N', b'G']);

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].headers.get("authorization").is_none(),
            "the API key must never be sent to the CDN"
        );
    }

    #[tokio::test]
    async fn an_oversized_download_is_refused() {
        // An earlier version of this test had the mock advertise a false `content-length`.
        // hyper rejects that mismatch outright, so the test was exercising a broken server
        // rather than the size limit. Sending real bytes past a small configured limit tests
        // the thing that actually ships.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 4096]))
            .mount(&server)
            .await;

        let client = Client::with_config(
            ApiKey::new(FAKE_KEY).unwrap(),
            Config {
                base_url: format!("{}/api/v2", server.uri()),
                max_download_bytes: 64,
                ..Default::default()
            },
        )
        .unwrap();

        let err = client
            .download(&format!("{}/huge.png", server.uri()))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::DownloadTooLarge), "{err:?}");
    }

    #[tokio::test]
    async fn a_download_within_the_limit_succeeds() {
        // The control for the test above: without it, a limit of zero would also "pass".
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7u8; 32]))
            .mount(&server)
            .await;

        let client = Client::with_config(
            ApiKey::new(FAKE_KEY).unwrap(),
            Config {
                base_url: format!("{}/api/v2", server.uri()),
                max_download_bytes: 64,
                ..Default::default()
            },
        )
        .unwrap();

        let bytes = client
            .download(&format!("{}/ok.png", server.uri()))
            .await
            .unwrap();
        assert_eq!(bytes.len(), 32);
    }

    #[tokio::test]
    async fn a_success_false_envelope_on_a_200_is_still_an_error() {
        // Not observed in the wild — this API signals failure with a status code — but the
        // flag is parsed, so it must be acted on rather than decoratively present.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": false, "errors": ["Game not found", "try again"]
            })))
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let err = client.validate_key().await.unwrap_err();
        match &err {
            Error::Api(detail) => assert_eq!(detail, "Game not found; try again"),
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn only_transient_failures_are_retryable() {
        assert!(Error::RateLimited.is_retryable());
        assert!(Error::Timeout.is_retryable());
        assert!(Error::Server { status: 502 }.is_retryable());
        assert!(Error::Network("reset".into()).is_retryable());

        // Retrying any of these is pure waste — they will fail identically forever.
        assert!(!Error::Unauthorized.is_retryable());
        assert!(!Error::NotFound.is_retryable());
        assert!(!Error::BadRequest.is_retryable());
        assert!(!Error::Decode("bad".into()).is_retryable());
    }

    #[test]
    fn backoff_grows_and_stays_bounded() {
        let c = Client::with_config(ApiKey::new(FAKE_KEY).unwrap(), Config::default()).unwrap();
        let d0 = c.backoff(0);
        let d3 = c.backoff(3);
        assert!(d0 >= Duration::from_millis(500));
        assert!(d3 >= Duration::from_millis(4000), "{d3:?}");
        assert!(d3 < Duration::from_secs(10), "{d3:?}");
        // A large attempt count must not overflow into a nonsensical wait.
        assert!(c.backoff(64) < Duration::from_secs(120));
    }

    #[test]
    fn debug_output_never_contains_the_key() {
        let c = Client::with_config(ApiKey::new(FAKE_KEY).unwrap(), Config::default()).unwrap();
        let rendered = format!("{c:?}");
        assert!(!rendered.contains(FAKE_KEY), "{rendered}");
    }
}
