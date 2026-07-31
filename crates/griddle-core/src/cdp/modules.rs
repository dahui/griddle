//! Locating components inside Steam's minified bundle, and noticing when a Steam update moves
//! them.
//!
//! **This is the main reason to build this rather than keep fighting the Decky plugin.** Steam
//! mangles its export names per build, so any injected UI is inherently fragile — and the way
//! that fragility usually presents is a plugin that silently stops working. Here it becomes a
//! specific, reportable message with a named fallback.
//!
//! # Every finder is structural, never name-based
//!
//! Asset-type enum members appear only as mangled exports (`c.VYj`, `c.JoK`, `c.KoM`, `c.n4o`),
//! so matching on identifiers is hopeless. What survives minification is **content**:
//! localisation tokens like `#GameAction_GameProperties`, string literals like
//! `library_capsule`, and API names Valve cannot rename because the CEF host binds them
//! (`SetCustomArtworkForApp`). Every anchor here is one of those.
//!
//! # Sources are read without executing anything
//!
//! `require.m[id].toString()` yields a module's source **without running its factory** — 2564
//! modules, 0 unreadable, on the build this was designed against. Decky and Millennium execute
//! every module to inspect its exports; that has side effects in a realm shared with Valve's
//! own code and CSS Loader's, and it is unnecessary for locating a module.
//!
//! Export keys are deliberately *not* resolved here. Doing so needs the factory to run, so it
//! belongs to whichever feature actually needs the export — a broken export lookup then costs
//! one feature instead of the whole map.
//!
//! # 🔴 Recorded predicates, not just conclusions
//!
//! When an update breaks something, the predicate is what you edit. That is why [`Finder`]
//! stores its anchors as data with a `note` explaining what it is looking for, rather than
//! being a comment above a hardcoded module id.

use crate::settings::{ModuleMap, ModuleRef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A structural predicate over a module's source text.
#[derive(Debug, Clone, Serialize)]
pub struct Finder {
    pub name: &'static str,
    /// Every one of these must appear in the source.
    pub all_of: &'static [&'static str],
    /// At least one must appear. Empty means no constraint.
    pub any_of: &'static [&'static str],
    /// None of these may appear. Used to separate near-identical modules.
    pub none_of: &'static [&'static str],
    /// True when exactly one module should match. A second match means the predicate has gone
    /// loose and is about to start picking the wrong module at random.
    pub unique: bool,
    /// What this is looking for, in words. The thing you read when it breaks.
    pub note: &'static str,
}

