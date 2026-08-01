//! Directional auto-repeat: which held direction owns the cursor, and how fast it moves.
//!
//! Deliberately free of any clock, device or timer — `advance` is handed the currently-held
//! direction and the current time in milliseconds and returns what to emit. Everything awkward
//! about holding a stick is therefore a unit test rather than something to discover with a
//! controller in hand.
//!
//! Modelled on z13gui's `keyrepeat.Tracker`. The idea it exists for:
//!
//! **One direction owns the repeat at a time.** A thumbstick is analog and almost never held
//! on a perfect axis, so a diagonal push satisfies the threshold for *two* directions at once. If
//! both repeat, the cursor walks diagonally in a stutter and feels broken. Ownership makes a hold
//! unambiguous: the dominant axis wins, and it keeps the repeat until it is released.

/// A direction the user is pushing. Mirrors `focusgrid`'s `Direction` on the TypeScript side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// How far a stick must be pushed to count as held.
///
/// Generous on purpose. This is navigation, not aiming: a low threshold turns resting-thumb drift
/// into cursor movement, and a stick that has developed a little wear then moves the selection on
/// its own.
pub const DEADZONE: f32 = 0.5;

/// Before the first repeat. Long enough that a deliberate single step never double-fires.
const INITIAL_DELAY_MS: u64 = 400;
/// The first repeat interval, then ramping down.
const REPEAT_START_MS: u64 = 150;
/// The floor. Faster than this and a long list becomes impossible to stop on.
const REPEAT_MIN_MS: u64 = 55;
/// How much each successive repeat shortens the interval.
const RAMP_STEP_MS: u64 = 12;

/// The interval before repeat number `count` (0 = the first repeat after the initial delay).
fn interval_ms(count: u32) -> u64 {
    REPEAT_START_MS
        .saturating_sub(u64::from(count) * RAMP_STEP_MS)
        .max(REPEAT_MIN_MS)
}

/// Which way a stick is being pushed, or `None` inside the deadzone.
///
/// The **dominant axis wins** rather than both being reported. See the module note: a diagonal
/// must resolve to one direction or the repeat interleaves.
///
/// `y` is in the convention the caller supplies; the gamepad runner negates the raw axis so that
/// positive `y` is up, matching how a user describes the stick rather than how the hardware
/// reports it.
pub fn stick_direction(x: f32, y: f32) -> Option<Direction> {
    if x.abs() < DEADZONE && y.abs() < DEADZONE {
        return None;
    }
    if x.abs() >= y.abs() {
        Some(if x > 0.0 {
            Direction::Right
        } else {
            Direction::Left
        })
    } else {
        Some(if y > 0.0 {
            Direction::Up
        } else {
            Direction::Down
        })
    }
}

/// Tracks the held direction and decides when it should fire.
#[derive(Debug, Default)]
pub struct Repeater {
    owner: Option<Direction>,
    /// When the owner is next due to fire.
    next_at: u64,
    /// Repeats emitted for this hold, driving the ramp.
    count: u32,
}

