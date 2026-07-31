//! Finding Steam's `SharedJSContext` on the CEF remote-debugging port — and refusing to touch
//! anything that is not it.
//!
//! # 🔴 Port 8080 is a very common dev-server port
//!
//! Valve's debugger listens on 8080 and there is no authentication on it. That is Valve's
//! design and it is loopback-only, but it means **something else may well be answering**: a
//! Vite dev server, a Jenkins, a Tomcat, a Python `http.server`. Evaluating JavaScript into
//! whatever happens to be there would be inexcusable.
//!
//! So identification is positive, not by elimination. A target is Steam only if it satisfies
//! *all* of:
//!
//! | Check | Observed value `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]` |
//! |---|---|
//! | `/json/version` `Browser` | `Chrome/126.0.6478.183` |
//! | `/json/version` `User-Agent` | contains `Valve Steam Client` |
//! | target `type` | `page` |
//! | target `title` | `SharedJSContext` |
//! | target `url` | on `steamloopback.host` |
//!
//! Anything else is [`Error::NotSteam`], which the UI reports as "port 8080 is in use by
//! another application" — a legible message rather than a mysterious failure.
//!
//! # 🔴 `127.0.0.1`, never `localhost`
//!
//! `localhost` can resolve to `::1` first, and Steam binds IPv4. Using the literal address
//! removes a whole class of "works on my machine".

use serde::Deserialize;

/// Valve's fixed debugging port.
pub const DEFAULT_PORT: u16 = 8080;

/// **Not `localhost`.** See the module docs.
pub const LOOPBACK: &str = "127.0.0.1";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "nothing is listening on {host}:{port}. Steam must be running, and it must have been \
         started after the .cef-enable-remote-debugging file was created."
    )]
    NotListening { host: String, port: u16 },

    #[error(
        "something is listening on {host}:{port}, but it is not Steam ({detail}). \
         Port {port} is a common development-server port; close the other application or \
         change its port."
    )]
    NotSteam {
        host: String,
        port: u16,
        detail: String,
    },

    #[error(
        "Steam's debugger is reachable but has no SharedJSContext yet. It usually appears a \
         few seconds after the client starts."
    )]
    NoSharedJsContext,

    #[error("talking to the debugger on {host}:{port}: {source}")]
    Http {
        host: String,
        port: u16,
        #[source]
        source: reqwest::Error,
    },
}

/// One entry from `/json`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Target {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(rename = "webSocketDebuggerUrl", default)]
    pub websocket_url: String,
}

impl Target {
    /// Whether this is the realm we want.
    ///
    /// Three independent signals rather than one: the title alone would match a page a website
    /// could choose to call `SharedJSContext`, and the URL alone would match Steam's other
    /// loopback pages (the Big Picture document, context menus, the overlay).
    pub fn is_shared_js_context(&self) -> bool {
        self.kind == "page"
            && self.title == "SharedJSContext"
            && self.url.contains("steamloopback.host")
            && !self.websocket_url.is_empty()
    }
}

/// `/json/version`, used to confirm the listener really is the Steam client.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Version {
    #[serde(rename = "Browser", default)]
    pub browser: String,
    #[serde(rename = "User-Agent", default)]
    pub user_agent: String,
    #[serde(rename = "Protocol-Version", default)]
    pub protocol_version: String,
}

impl Version {
    /// Steam's CEF identifies itself in the user agent. `[VERIFIED-BOX 2026-07-27]`
    pub fn looks_like_steam(&self) -> bool {
        self.user_agent.contains("Valve Steam Client")
    }
}

/// Where the debugger is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Default for Endpoint {
    fn default() -> Self {
        Endpoint {
            host: LOOPBACK.to_owned(),
            port: DEFAULT_PORT,
        }
    }
}

impl Endpoint {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Endpoint {
            host: host.into(),
            port,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}:{}{}", self.host, self.port, path)
    }
}