/// The catalogue.
///
/// Anchors were chosen from the spike's source-text census: `SetCustomArtworkForApp` appeared
/// in 3 modules, `Focusable` in 16, `ModalRoot` in 3, `SliderField` in 1, `GamepadUI` in 198.
/// A one-word anchor with 198 hits is useless, which is why most of these are conjunctions.
pub const FINDERS: &[Finder] = &[
    Finder {
        name: "ArtworkApi",
        all_of: &["SetCustomArtworkForApp"],
        any_of: &[],
        none_of: &[],
        unique: false,
        note: "Any module referencing the apply API. Several legitimately do; this is a \
               liveness check on the anchor itself, not a component lookup.",
    },
    Finder {
        name: "SteamArtworkFlow",
        all_of: &["SetCustomArtworkForApp", "readAsDataURL", "CloseModal"],
        any_of: &[],
        none_of: &[],
        unique: true,
        note: "Steam's *own* set-custom-artwork implementation: reads a blob, strips the \
               data-URL prefix, and calls SetCustomArtworkForApp(e,r,\"png\",t) with the mime \
               hardcoded. 🔴 `CloseModal` is the discriminator — two other modules (80818, \
               81659 on build 10856968) do the same base64 strip but pass a *variable* mime, \
               so the hardcoded \"png\" is this call site's choice, not a universal rule.",
    },
    Finder {
        name: "AssetTypeNames",
        all_of: &[
            "library_capsule",
            "library_hero",
            "library_logo_transparent",
            // 🔴 Discriminator: without it this also matches webpack's asset manifest, which
            // lists paths like "./google_chrome/library_capsule.png". Anchoring on "the module
            // where these names are used *by the artwork setter*" is what we actually mean.
            "SetCustomArtworkForApp",
        ],
        any_of: &[],
        none_of: &[],
        unique: true,
        note: "The ELibraryAssetType switch: `switch(t){case vt.b_A:n=\"library_capsule\";…}`. \
               The enum members are mangled but the asset-name strings survive minification.",
    },
    Finder {
        name: "LogoPosition",
        all_of: &["SetCustomLogoPositionForApp"],
        any_of: &[],
        none_of: &[],
        unique: false,
        note: "Logo position payload shape: JSON.stringify({nVersion:1, logoPosition}).",
    },
    Finder {
        name: "FocusableFactory",
        all_of: &[
            "preferredFocus",
            "noFocusRing",
            "fnCanTakeFocus",
            // 🔴 Discriminator. The prop names alone also match the focus *tree node* class
            // (4690) and a dropdown that merely uses preferredFocus (60291). `gamepadEvents`
            // is part of the props-splitting hook's return shape and appears in neither.
            "gamepadEvents",
        ],
        any_of: &[],
        none_of: &[],
        unique: true,
        note: "The module holding the Focusable factory and its focus contexts. 🔴 The factory \
               is an export of this module (HR('div')); the hook with identical destructured \
               props returns {elemProps, navOptions, gamepadEvents} and is NOT the component.",
    },
    Finder {
        name: "FocusTreeNode",
        all_of: &["m_FocusableIfEmptyAncestor", "m_rgChildren"],
        any_of: &[],
        none_of: &[],
        unique: true,
        note: "Steam's gamepad focus-navigation tree node class. Field names are internal and \
               unmangled, which makes them a strong anchor.",
    },
    Finder {
        name: "ModalManager",
        all_of: &["ShowModalInternal", "ShowPortalModal"],
        any_of: &[],
        none_of: &[],
        unique: true,
        note: "The class whose ShowModal mounts inside Steam's own React tree — the only route \
               found that joins the gamepad focus tree. 🔴 ShowModal takes ONE argument; the \
               three-arg form is decky-frontend-lib's wrapper, not Steam's method.",
    },
    Finder {
        name: "ModalHost",
        all_of: &[
            "bRegisterModalManager",
            "DialogWrapper",
            // 🔴 Discriminator: the host is the thing that actually calls
            // `<dialog>.showModal()`. Without this it also matches a consumer (91435) that
            // merely passes these props along.
            "showModal",
        ],
        any_of: &[],
        none_of: &[],
        unique: true,
        note: "The component that renders a ModalManager and drives a native <dialog>. Not the \
               manager itself. Every literal `showModal` in the bundle is the DOM API, which is \
               exactly why it identifies the host rather than the manager.",
    },
    Finder {
        name: "AppContextMenu",
        all_of: &["#GameAction_GameProperties"],
        any_of: &[],
        none_of: &[],
        unique: true,
        note: "Where a 'Change Artwork…' entry is spliced in, before Properties. 🔴 Anchor on \
               the localisation token: earlier guesses #AppProperties_Title and \
               #AppDetails_Properties scored zero.",
    },
    Finder {
        name: "ShowContextMenu",
        all_of: &["showContextMenu"],
        any_of: &[],
        none_of: &[],
        unique: true,
        note: "The context-menu opener. 🔴 Distinct from the native HTMLDialogElement.showModal \
               DOM API, which is what every literal 'showModal' in the bundle actually is.",
    },
    Finder {
        name: "SliderField",
        all_of: &["SliderField"],
        any_of: &[],
        none_of: &[],
        unique: true,
        note: "The zoom slider control. Nice to have; losing it costs the slider, not the app.",
    },
];

/// What happened to one finder on one build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Matched, with the module ids it matched.
    Found {
        ids: Vec<String>,
    },
    /// Expected exactly one module and got several — the predicate needs tightening before it
    /// silently starts picking the wrong one.
    Ambiguous {
        ids: Vec<String>,
    },
    NotFound,
}

impl Outcome {
    pub fn is_usable(&self) -> bool {
        matches!(self, Outcome::Found { .. })
    }

    pub fn ids(&self) -> &[String] {
        match self {
            Outcome::Found { ids } | Outcome::Ambiguous { ids } => ids,
            Outcome::NotFound => &[],
        }
    }
}

/// The result of running every finder against one Steam build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub clstamp: String,
    pub total_modules: usize,
    /// Modules whose source could not be read. Expected to be 0; a non-zero value means
    /// `toString()` is being refused and the whole approach needs revisiting.
    pub unreadable: usize,
    pub outcomes: BTreeMap<String, Outcome>,
}

