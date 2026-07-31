//! Talking to Steam's own JavaScript realm over its CEF remote-debugging port.
//!
//! **This is the whole thesis of the project.** Steam Art Manager, SGDBoop and BoilR all write
//! files and need a Steam restart to show new art. The Decky plugin does not, because it calls
//! `SteamClient.Apps.SetCustomArtworkForApp` from *inside* Steam's JS realm. That realm is
//! reachable from a normal Windows process over Valve's own debugging port — no DLL injection,
//! no patched files, no Millennium.
//!
//! Measured: applying a capsule to a real shortcut took **28 ms and the library updated with no
//! restart**. `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27 — confirmed on screen]`
//!
//! - [`sentinel`] — the `.cef-enable-remote-debugging` opt-in file.
//! - [`target`] — finding `SharedJSContext`, and refusing anything that is not Steam.
//! - [`client`] — the CDP websocket and `Runtime.evaluate`.
//! - [`SteamJs`] — the operations we actually need, below.
//!
//! # Ordering, and why it is not negotiable
//!
//! 1. The sentinel must exist **and Steam must have started after it was created**. Creating
//!    it is always an explicit user action; we never create it silently.
//! 2. The listener on the port must identify as Steam before anything is evaluated.
//! 3. `SteamClient.Apps.SetCustomArtworkForApp` must be feature-detected with `typeof` before
//!    use. All four artwork functions report `.length === 0` because they are native bindings,
//!    so **arity is not a usable signal** — checking `fn.length` would reject working builds.

pub mod client;
pub mod sentinel;
pub mod target;

pub use client::Connection;
pub use sentinel::Sentinel;
pub use target::{Endpoint, Target};

use crate::appid::AppId;
use crate::base64;
use crate::grid::names::AssetType;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Target(#[from] target::Error),

    #[error(transparent)]
    Cdp(#[from] client::Error),

    #[error(
        "Steam's artwork API is not available in this build. Artwork will be applied by \
         writing files instead, which needs a Steam restart to show up."
    )]
    ApiMissing,

    #[error("{0} cannot be set through Steam's artwork API")]
    AssetNotSettable(AssetType),

    #[error("refusing to send a payload that is not plain base64")]
    NotBase64,

    #[error("refusing to apply empty image data")]
    EmptyImage,
}

/// What the readiness probe found.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Readiness {
    /// Steam's build stamp, e.g. `10840511`. The cache key for the module map.
    pub clstamp: Option<String>,
    /// `typeof SteamClient.Apps.SetCustomArtworkForApp === 'function'`.
    pub apply_api: bool,
    /// `Array.isArray(window.webpackChunksteamui)` — the module-discovery hook.
    pub webpack: bool,
}

impl Readiness {
    /// Whether live apply can be used at all.
    pub fn can_apply(&self) -> bool {
        self.apply_api
    }

    /// Whether the Big Picture UI can be injected.
    pub fn can_inject_ui(&self) -> bool {
        self.apply_api && self.webpack
    }
}

/// The operations this product needs from Steam's realm.
pub struct SteamJs {
    connection: Connection,
}

impl SteamJs {
    /// Find Steam, connect, and confirm the realm is usable.
    pub async fn connect(
        http: &reqwest::Client,
        endpoint: &Endpoint,
    ) -> Result<(Self, Readiness), Error> {
        let target = target::discover(http, endpoint).await?;
        let connection = Connection::connect(&target.websocket_url).await?;
        let mut steam = SteamJs { connection };
        let readiness = steam.probe().await?;
        Ok((steam, readiness))
    }

    pub fn connection(&mut self) -> &mut Connection {
        &mut self.connection
    }

    /// Feature-detect, rather than assume.
    ///
    /// `SharedJSContext` exists long before it is useful — it appears within a second of Steam
    /// starting, while `SteamClient` is populated later. So this is also the readiness gate,
    /// and it doubles as the check that decides whether to degrade to file-writing *before*
    /// anything is injected.
    pub async fn probe(&mut self) -> Result<Readiness, Error> {
        let expr = r#"
            (() => ({
                clstamp: (typeof CLSTAMP !== 'undefined') ? String(CLSTAMP) : null,
                apply_api: typeof window.SteamClient?.Apps?.SetCustomArtworkForApp === 'function',
                webpack: Array.isArray(window.webpackChunksteamui),
            }))()
        "#;
        Ok(self.connection.evaluate::<Readiness>(expr).await?)
    }