/// Confirm the listener is Steam, then find `SharedJSContext`.
///
/// Deliberately ordered: the identity check happens **before** the target list is even read,
/// so a non-Steam server never gets so far as having its pages inspected.
pub async fn discover(http: &reqwest::Client, endpoint: &Endpoint) -> Result<Target, Error> {
    let version = fetch_version(http, endpoint).await?;
    if !version.looks_like_steam() {
        return Err(Error::NotSteam {
            host: endpoint.host.clone(),
            port: endpoint.port,
            detail: describe(&version),
        });
    }

    let targets = fetch_targets(http, endpoint).await?;
    targets
        .into_iter()
        .find(Target::is_shared_js_context)
        .ok_or(Error::NoSharedJsContext)
}

/// Everything `/json` lists. Exposed for the diagnostics screen.
pub async fn fetch_targets(
    http: &reqwest::Client,
    endpoint: &Endpoint,
) -> Result<Vec<Target>, Error> {
    let body = http
        .get(endpoint.url("/json"))
        .send()
        .await
        .map_err(|source| classify(endpoint, source))?
        .text()
        .await
        .map_err(|source| classify(endpoint, source))?;

    // Read as text and parse ourselves rather than using reqwest's `json`, matching
    // `sgdb::client`: whatever is on this port may return anything at all, and the body is
    // never echoed into an error.
    serde_json::from_str::<Vec<Target>>(&body).map_err(|_| Error::NotSteam {
        host: endpoint.host.clone(),
        port: endpoint.port,
        detail: "/json did not return a CDP target list".to_owned(),
    })
}

pub async fn fetch_version(http: &reqwest::Client, endpoint: &Endpoint) -> Result<Version, Error> {
    let body = http
        .get(endpoint.url("/json/version"))
        .send()
        .await
        .map_err(|source| classify(endpoint, source))?
        .text()
        .await
        .map_err(|source| classify(endpoint, source))?;

    serde_json::from_str::<Version>(&body).map_err(|_| Error::NotSteam {
        host: endpoint.host.clone(),
        port: endpoint.port,
        detail: "/json/version did not return CDP version JSON".to_owned(),
    })
}

fn describe(v: &Version) -> String {
    if v.browser.is_empty() && v.user_agent.is_empty() {
        "no browser identification".to_owned()
    } else {
        format!("browser {:?}", v.browser)
    }
}