impl Resolution {
    pub fn usable(&self) -> usize {
        self.outcomes.values().filter(|o| o.is_usable()).count()
    }

    pub fn failed(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self
            .outcomes
            .iter()
            .filter(|(_, o)| !o.is_usable())
            .map(|(n, _)| n.as_str())
            .collect();
        v.sort_unstable();
        v
    }

    /// Convert to the form cached in settings.
    ///
    /// Only unambiguous results are stored: caching an ambiguous one would freeze a coin-flip
    /// into the settings file, and it would then look resolved on every later run.
    pub fn to_module_map(&self) -> ModuleMap {
        let mut entries = BTreeMap::new();
        let mut failed = Vec::new();
        for (name, outcome) in &self.outcomes {
            match outcome {
                Outcome::Found { ids } if !ids.is_empty() => {
                    let _ = entries.insert(
                        name.clone(),
                        ModuleRef {
                            module_id: ids[0].clone(),
                            export_key: None,
                        },
                    );
                }
                _ => failed.push(name.clone()),
            }
        }
        failed.sort();
        ModuleMap {
            clstamp: self.clstamp.clone(),
            entries,
            failed,
        }
    }

    /// Compare against what a previous build resolved to.
    pub fn diff(&self, previous: &Resolution) -> Diff {
        let mut diff = Diff {
            from: previous.clstamp.clone(),
            to: self.clstamp.clone(),
            ..Default::default()
        };

        for (name, now) in &self.outcomes {
            let before = previous.outcomes.get(name);
            match (before, now) {
                (Some(b), n) if b.is_usable() && !n.is_usable() => {
                    diff.newly_failed.push(name.clone());
                }
                (Some(b), n) if !b.is_usable() && n.is_usable() => {
                    diff.newly_found.push(name.clone());
                }
                (Some(b), n) if b.is_usable() && n.is_usable() => {
                    if b.ids() != n.ids() {
                        diff.moved.push(Moved {
                            name: name.clone(),
                            from: b.ids().to_vec(),
                            to: n.ids().to_vec(),
                        });
                    } else {
                        diff.unchanged.push(name.clone());
                    }
                }
                (None, n) if n.is_usable() => diff.newly_found.push(name.clone()),
                _ => diff.still_failing.push(name.clone()),
            }
        }

        // A finder that existed before and has since been deleted from the catalogue.
        for name in previous.outcomes.keys() {
            if !self.outcomes.contains_key(name) {
                diff.retired.push(name.clone());
            }
        }

        for v in [
            &mut diff.newly_failed,
            &mut diff.newly_found,
            &mut diff.unchanged,
            &mut diff.still_failing,
            &mut diff.retired,
        ] {
            v.sort();
        }
        diff
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub name: String,
    pub from: Vec<String>,
    pub to: Vec<String>,
}

/// What changed between two builds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    pub from: String,
    pub to: String,
    /// Found before and after, at the same module ids.
    pub unchanged: Vec<String>,
    /// Found before and after, but somewhere else. Entirely normal across a rebuild.
    pub moved: Vec<Moved>,
    /// 🔴 The ones that matter: worked before, do not now.
    pub newly_failed: Vec<String>,
    pub newly_found: Vec<String>,
    pub still_failing: Vec<String>,
    pub retired: Vec<String>,
}

impl Diff {
    pub fn is_regression(&self) -> bool {
        !self.newly_failed.is_empty()
    }

    /// The message this whole design exists to be able to produce.
    pub fn summary(&self) -> String {
        let total = self.unchanged.len() + self.moved.len() + self.newly_failed.len();
        let found = self.unchanged.len() + self.moved.len();
        let mut s = format!(
            "Steam updated from build {} to {}. {found} of {total} components re-found",
            self.from, self.to
        );
        if !self.moved.is_empty() {
            s.push_str(&format!(" ({} moved)", self.moved.len()));
        }
        if self.newly_failed.is_empty() {
            s.push('.');
        } else {
            s.push_str(&format!("; {} not found: ", self.newly_failed.len()));
            s.push_str(&self.newly_failed.join(", "));
            s.push('.');
        }
        s
    }
}

/// A user-visible capability and what it needs from the map.
///
/// Each feature declares its own dependencies so that losing one component disables exactly
/// the features that need it. Losing `SliderField` costs the zoom slider, not the app.
#[derive(Debug, Clone, Copy)]
pub struct Feature {
    pub name: &'static str,
    pub requires: &'static [&'static str],
    /// What the user should do instead. Shown verbatim when the feature is unavailable.
    pub fallback: &'static str,
}

