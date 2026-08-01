//! Reading a game controller, natively.
//!
//! 🔴 **Not through the webview's Gamepad API**, and that is the whole reason this module exists
//! rather than twenty lines of `navigator.getGamepads()` in TypeScript. Two open, unresolved
//! WebView2 bugs rule it out, and one of them lands squarely on this product's main use case:
//!
//! - [WebView2Feedback #5507] — **gamepad input stops working in WebView2 apps whenever the Steam
//!   Overlay is attached.** Griddle launched from Big Picture as a non-Steam shortcut *always* has
//!   the overlay attached, so this is not an edge case here, it is the primary path.
//! - [WebView2Feedback #3025] — the Gamepad API only delivers events while DevTools holds focus.
//!
//! Reading natively sidesteps both. It also works *with* Steam rather than against it: the Steam
//! Overlay hooks XInput, DirectInput, RawInput and Windows.Gaming.Input and injects an emulated
//! Xbox pad into them, so whatever the user has mapped in Steam Input is what arrives here. The
//! same hooking that breaks WebView2's plumbing is what makes this path work.
//!
//! [WebView2Feedback #5507]: https://github.com/MicrosoftEdge/WebView2Feedback/issues/5507
//! [WebView2Feedback #3025]: https://github.com/MicrosoftEdge/WebView2Feedback/issues/3025
//!
//! # Shape
//!
//! The runner emits **semantic actions**, never raw axes or button ids. The frontend already
//! handles the identical vocabulary from the keyboard, so a pad is not a second implementation of
//! navigation — it is a second source for the one that exists.
//!
//! [`repeat`] holds everything that can be reasoned about without a device, and is where the
//! tests are.

pub mod repeat;

pub use repeat::{DEADZONE, Direction, Repeater, stick_direction};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// What the UI is asked to do. One vocabulary, shared with the keyboard.
///
/// 🔴 **`camelCase`, not `lowercase`.** These strings are matched literally by `NavAction` in
/// `apps/desktop/src/focus.tsx`, and `rename_all = "lowercase"` flattens `TabPrev` to `"tabprev"`
/// — which no one notices while every variant is a single word, because then the two renames are
/// identical. The first two-word action silently stopped arriving while all the others kept
/// working. The test at the bottom of this module pins every string for that reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    /// Activate the focused control. A/cross.
    Accept,
    /// Dismiss the topmost dialog, or leave the current screen. B/circle.
    Back,
    /// Open the focused control's context menu. Y/triangle.
    Menu,
    /// Previous tab within whatever the current screen calls a tab. LB.
    TabPrev,
    /// Next tab. RB.
    TabNext,
}

