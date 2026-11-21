//! Frame pacing for a window the display does not pace (v0.9.3).
//!
//! The shell redraws continuously and leaves the pace to the display (`PresentMode::Fifo`): taking the next drawable
//! waits for a refresh. A window that is fully covered, minimised or off every screen gets no such wait (macOS stops
//! handing out paced drawables to an occluded window; on Windows a minimised window's present returns at once), so the
//! loop would spin: measured on the home machine, 60 550 frames a second and 175 % CPU for a covered glance-proof
//! window (GlancEhr measured 6 500 a second in its own window): frames that get no drawable, one after the other
//! (presents still came every 16 ms). The [`Pacer`] holds a redraw until about one refresh has passed since the last
//! frame began when
//!
//! - the OS says the window is hidden (winit's `Occluded` on macOS, `is_minimized` elsewhere), or
//! - the last frame got no drawable (the spin above, whatever the OS says), or
//! - the frames came less than [`UNPACED`] apart twice in a row, which no display paces (240 Hz is 4.2 ms): the last
//!   net, for a window that gets drawables at once and that the OS does not report; it lets at most two frames through
//!   a refresh. Once is not enough: a visible window's FIFO queue hands out a drawable at once now and then after a
//!   late frame (1 to 14 times in 8 s in the runs on the home machine, never twice in a row), and holding that frame would present
//!   it after the refresh it was in time for.
//!
//! The wait is the event loop's (`ControlFlow::WaitUntil`), not a sleep, so menus, OS events and the other windows
//! carry on. The rate while hidden is one display refresh, the same as visible: apps run their engine in their frame
//! (GlancEhr's show), so hiding the window never slows it. Shown again, the next frame is drawn at once.

use std::time::{Duration, Instant};

/// A frame that begins sooner than this after the last one was not paced by a display.
pub(crate) const UNPACED: Duration = Duration::from_millis(2);

/// One refresh of a display at `millihertz` (24 to 240 Hz; 60 Hz when the OS gives none), less 5 %, so a held frame is
/// never later than the display's own refresh would have been.
pub(crate) fn refresh(millihertz: Option<u32>) -> Duration {
    let hz = millihertz.map_or(60.0, |m| f64::from(m) / 1000.0).clamp(24.0, 240.0);
    Duration::from_secs_f64(0.95 / hz)
}

/// The pace of one window's frames.
#[derive(Debug, Default)]
pub(crate) struct Pacer {
    /// When the last frame began.
    last: Option<Instant>,
    /// The last frame began less than [`UNPACED`] after the one before.
    quick: bool,
    /// The last frame got no drawable.
    starved: bool,
    /// The OS says the window cannot be seen.
    hidden: bool,
    /// A held redraw is due then.
    due: Option<Instant>,
}

impl Pacer {
    /// A redraw is about to run at `now` (`refresh`: [`refresh`] of the window's display). `None`: draw it. `Some(t)`:
    /// hold it until `t`; [`Pacer::take_due`] says when to ask for it again.
    pub(crate) fn hold(&mut self, now: Instant, refresh: Duration) -> Option<Instant> {
        if let Some(last) = self.last {
            let since = now.saturating_duration_since(last);
            let quick = since < UNPACED;
            if since < refresh && (self.hidden || self.starved || (quick && self.quick)) {
                let due = last + refresh;
                self.due = Some(due);
                return Some(due);
            }
            self.quick = quick;
        }
        self.last = Some(now);
        self.due = None;
        None
    }

    /// The frame drawn after [`Pacer::hold`] said draw got a drawable and was presented (`false`: none came).
    pub(crate) fn presented(&mut self, presented: bool) {
        self.starved = !presented;
    }

    /// The OS says the window is hidden (covered, minimised) or can be seen again. Seen again, the next frame is drawn
    /// at once; `true`: a redraw was being held, ask for it now.
    pub(crate) fn set_hidden(&mut self, hidden: bool) -> bool {
        let shown = self.hidden && !hidden;
        self.hidden = hidden;
        if !shown {
            return false;
        }
        (self.last, self.quick, self.starved) = (None, false, false);
        self.due.take().is_some()
    }

    /// When the held redraw is due (the event loop sleeps until the earliest of all windows').
    pub(crate) fn due(&self) -> Option<Instant> {
        self.due
    }