pub const FEATURES: &[Feature] = &[
    Feature {
        name: "Big Picture UI",
        requires: &["FocusableFactory", "ModalManager"],
        fallback: "Use the desktop window to change artwork.",
    },
    Feature {
        name: "Context-menu entry",
        requires: &["AppContextMenu", "ShowContextMenu"],
        fallback: "Open artwork from the desktop window instead.",
    },
    Feature {
        name: "Zoom slider",
        requires: &["SliderField"],
        fallback: "Zoom is fixed at its default.",
    },
];

impl Feature {
    pub fn available(&self, resolution: &Resolution) -> bool {
        self.requires
            .iter()
            .all(|r| resolution.outcomes.get(*r).is_some_and(Outcome::is_usable))
    }

    pub fn missing<'a>(&self, resolution: &'a Resolution) -> Vec<&'a str> {
        self.requires
            .iter()
            .filter(|r| !resolution.outcomes.get(**r).is_some_and(Outcome::is_usable))
            .filter_map(|r| resolution.outcomes.keys().find(|k| k.as_str() == *r))
            .map(String::as_str)
            .collect()
    }
}

/// 🔑 **Live apply needs no finders at all.**
///
/// It calls `SteamClient.Apps.SetCustomArtworkForApp` directly, which the CEF host binds and
/// Valve cannot rename without breaking their own client. So the riskiest, most valuable
/// feature in the product is also the one least exposed to a Steam update — worth keeping true
/// as the code grows.
pub const LIVE_APPLY_NEEDS_NO_MODULES: () = ();

/// The JavaScript that captures the module registry and runs every finder in one round trip.
///
/// Reads `require.m[id].toString()` rather than executing factories — see the module docs.
///
/// # 🔴 The chunk id must be unique per push
///
/// webpack keys installed chunks by their id. Pushing `[[{}], …]` looks unique in the source
/// but stringifies to `"[object Object]"` every time, so the **second** scan in a Steam session
/// finds the chunk already installed and the callback is never called. That failed as
/// `the module registry was not handed over` — and had this returned empty hits instead of an
/// error, it would have looked exactly like Steam removing every component at once.
///
/// A fresh random id per call keeps every scan working. This is why the spike's snippet used
/// `Math.random()`; the reason was not recorded at the time, so it got dropped.
pub fn resolve_script(finders: &[Finder]) -> String {
    // Serialised as JSON, which is valid JS, so anchors containing `#`, quotes or backslashes
    // are escaped correctly rather than hand-quoted.
    let table = serde_json::to_string(finders).unwrap_or_else(|_| "[]".to_owned());
    format!(
        r#"(() => {{
  let req = null;
  try {{
    const marker = '__sgdb_scan_' + Math.random().toString(36).slice(2);
    window.webpackChunksteamui.push([[marker], {{}}, (r) => {{ req = r; }}]);
  }} catch (e) {{
    return {{ error: 'could not capture the module registry: ' + e.message }};
  }}
  if (!req || !req.m) return {{ error: 'the module registry was not handed over' }};

  const FINDERS = {table};
  const ids = Object.keys(req.m);
  const sources = [];
  let unreadable = 0;
  for (const id of ids) {{
    try {{ sources.push([id, req.m[id].toString()]); }} catch (e) {{ unreadable++; }}
  }}

  const hits = {{}};
  for (const f of FINDERS) {{
    const matched = [];
    for (const [id, src] of sources) {{
      if (f.all_of.some((s) => !src.includes(s))) continue;
      if (f.any_of.length && !f.any_of.some((s) => src.includes(s))) continue;
      if (f.none_of.some((s) => src.includes(s))) continue;
      matched.push(id);
    }}
    hits[f.name] = matched;
  }}
  return {{ total: ids.length, unreadable, hits }};
}})()"#
    )
}

/// Raw shape returned by [`resolve_script`].
#[derive(Debug, Clone, Deserialize)]
pub struct RawScan {
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub unreadable: usize,
    #[serde(default)]
    pub hits: BTreeMap<String, Vec<String>>,
}