    /// Steam's build stamp, which is also readable from `steamui/changelist.txt` on disk.
    ///
    /// Being able to read it from both is what makes the build-stamped module map work: cache
    /// the resolved modules against a stamp, and on a change re-run every finder and diff.
    pub async fn clstamp(&mut self) -> Result<Option<String>, Error> {
        Ok(self
            .connection
            .evaluate::<Option<String>>("(typeof CLSTAMP !== 'undefined') ? String(CLSTAMP) : null")
            .await?)
    }

    /// Apply artwork live. **No Steam restart.**
    ///
    /// The mime argument is the literal `"png"` regardless of what the bytes actually are —
    /// that is not a workaround, it is what Valve's own code does. Chromium sniffs content
    /// rather than trusting the label, which is why a 45-frame animated WebP labelled `png`
    /// lands at `<appid>p.png` and animates in both the desktop library and Big Picture.
    /// `[VERIFIED-BOX 2026-07-27]`
    pub async fn apply_artwork(
        &mut self,
        app: AppId,
        asset: AssetType,
        image: &[u8],
    ) -> Result<(), Error> {
        if image.is_empty() {
            return Err(Error::EmptyImage);
        }
        // Icon and HeroBlur are silent no-ops through this API: ordinal 4 takes ~500 ms and
        // writes nothing at all, for shortcuts and real Steam apps alike (S8). Refusing here
        // is better than a call that appears to succeed and does nothing.
        if !asset.supports_live_apply() {
            return Err(Error::AssetNotSettable(asset));
        }

        let payload = base64::encode(image);
        Ok(self
            .connection
            .evaluate_unit(&apply_expression(app, asset, &payload)?)
            .await?)
    }

    /// Remove custom artwork, restoring Steam's own.
    pub async fn clear_artwork(&mut self, app: AppId, asset: AssetType) -> Result<(), Error> {
        if !asset.supports_live_apply() {
            return Err(Error::AssetNotSettable(asset));
        }
        let expr = format!(
            "window.SteamClient.Apps.ClearCustomArtworkForApp({}, {})",
            app.get(),
            asset as u32
        );
        Ok(self.connection.evaluate_unit(&expr).await?)
    }

    /// Whether Steam knows this appid, and what it calls it. Useful for confirming a shortcut
    /// id resolves before applying anything to it.
    pub async fn app_name(&mut self, app: AppId) -> Result<Option<String>, Error> {
        let expr = format!(
            "(() => {{ const o = window.appStore?.GetAppOverviewByAppID({}); \
             return o ? String(o.display_name ?? o.name ?? '') : null; }})()",
            app.get()
        );
        Ok(self.connection.evaluate::<Option<String>>(&expr).await?)
    }
}