    /// `true` once the held redraw is due at `now` (it is no longer held: ask for it).
    pub(crate) fn take_due(&mut self, now: Instant) -> bool {
        match self.due {
            Some(t) if t <= now => {
                self.due = None;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    /// Run the loop as the shell does for `span` from `start`: a redraw is asked for as soon as a frame ends (`work`
    /// after it began) or once a held one is due; every frame gets a drawable when `drawable`. The frame starts drawn.
    fn run(p: &mut Pacer, start: Instant, span: Duration, work: Duration, refresh: Duration, drawable: bool) -> Vec<Instant> {
        let mut drawn = Vec::new();
        let mut now = start;
        while now < start + span {
            match p.hold(now, refresh) {
                None => {
                    drawn.push(now);
                    p.presented(drawable);
                    now += work;
                }
                Some(due) => {
                    assert!(due > now, "a held redraw is due later");
                    now = due;
                    assert!(p.take_due(now));
                }
            }
        }
        drawn
    }

    #[test]
    fn the_refresh_is_the_displays_less_five_percent_with_60_hz_when_unknown_and_clamped() {
        assert_eq!(refresh(Some(120_000)), Duration::from_secs_f64(0.95 / 120.0));
        assert_eq!(refresh(None), Duration::from_secs_f64(0.95 / 60.0));
        assert_eq!(refresh(Some(1_000)), Duration::from_secs_f64(0.95 / 24.0));
        assert_eq!(refresh(Some(500_000)), Duration::from_secs_f64(0.95 / 240.0));
    }

    #[test]
    fn a_hidden_window_draws_once_a_refresh_not_as_fast_as_the_loop_turns() {
        // Covered: no drawable wait, a frame takes 0.02 ms. One second at 120 Hz: about 126 frames (0.95 of a refresh
        // apart), never 50 000.
        let r = refresh(Some(120_000));
        let mut p = Pacer::default();
        p.set_hidden(true);
        let t0 = Instant::now();
        let drawn = run(&mut p, t0, Duration::from_secs(1), Duration::from_micros(20), r, true);
        assert!((120..=128).contains(&drawn.len()), "{} frames", drawn.len());
        assert!(drawn.windows(2).all(|w| w[1] - w[0] >= r));
    }

    #[test]
    fn frames_without_a_drawable_are_paced_whatever_the_os_says() {
        // The spin measured on macOS: the OS has not said "covered" (or never does), no drawable comes, a frame takes
        // 0.1 ms. Held to one refresh each.
        let r = refresh(Some(60_000));
        let mut p = Pacer::default();
        let drawn = run(&mut p, Instant::now(), Duration::from_secs(1), Duration::from_micros(100), r, false);
        assert!((60..=64).contains(&drawn.len()), "{} frames", drawn.len());
    }

    #[test]
    fn an_unreported_spin_with_drawables_gets_at_most_two_frames_a_refresh() {
        let r = refresh(Some(60_000));
        let mut p = Pacer::default();
        let drawn = run(&mut p, Instant::now(), Duration::from_secs(1), Duration::from_micros(100), r, true);
        assert!((120..=130).contains(&drawn.len()), "{} frames", drawn.len());
    }

    #[test]
    fn one_quick_drawable_in_a_visible_window_is_not_held() {
        // FIFO catching up after a late frame: one drawable at once, then the display paces again.
        let r = refresh(Some(120_000));
        let mut p = Pacer::default();
        let t0 = Instant::now();
        let mut now = t0;
        for i in 0..240u32 {
            assert_eq!(p.hold(now, r), None, "frame {i}");
            p.presented(true);
            now += match i % 50 {
                10 => 20 * MS,
                11 => Duration::from_micros(300),
                _ => Duration::from_micros(8_333),
            };
        }
    }

    #[test]
    fn after_a_frame_without_a_drawable_the_next_waits_out_the_refresh() {
        // A resize made the surface outdated: no drawable for that frame; the next comes a refresh later at most.
        let r = refresh(Some(120_000));
        let mut p = Pacer::default();
        let t0 = Instant::now();
        assert_eq!(p.hold(t0, r), None);
        p.presented(false);
        assert_eq!(p.hold(t0 + 3 * MS, r), Some(t0 + r));
        assert!(p.take_due(t0 + r));
        assert_eq!(p.hold(t0 + r, r), None);
        p.presented(true);
        assert_eq!(p.hold(t0 + r + 3 * MS, r), None, "a drawable came: no longer held");
    }

    #[test]
    fn a_visible_window_paced_by_the_display_is_never_held() {
        // Visible: the drawable wait makes each frame a refresh long (8.33 ms at 120 Hz), or longer after a hitch;
        // nothing is held, so the display alone sets the pace.
        let mut p = Pacer::default();
        let r = refresh(Some(120_000));
        let t0 = Instant::now();
        let mut now = t0;
        for i in 0..240u32 {
            assert_eq!(p.hold(now, r), None, "frame {i}");
            p.presented(true);
            now += if i == 100 { 40 * MS } else { Duration::from_micros(8_333) };
        }
        assert_eq!(p.due(), None);
    }

    #[test]
    fn shown_again_the_next_frame_is_drawn_at_once() {
        let r = refresh(Some(120_000));
        let mut p = Pacer::default();
        p.set_hidden(true);
        let t0 = Instant::now();
        assert_eq!(p.hold(t0, r), None);
        // Covered: the next redraw 3 ms later is held until a refresh after t0.
        assert_eq!(p.hold(t0 + 3 * MS, r), Some(t0 + r));
        assert_eq!(p.due(), Some(t0 + r));
        assert!(!p.take_due(t0 + 4 * MS), "not due yet");
        // Uncovered at 5 ms: the held redraw is asked for now and drawn now, not at t0 + r.
        assert!(p.set_hidden(false));
        assert_eq!(p.due(), None);
        assert_eq!(p.hold(t0 + 5 * MS, r), None);
        // And the display paces it from there: a refresh later, drawn.
        assert_eq!(p.hold(t0 + 5 * MS + Duration::from_micros(8_333), r), None);
    }

    #[test]
    fn hidden_again_or_still_visible_asks_for_nothing() {
        let mut p = Pacer::default();
        assert!(!p.set_hidden(false));
        assert!(!p.set_hidden(true));
        assert!(!p.set_hidden(true));
        // Shown with nothing held: nothing to ask for, but the next frame is still drawn at once.
        let t0 = Instant::now();
        assert_eq!(p.hold(t0, refresh(None)), None);
        assert!(!p.set_hidden(false));
        assert_eq!(p.hold(t0 + 3 * MS, refresh(None)), None);
    }
}