impl From<Direction> for Action {
    fn from(d: Direction) -> Self {
        match d {
            Direction::Up => Action::Up,
            Direction::Down => Action::Down,
            Direction::Left => Action::Left,
            Direction::Right => Action::Right,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions are allowed to panic")]
mod wire_tests {
    use super::Action;

    /// The frontend matches these strings literally, and nothing on the TypeScript side can check
    /// them — a mismatch is not a type error anywhere, it is an action that silently never fires.
    ///
    /// 🔴 This exists because that happened: under `rename_all = "lowercase"`, `TabPrev` went out
    /// as `"tabprev"` while `NavAction` expected `"tabPrev"`, and LB/RB did nothing at all. Every
    /// other action was a single word, so the rename was a no-op for them and the fault was
    /// invisible until the vocabulary grew a two-word entry.
    ///
    /// **Keep this list in step with `NavAction` in `apps/desktop/src/focus.tsx`.**
    #[test]
    fn actions_serialise_to_the_strings_the_frontend_matches_on() {
        for (action, expected) in [
            (Action::Up, "up"),
            (Action::Down, "down"),
            (Action::Left, "left"),
            (Action::Right, "right"),
            (Action::Accept, "accept"),
            (Action::Back, "back"),
            (Action::Menu, "menu"),
            (Action::TabPrev, "tabPrev"),
            (Action::TabNext, "tabNext"),
        ] {
            assert_eq!(
                serde_json::to_string(&action).unwrap(),
                format!("\"{expected}\""),
                "{action:?} must reach the frontend as {expected:?}"
            );
        }
    }
}

/// A latch the host sets when the window gains or loses focus.
///
/// 🔴 Load-bearing, not a nicety. This process reads the pad globally — nothing scopes it to our
/// window — so without the gate, navigating a game with the controller would also be driving
/// Griddle in the background. z13gui gates every event on window visibility for the same reason.
#[derive(Clone, Default)]
pub struct FocusGate(Arc<AtomicBool>);

impl FocusGate {
    pub fn new(focused: bool) -> Self {
        Self(Arc::new(AtomicBool::new(focused)))
    }
    pub fn set(&self, focused: bool) {
        self.0.store(focused, Ordering::Relaxed);
    }
    pub fn is_open(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// The controllers visible right now, by name.
///
/// 🔴 A returned value rather than a log line, because "the pad is not being read" and "the UI is
/// ignoring the pad" look identical from the outside and have nothing in common as causes. The
/// runner below can only `warn!` about a backend that will not start, and a `warn!` is invisible
/// in a `windows_subsystem = "windows"` binary with no console — so the one question worth asking
/// has to be answerable on demand.
#[cfg(windows)]
pub fn connected() -> Vec<String> {
    let Ok(gilrs) = gilrs::Gilrs::new() else {
        return Vec::new();
    };
    gilrs
        .gamepads()
        .filter(|(_, pad)| pad.is_connected())
        .map(|(_, pad)| pad.name().to_owned())
        .collect()
}

#[cfg(not(windows))]
pub fn connected() -> Vec<String> {
    Vec::new()
}

/// How each button this app uses resolves on the attached pads, without pressing anything.
///
/// Built while chasing LB and RB doing nothing when A, B and Y worked. It **exonerated** the
/// mapping — every button resolved identically, `mapped` matching `native` — which is what
/// redirected the search to the wire format, where the fault actually was. Kept because ruling a
/// layer out cheaply is worth as much as convicting one, and this answers statically what would
/// otherwise need a controller in hand.
#[cfg(windows)]
pub fn describe_buttons() -> Vec<String> {
    use gilrs::Button;
    const WANTED: &[(Button, &str)] = &[
        (Button::South, "Accept (A)"),
        (Button::East, "Back (B)"),
        (Button::North, "Menu (Y)"),
        (Button::LeftTrigger, "TabPrev (LB)"),
        (Button::RightTrigger, "TabNext (RB)"),
        (Button::LeftTrigger2, "— (LT)"),
        (Button::RightTrigger2, "— (RT)"),
    ];

    let Ok(gilrs) = gilrs::Gilrs::new() else {
        return vec!["gilrs would not start".to_owned()];
    };
    let mut out = Vec::new();
    for (_, pad) in gilrs.gamepads().filter(|(_, p)| p.is_connected()) {
        out.push(format!(
            "{} — mapping source {:?}",
            pad.name(),
            pad.mapping_source()
        ));
        for (button, label) in WANTED {
            out.push(format!(
                "    {label:<14} mapped={:?} native={:?}",
                pad.button_code(*button),
                button.to_nec(),
            ));
        }
    }
    out
}

#[cfg(not(windows))]
pub fn describe_buttons() -> Vec<String> {
    Vec::new()
}

/// Start reading controllers on a background thread.
///
/// Never fails the caller: a machine with no controller, or a gilrs backend that will not
/// initialise, logs and returns. Griddle is fully usable with a mouse and keyboard, so a pad that
/// cannot be read is a missing convenience rather than a broken app.
#[cfg(windows)]
pub fn spawn<F>(gate: FocusGate, handler: F) -> std::thread::JoinHandle<()>
where
    F: Fn(Action) + Send + 'static,
{
    std::thread::spawn(move || windows_impl::run(&gate, &handler))
}

/// No controller support off Windows. The app is Windows-only; this exists so the crate still
/// compiles on the Linux CI leg, which is what catches cfg-gated mistakes.
#[cfg(not(windows))]
pub fn spawn<F>(_gate: FocusGate, _handler: F) -> std::thread::JoinHandle<()>
where
    F: Fn(Action) + Send + 'static,
{
    std::thread::spawn(|| tracing::info!("controller input is Windows-only"))
}

#[cfg(windows)]
mod windows_impl {
    use super::{Action, Direction, FocusGate, Repeater, stick_direction};
    use gilrs::{Axis, Button, Gilrs};
    use std::time::{Duration, Instant};

    /// Buttons that fire once per press. Directions are held; these are not.
    ///
    /// 🔴 `LeftTrigger`/`RightTrigger` are the **bumpers** in gilrs' vocabulary — LB and RB. The
    /// analog triggers are `LeftTrigger2`/`RightTrigger2`. Reading the names the other way round
    /// is an easy mistake that produces tab switching on a control nobody presses deliberately.
    const EDGE_BUTTONS: &[(Button, Action)] = &[
        (Button::South, Action::Accept),
        (Button::East, Action::Back),
        (Button::North, Action::Menu),
        (Button::LeftTrigger, Action::TabPrev),
        (Button::RightTrigger, Action::TabNext),
    ];

    /// How often the pad is sampled. 8 ms is comfortably under the fastest repeat interval, so
    /// the ramp is limited by `repeat` rather than by how often we happen to look.
    const POLL: Duration = Duration::from_millis(8);

    pub(super) fn run<F: Fn(Action)>(gate: &FocusGate, handler: &F) {
        // Created on this thread on purpose: gilrs owns platform handles that are not portable
        // across threads.
        let mut gilrs = match Gilrs::new() {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!(error = %e, "no controller support; keyboard and mouse only");
                return;
            }
        };

        let mut repeater = Repeater::new();
        let started = Instant::now();

        loop {
            let now_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let open = gate.is_open();

            // 🔴 Edge buttons come from **events**, not from polling `is_pressed`.
            //
            // `is_pressed(Button::X)` consults the gamepad's SDL mapping first and only falls back
            // to the button's native code, so a mapping that disagrees for one button leaves that
            // button permanently unpressed while its neighbours work — which is exactly how LB and
            // RB did nothing while A, B and Y were fine. Taking gilrs' own `ButtonPressed` uses
            // whatever resolution gilrs itself arrived at, and is edge-triggered by construction,
            // so there is no press-tracking to keep in step either.
            //
            // Every event is logged: "what does the pad actually report for this button" is
            // otherwise unanswerable, and every wrong guess about it looks like a UI bug.
            // `examples/pad_probe` turns these on.
            while let Some(event) = gilrs.next_event() {
                tracing::debug!(event = ?event.event, "gilrs");
                let gilrs::EventType::ButtonPressed(button, _) = event.event else {
                    continue;
                };
                if !open {
                    continue;
                }
                if let Some((_, action)) = EDGE_BUTTONS.iter().find(|(b, _)| *b == button) {
                    handler(*action);
                }
            }

            let mut held: Option<Direction> = None;

            for (_, pad) in gilrs.gamepads() {
                if !pad.is_connected() {
                    continue;
                }
                // The d-pad wins over the stick when both are pushed: it is the deliberate
                // input, and a resting thumb should not fight a button press.
                let dpad = if pad.is_pressed(Button::DPadUp) {
                    Some(Direction::Up)
                } else if pad.is_pressed(Button::DPadDown) {
                    Some(Direction::Down)
                } else if pad.is_pressed(Button::DPadLeft) {
                    Some(Direction::Left)
                } else if pad.is_pressed(Button::DPadRight) {
                    Some(Direction::Right)
                } else {
                    None
                };
                let stick =
                    stick_direction(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
                held = held.or(dpad).or(stick);
            }

            if open {
                if let Some(direction) = repeater.advance(held, now_ms) {
                    handler(direction.into());
                }
            } else {
                // Keep the repeater fed while the gate is shut, so a direction held across an
                // alt-tab does not fire a stale step the instant focus returns.
                let _ = repeater.advance(None, now_ms);
            }

            std::thread::sleep(POLL);
        }
    }
}
