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

    /// Look up a game by SteamGridDB's own id. `Ok(None)` when there is no such game.
    ///
    /// `GET /games/id/{id}` — **200 with a full record, probed against the live API**
    /// `[VERIFIED-BOX 2026-07-30]`. Used only to name a manual override that was stored before
    /// the name was kept alongside it; a current override needs no request at all.
    pub async fn game_by_id(&self, id: u64) -> Result<Option<Game>, Error> {
        let url = self.build_url(&["games", "id", &id.to_string()], &[])?;
        match self.get_envelope::<Game>(url).await {
            Ok(env) => Ok(env.data),
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
        // Caught locally: a dimension from the wrong endpoint is a 400, which would otherwise
        // surface as "SteamGridDB rejected the request" and read like a service fault.
        query.validate_for(kind).map_err(|_| Error::BadRequest)?;

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
    /// **Sends no `Authorization` header.** Also the reason the download lives in Rust at all:
    /// applying artwork live means handing base64 to Steam's own realm, and JS in that realm
    /// cannot fetch the bytes itself — a plain `fetch` to `cdn2.steamgriddb.com` fails CORS, and
    /// `mode:'no-cors'` yields an opaque response whose body cannot be read
    /// `[VERIFIED-BOX 2026-07-27]`. So Rust downloads and hands over base64.
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
#[path = "client_tests.rs"]
mod tests;
