//! Custom logo positioning.
//!
//! Steam stores this in `userdata/<id>/config/grid/<appid>.json`:
//!
//! ```json
//! {"nVersion":1,"logoPosition":{"pinnedPosition":"BottomLeft","nWidthPct":50,"nHeightPct":50}}
//! ```
//!
//! Confirmed against Valve's own shipped code, which calls
//! `SetCustomLogoPositionForApp(e.appid, JSON.stringify({nVersion:1, logoPosition:t}))`.
//! `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]`
//!
//! # Why this exists in two languages
//!
//! The desktop GUI and the Big Picture injection render in different runtimes, so the
//! positioner geometry is implemented in both Rust and TypeScript
//! (`packages/shared/src/logo.ts`). **Both are tested against the same fixture file**,
//! `packages/shared/fixtures/logo-positions.json`. That file is the thing that stops them
//! drifting; add a case there before changing either implementation.
//!
//! # Two things that will bite
//!
//! **There are only five anchors.** No `BottomRight`, no `UpperRight`, no `CenterLeft`.
//!
//! **A custom logo with no stored position may not render at all**, so writing
//! `<appid>_logo.png` must also write an `<appid>.json` when none exists. See
//! [`DEFAULT_POSITION`]. `[VERIFIED-SOURCE — decky-steamgriddb force-creates this for shortcuts]`

use serde::{Deserialize, Serialize};

/// Where the logo is anchored within the hero area.
///
/// Serialized with Steam's exact PascalCase spellings — these strings go into a file Steam
/// parses, so the `serde` renames are load-bearing, not stylistic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinnedPosition {
    BottomLeft,
    UpperLeft,
    UpperCenter,
    CenterCenter,
    BottomCenter,
}

impl PinnedPosition {
    /// Every anchor, in the order the positioner's "next anchor" action cycles through them.
    pub const ALL: [PinnedPosition; 5] = [
        PinnedPosition::BottomLeft,
        PinnedPosition::UpperLeft,
        PinnedPosition::UpperCenter,
        PinnedPosition::CenterCenter,
        PinnedPosition::BottomCenter,
    ];