impl Repeater {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the currently-held direction and the current time; get back what to emit now.
    ///
    /// Call this every poll. `held` is `None` when nothing is pushed, which releases ownership —
    /// so letting go and pushing again always produces an immediate step rather than resuming
    /// mid-ramp.
    pub fn advance(&mut self, held: Option<Direction>, now_ms: u64) -> Option<Direction> {
        let Some(direction) = held else {
            self.owner = None;
            self.count = 0;
            return None;
        };

        // A different direction takes ownership immediately, and fires at once: changing
        // direction mid-hold should feel instant, not wait out the previous repeat.
        if self.owner != Some(direction) {
            self.owner = Some(direction);
            self.count = 0;
            self.next_at = now_ms.saturating_add(INITIAL_DELAY_MS);
            return Some(direction);
        }

        if now_ms < self.next_at {
            return None;
        }
        let step = interval_ms(self.count);
        self.count = self.count.saturating_add(1);
        // Measured from *now* rather than from `next_at`, so a poll that arrives late does not
        // bank up a burst of repeats it then fires all at once.
        self.next_at = now_ms.saturating_add(step);
        Some(direction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_fires_immediately_then_waits_out_the_initial_delay() {
        let mut r = Repeater::new();
        assert_eq!(r.advance(Some(Direction::Down), 0), Some(Direction::Down));
        // Nothing for the whole initial delay, or a single deliberate tap becomes two steps.
        assert_eq!(r.advance(Some(Direction::Down), 100), None);
        assert_eq!(r.advance(Some(Direction::Down), 399), None);
        assert_eq!(r.advance(Some(Direction::Down), 400), Some(Direction::Down));
    }

    #[test]
    fn releasing_resets_the_ramp() {
        let mut r = Repeater::new();
        let mut now = 0;
        assert!(r.advance(Some(Direction::Down), now).is_some());
        // Hold long enough to ramp up.
        for _ in 0..10 {
            now += 400;
            let _ = r.advance(Some(Direction::Down), now);
        }
        // Let go, then press again: the next press must fire at once and wait the *full* initial
        // delay, not resume at the accelerated rate.
        assert_eq!(r.advance(None, now), None);
        now += 10;
        assert_eq!(r.advance(Some(Direction::Down), now), Some(Direction::Down));
        assert_eq!(r.advance(Some(Direction::Down), now + 399), None);
    }

    #[test]
    fn changing_direction_takes_ownership_and_fires_at_once() {
        let mut r = Repeater::new();
        assert_eq!(r.advance(Some(Direction::Down), 0), Some(Direction::Down));
        // Mid-hold, without passing through "nothing held" — which is exactly what rolling a
        // thumbstick around its edge does.
        assert_eq!(
            r.advance(Some(Direction::Right), 50),
            Some(Direction::Right)
        );
        // …and the new owner starts its own initial delay rather than inheriting the old one.
        assert_eq!(r.advance(Some(Direction::Right), 449), None);
        assert_eq!(
            r.advance(Some(Direction::Right), 450),
            Some(Direction::Right)
        );
    }

    #[test]
    fn the_repeat_accelerates_but_never_past_the_floor() {
        assert_eq!(interval_ms(0), REPEAT_START_MS);
        assert!(interval_ms(1) < interval_ms(0));
        // The floor holds no matter how long the stick is held; without the clamp the ramp
        // subtracts past zero and saturates to an interval of nothing at all.
        assert_eq!(interval_ms(1_000), REPEAT_MIN_MS);
        assert_eq!(interval_ms(u32::MAX), REPEAT_MIN_MS);
    }

    #[test]
    fn a_late_poll_does_not_bank_up_a_burst() {
        // The window loses focus, or the thread is starved, and the next poll arrives seconds
        // late. Scheduling from `next_at` rather than from `now` would then fire once per missed
        // interval — the cursor shoots across the screen the moment the app comes back.
        let mut r = Repeater::new();
        assert!(r.advance(Some(Direction::Up), 0).is_some());
        assert!(r.advance(Some(Direction::Up), 10_000).is_some());
        assert_eq!(r.advance(Some(Direction::Up), 10_001), None);
    }

    #[test]
    fn the_deadzone_ignores_a_resting_thumb() {
        assert_eq!(stick_direction(0.0, 0.0), None);
        assert_eq!(stick_direction(0.4, -0.4), None);
        assert_eq!(stick_direction(-0.49, 0.49), None);
    }

    #[test]
    fn a_diagonal_resolves_to_one_direction() {
        // The whole reason ownership exists. Both axes are past the deadzone here, and reporting
        // both would make the cursor stutter diagonally.
        assert_eq!(stick_direction(0.9, 0.6), Some(Direction::Right));
        assert_eq!(stick_direction(0.6, 0.9), Some(Direction::Up));
        assert_eq!(stick_direction(-0.9, -0.6), Some(Direction::Left));
        assert_eq!(stick_direction(-0.6, -0.9), Some(Direction::Down));
    }

    #[test]
    fn a_perfect_diagonal_still_picks_exactly_one() {
        // Equal magnitudes must not be a coin flip that alternates between polls, or a stick held
        // at 45° would emit right, up, right, up. The tie is broken toward the horizontal.
        let first = stick_direction(0.8, 0.8);
        assert_eq!(first, Some(Direction::Right));
        assert_eq!(stick_direction(0.8, 0.8), first, "must be deterministic");
    }

    #[test]
    fn the_axes_map_the_way_a_user_would_describe_them() {
        // Positive y is up by this module's convention; the runner negates the raw axis. Getting
        // this backwards inverts the whole UI and reads as "the stick is upside down".
        assert_eq!(stick_direction(0.0, 1.0), Some(Direction::Up));
        assert_eq!(stick_direction(0.0, -1.0), Some(Direction::Down));
        assert_eq!(stick_direction(1.0, 0.0), Some(Direction::Right));
        assert_eq!(stick_direction(-1.0, 0.0), Some(Direction::Left));
    }
}
