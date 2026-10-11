//! The Pomodoro timer: a focus block of 5 to 60 minutes (25 by default) that counts down on a
//! dial. A click starts or pauses it, a double click resets it, and dragging the dial while it is
//! stopped sets the length in 5-minute steps.
//!
//! This is only the state machine: it is told how many seconds passed (`tick`), so it needs no
//! clock and is tested without waiting. The widget in the UI crate draws it and feeds it time.

/// The default focus block.
pub const DEFAULT_MINUTES: u32 = 25;
/// The shortest and longest block the dial can set.
pub const MIN_MINUTES: u32 = 5;
pub const MAX_MINUTES: u32 = 60;
const STEP_MINUTES: u32 = 5;
/// The dial shows a full turn as this many seconds.
pub const DIAL_SECONDS: u32 = MAX_MINUTES * 60;

/// What a tick produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    None,
    /// The block ran out on this tick.
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pomodoro {
    duration: u32,
    left: u32,
    running: bool,
    dragging: bool,
}

impl Default for Pomodoro {
    fn default() -> Self {
        let duration = DEFAULT_MINUTES * 60;
        Pomodoro { duration, left: duration, running: false, dragging: false }
    }
}

impl Pomodoro {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn dragging(&self) -> bool {
        self.dragging
    }

    /// Seconds left in the block.
    pub fn left(&self) -> u32 {
        self.left
    }

    /// The length of a block, in seconds.
    pub fn duration(&self) -> u32 {
        self.duration
    }

    /// Started, paused part-way, or being set: anything but a fresh block.
    pub fn active(&self) -> bool {
        self.running || self.dragging || self.left < self.duration
    }

    /// `MM:SS`.
    pub fn label(&self) -> String {
        format!("{:02}:{:02}", self.left / 60, self.left % 60)
    }

    /// How much of the dial is filled, 0 to 1 (a full dial is 60 minutes).
    pub fn fraction(&self) -> f64 {
        (f64::from(self.left) / f64::from(DIAL_SECONDS)).clamp(0.0, 1.0)
    }

    /// Start or pause. A finished block starts again from its full length.
    pub fn toggle(&mut self) {
        if self.dragging {
            return;
        }
        if self.left == 0 {
            self.left = self.duration;
            self.running = true;
        } else {
            self.running = !self.running;
        }
    }

    /// Stop and go back to a full block.
    pub fn reset(&mut self) {
        self.running = false;
        self.dragging = false;
        self.left = self.duration;
    }

    /// Time passed. Only a running timer counts down.
    pub fn tick(&mut self, seconds: u32) -> Event {
        if !self.running || self.dragging {
            return Event::None;
        }
        if self.left <= seconds {
            self.left = 0;
            self.running = false;
            return Event::Completed;
        }
        self.left -= seconds;
        Event::None
    }

    /// Set the block length in minutes: clamped to 5..=60 and rounded to a 5-minute step. A running
    /// timer is not changed.
    pub fn set_minutes(&mut self, minutes: u32) -> u32 {
        let m = snap_minutes(f64::from(minutes));
        if !self.running {
            self.dragging = false;
            self.duration = m * 60;
            self.left = self.duration;
        }
        m
    }

    /// The dial is being dragged to `turn` (0 at the top, 1 after a full clockwise turn). The dial
    /// shows the minutes while dragging; nothing is kept until [`Pomodoro::end_drag`]. Ignored
    /// while running.
    pub fn drag_to(&mut self, turn: f64) {
        if self.running || !turn.is_finite() {
            return;
        }
        self.dragging = true;
        self.left = (turn.rem_euclid(1.0) * f64::from(DIAL_SECONDS)).round() as u32;
    }