    /// Next anchor in cycle order, wrapping.
    pub fn next(self) -> PinnedPosition {
        let i = Self::ALL.iter().position(|&p| p == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LogoPosition {
    #[serde(rename = "pinnedPosition")]
    pub pinned_position: PinnedPosition,
    /// Percentage of the hero area's width, 0-100.
    #[serde(rename = "nWidthPct")]
    pub width_pct: f64,
    /// Percentage of the hero area's height, 0-100.
    #[serde(rename = "nHeightPct")]
    pub height_pct: f64,
}

/// The on-disk wrapper. This is what `<appid>.json` contains.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LogoPositionForApp {
    #[serde(rename = "nVersion")]
    pub version: u32,
    #[serde(rename = "logoPosition")]
    pub logo_position: LogoPosition,
}

/// What a logo apply writes when the app has no stored position.
pub const DEFAULT_POSITION: LogoPosition = LogoPosition {
    pinned_position: PinnedPosition::BottomLeft,
    width_pct: 50.0,
    height_pct: 50.0,
};

impl LogoPosition {
    pub fn for_app(self) -> LogoPositionForApp {
        LogoPositionForApp {
            version: 1,
            logo_position: self,
        }
    }

    /// CSS `top`/`left` percentages for this position.
    ///
    /// Centered anchors offset by half the *remaining* space, which is why they visually move
    /// at twice the rate of a corner anchor when resized.
    pub fn to_css(self) -> Css {
        let (w, h) = (self.width_pct, self.height_pct);
        let centered_left = (100.0 - w) / 2.0;
        match self.pinned_position {
            PinnedPosition::UpperLeft => Css {
                top: 0.0,
                left: 0.0,
            },
            PinnedPosition::BottomLeft => Css {
                top: 100.0 - h,
                left: 0.0,
            },
            PinnedPosition::UpperCenter => Css {
                top: 0.0,
                left: centered_left,
            },
            PinnedPosition::CenterCenter => Css {
                top: (100.0 - h) / 2.0,
                left: centered_left,
            },
            PinnedPosition::BottomCenter => Css {
                top: 100.0 - h,
                left: centered_left,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Css {
    pub top: f64,
    pub left: f64,
}

impl Default for LogoPosition {
    fn default() -> Self {
        DEFAULT_POSITION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The golden table shared with `packages/shared/src/logo.test.ts`.
    ///
    /// Read from the JSON at test time rather than duplicated here — a copy would defeat the
    /// entire purpose of having one fixture.
    #[derive(serde::Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    #[derive(serde::Deserialize)]
    struct Case {
        pin: String,
        w: f64,
        h: f64,
        top: f64,
        left: f64,
    }

    fn load_fixture() -> Fixture {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/shared/fixtures/logo-positions.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("shared logo fixture missing at {path}: {e}"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("shared logo fixture malformed: {e}"))
    }

    fn parse_pin(s: &str) -> PinnedPosition {
        match s {
            "BottomLeft" => PinnedPosition::BottomLeft,
            "UpperLeft" => PinnedPosition::UpperLeft,
            "UpperCenter" => PinnedPosition::UpperCenter,
            "CenterCenter" => PinnedPosition::CenterCenter,
            "BottomCenter" => PinnedPosition::BottomCenter,
            other => panic!("fixture names an anchor Steam does not have: {other}"),
        }
    }

    #[test]
    fn matches_the_shared_golden_table() {
        let fixture = load_fixture();
        assert!(!fixture.cases.is_empty(), "fixture has no cases");

        for c in &fixture.cases {
            let css = LogoPosition {
                pinned_position: parse_pin(&c.pin),
                width_pct: c.w,
                height_pct: c.h,
            }
            .to_css();

            assert!(
                (css.top - c.top).abs() < 1e-9 && (css.left - c.left).abs() < 1e-9,
                "{} w={} h={}: expected top={} left={}, got top={} left={}",
                c.pin,
                c.w,
                c.h,
                c.top,
                c.left,
                css.top,
                css.left,
            );
        }
    }

    #[test]
    fn fixture_covers_every_anchor() {
        let fixture = load_fixture();
        for pin in PinnedPosition::ALL {
            let name = format!("{pin:?}");
            assert!(
                fixture.cases.iter().any(|c| c.pin == name),
                "shared fixture has no case for {name}",
            );
        }
    }

    #[test]
    fn serializes_exactly_as_steam_expects() {
        let json = serde_json::to_string(&DEFAULT_POSITION.for_app()).unwrap();
        assert_eq!(
            json,
            r#"{"nVersion":1,"logoPosition":{"pinnedPosition":"BottomLeft","nWidthPct":50.0,"nHeightPct":50.0}}"#
        );
    }

    #[test]
    fn round_trips_through_json() {
        let original = LogoPosition {
            pinned_position: PinnedPosition::CenterCenter,
            width_pct: 42.5,
            height_pct: 17.25,
        };
        let json = serde_json::to_string(&original.for_app()).unwrap();
        let back: LogoPositionForApp = serde_json::from_str(&json).unwrap();
        assert_eq!(back.logo_position, original);
        assert_eq!(back.version, 1);
    }

    #[test]
    fn rejects_an_anchor_steam_does_not_have() {
        // A hand-edited <appid>.json should fail to parse rather than silently become
        // something else.
        let json = r#"{"nVersion":1,"logoPosition":{"pinnedPosition":"BottomRight","nWidthPct":50,"nHeightPct":50}}"#;
        assert!(serde_json::from_str::<LogoPositionForApp>(json).is_err());
    }

    #[test]
    fn cycle_order_wraps_and_visits_all_five() {
        let mut pin = PinnedPosition::BottomLeft;
        let mut seen = Vec::new();
        for _ in 0..5 {
            seen.push(pin);
            pin = pin.next();
        }
        assert_eq!(seen, PinnedPosition::ALL);
        assert_eq!(pin, PinnedPosition::BottomLeft);
    }

    #[test]
    fn css_output_stays_on_canvas() {
        for pin in PinnedPosition::ALL {
            for w in 1..=100 {
                for h in 1..=100 {
                    let (w, h) = (f64::from(w), f64::from(h));
                    let css = LogoPosition {
                        pinned_position: pin,
                        width_pct: w,
                        height_pct: h,
                    }
                    .to_css();
                    assert!(
                        css.top >= 0.0 && css.left >= 0.0,
                        "{pin:?} {w}x{h} went negative"
                    );
                    assert!(
                        css.top + h <= 100.0 + 1e-9 && css.left + w <= 100.0 + 1e-9,
                        "{pin:?} {w}x{h} overflowed the container",
                    );
                }
            }
        }
    }
}