/// Build the apply expression.
///
/// 🔑 **The payload is validated as base64 before being spliced into a JavaScript string
/// literal.** The base64 alphabet contains no quote, backslash or newline, so a value that
/// passes [`base64::is_base64`] provably cannot break out of the literal — the check is an
/// injection guarantee, not a tidiness test. The other two interpolations are integers.
///
/// The alternative, sending the payload as a `Runtime.callFunctionOn` argument, is not obviously
/// safer and is more moving parts; an 802 KB literal crossed CDP without complaint.
fn apply_expression(app: AppId, asset: AssetType, base64_payload: &str) -> Result<String, Error> {
    if !base64::is_base64(base64_payload) {
        return Err(Error::NotBase64);
    }
    Ok(format!(
        "window.SteamClient.Apps.SetCustomArtworkForApp({}, \"{}\", \"png\", {})",
        app.get(),
        base64_payload,
        asset as u32
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    const SHORTCUT: AppId = AppId::new(4_048_848_997);

    #[test]
    fn the_apply_expression_matches_what_valve_does() {
        let expr = apply_expression(SHORTCUT, AssetType::Capsule, "QUJD").unwrap();
        assert_eq!(
            expr,
            "window.SteamClient.Apps.SetCustomArtworkForApp(4048848997, \"QUJD\", \"png\", 0)"
        );
        // The unsigned appid crosses the CDP boundary, not the signed one stored in
        // shortcuts.vdf. Getting this wrong writes art nothing ever reads.
        assert!(expr.contains("4048848997"));
        assert!(!expr.contains("-246118299"));
        // Valve hardcodes the mime. Chromium sniffs content, so this is the mechanism, not a bug.
        assert!(expr.contains("\"png\""));
    }

    #[test]
    fn each_asset_type_uses_its_measured_ordinal() {
        // Applied one at a time against a real shortcut and a real Steam app, watching which
        // file appeared. An off-by-one here writes hero art into the capsule slot.
        for (asset, ordinal) in [
            (AssetType::Capsule, 0),
            (AssetType::Hero, 1),
            (AssetType::Logo, 2),
            (AssetType::Header, 3),
        ] {
            let expr = apply_expression(SHORTCUT, asset, "QUJD").unwrap();
            assert!(
                expr.ends_with(&format!("\"png\", {ordinal})")),
                "{asset} should use ordinal {ordinal}: {expr}"
            );
        }
    }

    #[test]
    fn a_payload_that_could_escape_the_js_literal_is_refused() {
        // The injection guarantee. Each of these would otherwise close the string and run
        // arbitrary code inside Steam's realm, alongside Valve's own and CSS Loader's.
        for evil in [
            "AAA\", 0); alert(1); //",
            "AAA\\\", 0)",
            "AAA\nBBB",
            "data:image/png;base64,AAAA",
            "",
        ] {
            assert!(
                matches!(
                    apply_expression(SHORTCUT, AssetType::Capsule, evil),
                    Err(Error::NotBase64)
                ),
                "should have been refused: {evil:?}"
            );
        }
    }

    #[test]
    fn real_encoded_image_bytes_are_accepted() {
        // The control for the test above: the guard must not reject legitimate payloads.
        let png = [0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0x00];
        let encoded = base64::encode(&png);
        let expr = apply_expression(SHORTCUT, AssetType::Capsule, &encoded).unwrap();
        assert!(expr.contains(&encoded));
    }

    #[tokio::test]
    async fn empty_image_data_is_refused_before_any_network_work() {
        // No connection exists in this test, so reaching the socket would hang or panic;
        // returning early is the behaviour under test.
        assert!(matches!(
            apply_expression(SHORTCUT, AssetType::Capsule, &base64::encode(b"")),
            Err(Error::NotBase64)
        ));
    }

    #[test]
    fn readiness_distinguishes_apply_from_ui_injection() {
        // Losing webpack costs the Big Picture UI but not live apply, and the two degrade
        // independently.
        let full = Readiness {
            clstamp: Some("10840511".into()),
            apply_api: true,
            webpack: true,
        };
        assert!(full.can_apply() && full.can_inject_ui());

        let no_webpack = Readiness {
            webpack: false,
            ..full.clone()
        };
        assert!(no_webpack.can_apply(), "apply does not need webpack");
        assert!(!no_webpack.can_inject_ui());

        let nothing = Readiness {
            clstamp: None,
            apply_api: false,
            webpack: false,
        };
        assert!(!nothing.can_apply() && !nothing.can_inject_ui());
    }

    #[test]
    fn readiness_parses_the_probe_shape() {
        let r: Readiness = serde_json::from_value(serde_json::json!({
            "clstamp": "10840511", "apply_api": true, "webpack": true
        }))
        .unwrap();
        assert_eq!(r.clstamp.as_deref(), Some("10840511"));
        assert!(r.can_inject_ui());

        // Steam still starting: the realm exists but SteamClient is not populated yet.
        let early: Readiness = serde_json::from_value(serde_json::json!({
            "clstamp": null, "apply_api": false, "webpack": false
        }))
        .unwrap();
        assert!(!early.can_apply());
    }
}