    /// The drag ended: keep the chosen length, snapped to 5 minutes. Returns the minutes, or `None`
    /// if nothing was being dragged.
    pub fn end_drag(&mut self) -> Option<u32> {
        if !self.dragging {
            return None;
        }
        let m = snap_minutes(f64::from(self.left) / 60.0);
        self.dragging = false;
        self.duration = m * 60;
        self.left = self.duration;
        Some(m)
    }
}

fn snap_minutes(minutes: f64) -> u32 {
    let steps = (minutes / f64::from(STEP_MINUTES)).round();
    let m = if steps.is_finite() { steps * f64::from(STEP_MINUTES) } else { f64::from(DEFAULT_MINUTES) };
    m.clamp(f64::from(MIN_MINUTES), f64::from(MAX_MINUTES)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_timer_is_25_minutes_and_stopped() {
        let p = Pomodoro::new();
        assert_eq!((p.label().as_str(), p.running(), p.active()), ("25:00", false, false));
        assert!((p.fraction() - 25.0 / 60.0).abs() < 1e-12);
    }

    #[test]
    fn it_counts_down_pauses_and_completes() {
        let mut p = Pomodoro::new();
        assert_eq!(p.tick(10), Event::None, "a stopped timer does not move");
        p.toggle();
        assert_eq!(p.tick(61), Event::None);
        assert_eq!(p.label(), "23:59");
        p.toggle();
        p.tick(100);
        assert_eq!(p.label(), "23:59", "paused");
        assert!(p.active());
        p.toggle();
        assert_eq!(p.tick(10_000), Event::Completed);
        assert_eq!((p.left(), p.running()), (0, false));
        assert_eq!(p.tick(1), Event::None, "finished timers stay finished");
        p.toggle();
        assert_eq!((p.label().as_str(), p.running()), ("25:00", true), "starts again from the full block");
        p.reset();
        assert_eq!((p.label().as_str(), p.running(), p.active()), ("25:00", false, false));
    }

    #[test]
    fn dragging_the_dial_sets_the_length_in_five_minute_steps() {
        let mut p = Pomodoro::new();
        assert_eq!(p.end_drag(), None);
        p.drag_to(0.5);
        assert_eq!(p.label(), "30:00");
        assert!(p.dragging() && p.active());
        p.toggle();
        assert!(!p.running(), "a click while dragging does nothing");
        p.drag_to(0.4);
        assert_eq!(p.end_drag(), Some(25), "24 minutes snaps to 25");
        assert_eq!(p.duration(), 25 * 60);
        p.drag_to(0.01);
        assert_eq!(p.end_drag(), Some(5), "never below 5 minutes");
        p.drag_to(1.0);
        assert_eq!(p.left(), 0, "a full turn wraps to the top");
        assert_eq!(p.end_drag(), Some(5));
        p.drag_to(0.999);
        assert_eq!(p.end_drag(), Some(60));
        assert_eq!(p.label(), "60:00");
    }

    #[test]
    fn a_running_timer_cannot_be_reset_by_dragging_or_set_minutes() {
        let mut p = Pomodoro::new();
        p.toggle();
        p.tick(60);
        p.drag_to(0.9);
        assert_eq!((p.left(), p.dragging()), (24 * 60, false));
        assert_eq!(p.set_minutes(45), 45, "the answer is still the snapped value");
        assert_eq!(p.duration(), 25 * 60, "but the running block is untouched");
        p.reset();
        assert_eq!(p.set_minutes(47), 45);
        assert_eq!(p.label(), "45:00");
        assert_eq!(p.set_minutes(0), 5);
        assert_eq!(p.set_minutes(1_000_000), 60);
    }

    #[test]
    fn hostile_numbers_are_harmless() {
        let mut p = Pomodoro::new();
        p.drag_to(f64::NAN);
        p.drag_to(f64::INFINITY);
        assert!(!p.dragging());
        p.drag_to(-3.25);
        assert!(p.left() <= DIAL_SECONDS);
        p.end_drag();
        p.toggle();
        assert_eq!(p.tick(u32::MAX), Event::Completed);
    }
}
