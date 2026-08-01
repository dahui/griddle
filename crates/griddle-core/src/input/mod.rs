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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    /// Activate the focused control. A/cross.
    Accept,
    /// Close the topmost overlay, or go back. B/circle.
    Back,
    /// Open the focused control's context menu. Y/triangle.
    Menu,
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
    use gilrs::{Axis, Button, GamepadId, Gilrs};
    use std::time::{Duration, Instant};

    /// Buttons that fire once per press. Directions are held; these are not.
    const EDGE_BUTTONS: &[(Button, Action)] = &[
        (Button::South, Action::Accept),
        (Button::East, Action::Back),
        (Button::North, Action::Menu),
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
        let mut pressed: Vec<(GamepadId, Button)> = Vec::new();
        let started = Instant::now();

        loop {
            // Drain the queue so gilrs' cached state is current. The events themselves are not
            // used: a held direction is a *state* question, and edge buttons are found below.
            while gilrs.next_event().is_some() {}

            let now_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let open = gate.is_open();

            let mut held: Option<Direction> = None;
            let mut down: Vec<(GamepadId, Button)> = Vec::new();

            for (id, pad) in gilrs.gamepads() {
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

                for (button, _) in EDGE_BUTTONS {
                    if pad.is_pressed(*button) {
                        down.push((id, *button));
                    }
                }
            }

            if open {
                if let Some(direction) = repeater.advance(held, now_ms) {
                    handler(direction.into());
                }
                // Edge-triggered: only a button that was not down last poll counts as a press,
                // so holding A does not re-activate the focused control eight times a second.
                for (id, button) in &down {
                    if pressed.contains(&(*id, *button)) {
                        continue;
                    }
                    if let Some((_, action)) = EDGE_BUTTONS.iter().find(|(b, _)| b == button) {
                        handler(*action);
                    }
                }
            } else {
                // Keep the repeater fed while the gate is shut, so a direction held across an
                // alt-tab does not fire a stale step the instant focus returns.
                let _ = repeater.advance(None, now_ms);
            }

            // Updated regardless of the gate: a button held while the window was in the
            // background must not count as a fresh press when it comes back.
            pressed = down;

            std::thread::sleep(POLL);
        }
    }
}