/// A connection refusal is "Steam is not running", not an error worth a stack trace. Anything
/// else is reported as-is.
fn classify(endpoint: &Endpoint, source: reqwest::Error) -> Error {
    if source.is_connect() {
        Error::NotListening {
            host: endpoint.host.clone(),
            port: endpoint.port,
        }
    } else {
        Error::Http {
            host: endpoint.host.clone(),
            port: endpoint.port,
            source,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    /// The real `/json` entry for SharedJSContext on this machine.
    /// `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]`
    fn real_shared_js_context() -> Target {
        Target {
            id: "F1A2B3".into(),
            title: "SharedJSContext".into(),
            url: "https://steamloopback.host/index.html?debug=1".into(),
            kind: "page".into(),
            websocket_url: "ws://127.0.0.1:8080/devtools/page/F1A2B3".into(),
        }
    }

    #[test]
    fn recognises_the_real_shared_js_context() {
        assert!(real_shared_js_context().is_shared_js_context());
    }

    #[test]
    fn every_identifying_signal_is_load_bearing() {
        // Drop each one in turn; all four must be required. A title-only check would match a
        // web page that simply chose that title.
        let mut t = real_shared_js_context();
        t.kind = "iframe".into();
        assert!(!t.is_shared_js_context(), "type must be page");

        let mut t = real_shared_js_context();
        t.title = "Steam Big Picture Mode".into();
        assert!(!t.is_shared_js_context(), "title must match exactly");

        let mut t = real_shared_js_context();
        t.url = "https://example.com/index.html".into();
        assert!(!t.is_shared_js_context(), "must be on steamloopback.host");

        let mut t = real_shared_js_context();
        t.websocket_url = String::new();
        assert!(!t.is_shared_js_context(), "unusable without a socket URL");
    }

    #[test]
    fn steams_other_loopback_pages_are_not_mistaken_for_the_realm() {
        // With Big Picture open the target list gains a separate page, and popups appear as
        // their own targets. Picking one of those would put our code in a document that is
        // never displayed, which is precisely the mistake probes 6-8 made.
        for (title, url) in [
            (
                "Steam Big Picture Mode",
                "https://steamloopback.host/routes/bpm",
            ),
            ("Menu", "https://steamloopback.host/contextmenu"),
            ("Steam", "https://steamloopback.host/routes/library/home"),
        ] {
            let t = Target {
                title: title.into(),
                url: url.into(),
                kind: "page".into(),
                websocket_url: "ws://127.0.0.1:8080/devtools/page/X".into(),
                ..real_shared_js_context()
            };
            assert!(!t.is_shared_js_context(), "{title} must not match");
        }
    }

    #[test]
    fn a_page_impersonating_the_title_off_loopback_is_rejected() {
        let t = Target {
            title: "SharedJSContext".into(),
            url: "https://evil.example/index.html".into(),
            ..real_shared_js_context()
        };
        assert!(!t.is_shared_js_context());
    }

    #[test]
    fn the_steam_user_agent_is_what_identifies_the_listener() {
        let steam = Version {
            browser: "Chrome/126.0.6478.183".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                         (KHTML, like Gecko) Valve Steam Client/default/0 Safari/537.36"
                .into(),
            protocol_version: "1.3".into(),
        };
        assert!(steam.looks_like_steam());

        // A plain Chrome with remote debugging on the same port must not be accepted.
        let chrome = Version {
            browser: "Chrome/126.0.6478.183".into(),
            user_agent: "Mozilla/5.0 (Windows NT 10.0) Chrome/126.0.0.0 Safari/537.36".into(),
            protocol_version: "1.3".into(),
        };
        assert!(
            !chrome.looks_like_steam(),
            "matching the browser string alone is not enough"
        );

        assert!(!Version::default().looks_like_steam());
    }

    #[test]
    fn endpoint_defaults_to_the_literal_loopback_address() {
        let e = Endpoint::default();
        assert_eq!(e.host, "127.0.0.1");
        assert_ne!(e.host, "localhost", "localhost can resolve to ::1 first");
        assert_eq!(e.port, 8080);
        assert_eq!(e.url("/json"), "http://127.0.0.1:8080/json");
    }

    #[test]
    fn targets_deserialize_from_the_real_json_shape() {
        let json = r#"[
          {"description":"","devtoolsFrontendUrl":"/devtools/inspector.html?ws=...",
           "id":"637D...","title":"SharedJSContext",
           "type":"page","url":"https://steamloopback.host/index.html",
           "webSocketDebuggerUrl":"ws://127.0.0.1:8080/devtools/page/637D"},
          {"id":"AAAA","title":"Menu","type":"page",
           "url":"https://steamloopback.host/contextmenu",
           "webSocketDebuggerUrl":"ws://127.0.0.1:8080/devtools/page/AAAA"}
        ]"#;
        let targets: Vec<Target> = serde_json::from_str(json).unwrap();
        assert_eq!(targets.len(), 2);

        let found: Vec<_> = targets
            .iter()
            .filter(|t| t.is_shared_js_context())
            .collect();
        assert_eq!(found.len(), 1, "exactly one target may match");
        assert_eq!(found[0].id, "637D...");
    }

    #[test]
    fn a_target_missing_optional_fields_still_parses() {
        // Robustness against a CEF version that omits a field, rather than failing the whole
        // discovery and reporting "Steam not found".
        let t: Target = serde_json::from_str(r#"{"id":"X","type":"page"}"#).unwrap();
        assert_eq!(t.id, "X");
        assert!(!t.is_shared_js_context());
    }

    #[test]
    fn the_not_steam_error_names_the_port_conflict() {
        let e = Error::NotSteam {
            host: "127.0.0.1".into(),
            port: 8080,
            detail: "browser \"Werkzeug/3.0.1\"".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("not Steam"), "{msg}");
        assert!(msg.contains("8080"), "{msg}");
        // The message has to tell the user what to actually do about it.
        assert!(msg.contains("development-server port"), "{msg}");
    }
}