/// Turn a raw scan into outcomes, applying each finder's uniqueness expectation.
pub fn interpret(clstamp: &str, scan: &RawScan, finders: &[Finder]) -> Resolution {
    let mut outcomes = BTreeMap::new();
    for f in finders {
        let ids = scan.hits.get(f.name).cloned().unwrap_or_default();
        let outcome = if ids.is_empty() {
            Outcome::NotFound
        } else if f.unique && ids.len() > 1 {
            Outcome::Ambiguous { ids }
        } else {
            Outcome::Found { ids }
        };
        let _ = outcomes.insert(f.name.to_owned(), outcome);
    }
    Resolution {
        clstamp: clstamp.to_owned(),
        total_modules: scan.total,
        unreadable: scan.unreadable,
        outcomes,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod tests {
    use super::*;

    fn scan_of(pairs: &[(&str, &[&str])]) -> RawScan {
        RawScan {
            error: None,
            total: 2564,
            unreadable: 0,
            hits: pairs
                .iter()
                .map(|(k, v)| {
                    (
                        (*k).to_owned(),
                        v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn every_finder_has_a_note_and_at_least_one_anchor() {
        // The note is what someone reads when a Steam update breaks the finder, so a finder
        // without one is a hardcoded module id with extra steps.
        for f in FINDERS {
            assert!(!f.all_of.is_empty(), "{} has no anchors", f.name);
            assert!(f.note.len() > 20, "{} needs a real note", f.name);
            for anchor in f.all_of {
                assert!(!anchor.is_empty(), "{} has an empty anchor", f.name);
            }
        }
    }

    #[test]
    fn finder_names_are_unique() {
        // They key a BTreeMap, so a duplicate would silently drop a finder.
        let mut names: Vec<&str> = FINDERS.iter().map(|f| f.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate finder name");
    }

    #[test]
    fn every_feature_depends_only_on_finders_that_exist() {
        // A typo here would make a feature permanently unavailable with no explanation.
        for feature in FEATURES {
            for req in feature.requires {
                assert!(
                    FINDERS.iter().any(|f| f.name == *req),
                    "feature {:?} requires unknown finder {req:?}",
                    feature.name
                );
            }
            assert!(
                !feature.fallback.is_empty(),
                "{} needs a fallback",
                feature.name
            );
        }
    }

    #[test]
    fn a_unique_finder_matching_twice_is_ambiguous_not_resolved() {
        // Silently taking the first of two matches is how a build starts loading the wrong
        // module without anyone noticing.
        let scan = scan_of(&[("ModalManager", &["3673", "9999"])]);
        let r = interpret("10856968", &scan, FINDERS);
        assert_eq!(
            r.outcomes.get("ModalManager").unwrap(),
            &Outcome::Ambiguous {
                ids: vec!["3673".into(), "9999".into()]
            }
        );
        assert!(!r.outcomes.get("ModalManager").unwrap().is_usable());
    }

    #[test]
    fn a_non_unique_finder_matching_several_times_is_fine() {
        let scan = scan_of(&[("ArtworkApi", &["87498", "5808", "1234"])]);
        let r = interpret("10856968", &scan, FINDERS);
        assert!(r.outcomes.get("ArtworkApi").unwrap().is_usable());
    }

    #[test]
    fn a_missing_finder_is_not_found() {
        let r = interpret("10856968", &scan_of(&[]), FINDERS);
        assert_eq!(r.outcomes.get("SliderField").unwrap(), &Outcome::NotFound);
        assert_eq!(r.usable(), 0);
        assert_eq!(r.failed().len(), FINDERS.len());
    }

    #[test]
    fn the_diff_produces_the_message_this_design_exists_for() {
        let before = interpret(
            "10840511",
            &scan_of(&[
                ("FocusableFactory", &["28869"]),
                ("ModalManager", &["3673"]),
                ("AppContextMenu", &["5808"]),
                ("SliderField", &["4242"]),
            ]),
            FINDERS,
        );
        // After an update: two moved, two gone.
        let after = interpret(
            "10856968",
            &scan_of(&[
                ("FocusableFactory", &["30011"]),
                ("ModalManager", &["3673"]),
            ]),
            FINDERS,
        );

        let diff = after.diff(&before);
        assert!(diff.is_regression());
        assert_eq!(diff.newly_failed, ["AppContextMenu", "SliderField"]);
        assert_eq!(diff.unchanged, ["ModalManager"]);
        assert_eq!(diff.moved.len(), 1);
        assert_eq!(diff.moved[0].name, "FocusableFactory");
        assert_eq!(diff.moved[0].from, ["28869"]);
        assert_eq!(diff.moved[0].to, ["30011"]);

        let summary = diff.summary();
        assert!(summary.contains("10840511"), "{summary}");
        assert!(summary.contains("10856968"), "{summary}");
        assert!(summary.contains("AppContextMenu"), "{summary}");
        assert!(summary.contains("moved"), "{summary}");
    }

    #[test]
    fn an_unchanged_build_reports_no_regression() {
        let hits = scan_of(&[("ModalManager", &["3673"]), ("SliderField", &["4242"])]);
        let a = interpret("10840511", &hits, FINDERS);
        let b = interpret("10840511", &hits, FINDERS);
        let diff = b.diff(&a);
        assert!(!diff.is_regression());
        assert!(diff.moved.is_empty());
        assert!(diff.summary().ends_with('.'), "{}", diff.summary());
    }

    #[test]
    fn features_degrade_independently() {
        // The point of per-feature dependencies: losing the slider must not cost Big Picture.
        let r = interpret(
            "10856968",
            &scan_of(&[
                ("FocusableFactory", &["28869"]),
                ("ModalManager", &["3673"]),
                ("AppContextMenu", &["5808"]),
                ("ShowContextMenu", &["39590"]),
            ]),
            FINDERS,
        );

        let by_name = |n: &str| FEATURES.iter().find(|f| f.name == n).unwrap();
        assert!(by_name("Big Picture UI").available(&r));
        assert!(by_name("Context-menu entry").available(&r));
        assert!(
            !by_name("Zoom slider").available(&r),
            "SliderField was not found"
        );
        assert_eq!(by_name("Zoom slider").missing(&r), ["SliderField"]);
    }

    #[test]
    fn the_module_map_stores_only_unambiguous_results() {
        // Caching an ambiguous pick would freeze a coin-flip into the settings file, where it
        // would then look resolved forever.
        let r = interpret(
            "10856968",
            &scan_of(&[
                ("ModalManager", &["3673", "9999"]),
                ("SliderField", &["4242"]),
            ]),
            FINDERS,
        );
        let map = r.to_module_map();
        assert_eq!(map.clstamp, "10856968");
        assert!(map.entries.contains_key("SliderField"));
        assert!(!map.entries.contains_key("ModalManager"));
        assert!(map.failed.contains(&"ModalManager".to_string()));
    }

    #[test]
    fn the_script_embeds_anchors_as_escaped_json() {
        let script = resolve_script(FINDERS);
        // The token anchor contains a `#`, which must survive into the JS intact.
        assert!(script.contains("#GameAction_GameProperties"), "{script}");
        assert!(script.contains("webpackChunksteamui"));
        assert!(script.contains("toString()"));
        // Factories must never be executed — that is the whole difference from Decky's approach.
        assert!(
            !script.contains("req(id)"),
            "the scan must not execute module factories"
        );
    }

    #[test]
    fn the_chunk_marker_is_unique_per_call() {
        // 🔴 Regression guard. A literal `{}` chunk id stringifies to "[object Object]", so
        // webpack treats the second push of a session as an already-installed chunk and never
        // calls the callback — every scan after the first silently found nothing.
        let script = resolve_script(FINDERS);
        assert!(
            script.contains("Math.random()"),
            "the chunk id must vary per call, or only the first scan in a Steam session works"
        );
        assert!(
            !script.contains("push([[{}]"),
            "a literal object as the chunk id is the bug this guards against"
        );
    }

    #[test]
    fn a_script_error_is_surfaced_rather_than_read_as_zero_matches() {
        // Without this, a failure to capture the registry would look identical to "Steam
        // removed every component we need".
        let raw: RawScan = serde_json::from_value(serde_json::json!({
            "error": "could not capture the module registry: x is not a function"
        }))
        .unwrap();
        assert!(raw.error.is_some());
        assert_eq!(raw.total, 0);
    }

    #[test]
    fn a_real_scan_shape_deserialises() {
        let raw: RawScan = serde_json::from_value(serde_json::json!({
            "total": 2564, "unreadable": 0,
            "hits": { "ArtworkApi": ["87498", "5808", "3673"], "SliderField": [] }
        }))
        .unwrap();
        assert_eq!(raw.total, 2564);
        assert_eq!(raw.hits.get("ArtworkApi").unwrap().len(), 3);
        assert!(raw.hits.get("SliderField").unwrap().is_empty());
    }
}
