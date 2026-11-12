//! The checks a new graphics card voltage goes through before it counts, kept by the app's engine on a clock it is
//! handed (milliseconds that only go forward), so the tests can drive every step. The owner's rules (docs/DESIGN.md,
//! GPU undervolt):
//!
//! 1. After Try the value runs at once and a 10 second question follows: "Keep -30 mV? Going back to 0 mV in 10 s".
//!    No answer, Go back, B, or Turbine Hub closing goes back to what was on before (the value checked earlier in
//!    this session, else 0).
//! 2. Keep there doesn't keep it yet: the value stays on for 5 minutes of play while the card is watched. Stop
//!    goes back at once. A sleep, a driver reset (the card reading 0 with no sleep), a card that can't be read or
//!    holds something else, or Turbine Hub closing goes back too, and says why.
//! 3. After the 5 minutes the question comes again with 60 seconds to answer. Only Keep there makes the value
//!    checked, and it stays on until the machine restarts. Nothing ever turns it on again by itself.
//!
//! A value that passed all of it on this kernel before may skip the 5 minutes (`short`): only the 10 second
//! question is asked again. Everything this module asks for is going back, never a new value.

pub const ASK_MS: u64 = 10_000;
pub const CHECK_MS: u64 = 300_000;
pub const AGAIN_MS: u64 = 60_000;
/// How often the card is read while a value is on.
pub const LOOK_MS: u64 = 2_000;

/// A clock in milliseconds: the real one since the engine started, or one the tests (and the pictures) set.
#[derive(Debug, Clone)]
pub struct Clock {
    start: std::time::Instant,
    set: Option<u64>,
}

impl Default for Clock {
    fn default() -> Clock {
        Clock { start: std::time::Instant::now(), set: None }
    }
}

impl Clock {
    pub fn now(&self) -> u64 {
        self.set.unwrap_or_else(|| self.start.elapsed().as_millis() as u64)
    }
    /// Stops the clock at this time (tests, pictures).
    pub fn set(&mut self, ms: u64) {
        self.set = Some(ms);
    }
}

/// Where a value being checked is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The 10 second question after Try.
    Ask,
    /// The 5 minutes of play.
    Check,
    /// The 60 second question after them.
    Again,
}

impl Stage {
    pub fn key(self) -> &'static str {
        match self {
            Stage::Ask => "ask",
            Stage::Check => "check",
            Stage::Again => "again",
        }
    }
    pub fn from_key(k: &str) -> Option<Stage> {
        [Stage::Ask, Stage::Check, Stage::Again].into_iter().find(|s| s.key() == k)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Underway {
    pub stage: Stage,
    /// The value being checked, and what goes back on when it doesn't pass.
    pub offset: i32,
    pub back: i32,
    /// When the stage ends (the clock's milliseconds).
    pub until: u64,
    /// Passed the whole check on this kernel before: Keep in the 10 second question is enough.
    pub short: bool,
}

/// What the engine asks the helper for. Only ever going back, or noting a driver reset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Do {
    /// Write this offset (0, or the value checked earlier in this session).
    Back { to: i32 },
    /// The driver reset the card while this offset was on: the helper notes it (the card is at 0 already).
    Trip { offset: i32 },
}

/// What happened, for the window: what to run, what to say, and whether a question opens or closes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Out {
    pub run: Option<Do>,
    pub say: Option<String>,
    /// The 60 second question opens now.
    pub again: bool,
    /// The question on screen is over (answered by the clock, or the check ended another way).
    pub close: bool,
}

/// What the card was seen holding: its offset (None when it couldn't be read), the kernel's count of sleeps, and
/// the offset in the helper's crash mark from this boot (None: no mark). A card at 0 while the mark still names
/// another value is a driver reset; a mark of 0, or none, means the 0 was asked for (Safe reset, Undo) or a sleep
/// cleared it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub card: Option<i32>,
    pub suspends: Option<u64>,
    pub armed: Option<i32>,
}

/// The checks of this session. `on` is the value checked and on until the machine restarts (None: the card's own
/// voltage, or whatever was there before Turbine Hub looked).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    pub underway: Option<Underway>,
    pub on: Option<i32>,
    /// The sleeps counted at the last look, to tell a sleep from a driver reset.
    pub suspends: Option<u64>,
}

/// "-30 mV", and "0 mV" for the card's own voltage.
pub fn mv(v: i32) -> String {
    format!("{v} mV")
}

/// "4:12".
pub fn clock_words(ms: u64) -> String {
    let s = ms.div_ceil(1000);
    format!("{}:{:02}", s / 60, s % 60)
}

impl Session {
    /// What goes back on when a new value doesn't pass: the value checked earlier in this session, else 0.
    pub fn back_to(&self) -> i32 {
        self.on.unwrap_or(0)
    }

    /// Seconds left in the stage (rounded up), for the countdowns.
    pub fn left(&self, now: u64) -> Option<u64> {
        self.underway.as_ref().map(|u| u.until.saturating_sub(now).div_ceil(1000))
    }

    /// The helper wrote the tried value: the 10 second question starts.
    pub fn tried(&mut self, now: u64, offset: i32, short: bool) {
        self.underway = Some(Underway { stage: Stage::Ask, offset, back: self.back_to(), until: now + ASK_MS, short });
    }

    /// Keep was pressed. In the 10 second question it starts the 5 minutes (or, `short`, checks the value at once);
    /// in the 60 second question it checks the value. Anything else is too late and does nothing.
    pub fn keep(&mut self, now: u64) -> Out {
        let Some(u) = self.underway.clone().filter(|u| now < u.until) else { return Out::default() };
        match u.stage {
            Stage::Ask if !u.short => {
                self.underway = Some(Underway { stage: Stage::Check, until: now + CHECK_MS, ..u });
                Out { say: Some(format!("Checking {} for 5 minutes. Play something you know; Stop goes back at once.", mv(u.offset))), ..Out::default() }
            }
            Stage::Ask | Stage::Again => {
                self.underway = None;
                self.on = Some(u.offset);
                Out { say: Some(format!("{} is on until the machine restarts", mv(u.offset))), ..Out::default() }
            }
            Stage::Check => Out::default(),
        }
    }

    /// Going back: Go back, B, Stop, or Turbine Hub closing (`why`). Nothing to do when nothing is being checked.
    pub fn back(&mut self, why: &str) -> Out {
        let Some(u) = self.underway.take() else { return Out::default() };
        Out { run: Some(Do::Back { to: u.back }), say: Some(format!("{why}: back to {}", mv(u.back))), close: true, ..Out::default() }
    }

    /// Something went wrong while a value was being checked: back to 0, and why.
    fn fail(&mut self, why: String) -> Out {
        self.underway = None;
        self.on = None;
        Out { run: Some(Do::Back { to: 0 }), say: Some(format!("{why}, so the graphics card is back at 0 mV")), close: true, ..Out::default() }
    }

    /// The clock moved on: a question nobody answered goes back, the 5 minutes end in the second question.
    pub fn tick(&mut self, now: u64) -> Out {
        let Some(u) = self.underway.clone().filter(|u| now >= u.until) else { return Out::default() };
        match u.stage {
            Stage::Check => {
                self.underway = Some(Underway { stage: Stage::Again, until: now + AGAIN_MS, ..u });
                Out { again: true, ..Out::default() }
            }
            Stage::Ask | Stage::Again => self.back("No answer"),
        }
    }

    /// The card was read. While a value is being checked anything but that value on the card ends the check; with a
    /// checked value on, a 0 with no sleep in between is a driver reset, and a sleep is reported.
    pub fn look(&mut self, l: Look) -> Out {
        let slept = matches!((self.suspends, l.suspends), (Some(a), Some(b)) if b != a);
        if l.suspends.is_some() {
            self.suspends = l.suspends;
        }
        if let Some(u) = self.underway.clone() {
            return match l.card {
                _ if slept => self.fail("The machine slept during the check".into()),
                None => self.fail("The graphics card's voltage couldn't be read".into()),
                Some(0) if u.offset != 0 && l.armed.is_some_and(|a| a != 0) => {
                    let mut out = self.fail("The graphics driver reset the card during the check".into());
                    out.run = Some(Do::Trip { offset: u.offset });
                    out
                }
                Some(v) if v != u.offset => self.fail(format!("The graphics card changed to {} by itself", mv(v))),
                Some(_) => Out::default(),
            };
        }
        let Some(on) = self.on.filter(|v| *v != 0) else { return Out::default() };
        match l.card {
            Some(v) if v == on && slept => Out { say: Some(format!("{} is still on after the sleep", mv(on))), ..Out::default() },
            Some(v) if v == on => Out::default(),
            Some(0) if slept => {
                self.on = None;
                Out { say: Some(format!("{} was off after the sleep. Use it again from Graphics card if you want it", mv(on))), ..Out::default() }
            }
            Some(0) if l.armed.is_some_and(|a| a != 0) => {
                self.on = None;
                Out { run: Some(Do::Trip { offset: on }), say: Some(format!("The graphics driver reset the card, so {} is off. Try a smaller step?", mv(on))), ..Out::default() }
            }
            // Put back to 0 on purpose (Safe reset, Undo), changed elsewhere, or unreadable: no longer this
            // session's value.
            _ => {
                self.on = None;
                Out::default()
            }
        }
    }

    /// Turbine Hub is closing: a value still being checked goes back.
    pub fn close(&mut self) -> Out {
        self.back("Turbine Hub closed during the check")
    }

    /// The words for the top bar and the section while a value is being checked: "Checking -30 mV: 4:12 left".
    pub fn words(&self, now: u64) -> String {
        match &self.underway {
            Some(u) if u.stage == Stage::Check => format!("Checking {}: {} left", mv(u.offset), clock_words(u.until.saturating_sub(now))),
            Some(u) if u.stage == Stage::Again => format!("Checked {}: answer within {} s", mv(u.offset), u.until.saturating_sub(now).div_ceil(1000)),
            Some(u) => format!("Trying {}: {} s", mv(u.offset), u.until.saturating_sub(now).div_ceil(1000)),
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(card: i32, suspends: u64) -> Look {
        Look { card: Some(card), suspends: Some(suspends), armed: Some(-30) }
    }

    #[test]
    fn the_ten_second_question_goes_back_by_itself() {
        let mut s = Session::default();
        s.tried(1_000, -30, false);
        assert_eq!(s.left(1_000), Some(10));
        assert_eq!(s.left(1_001), Some(10));
        assert_eq!(s.left(10_500), Some(1));
        assert_eq!(s.words(1_000), "Trying -30 mV: 10 s");
        assert_eq!(s.tick(10_999), Out::default());
        let out = s.tick(11_000);
        assert_eq!(out.run, Some(Do::Back { to: 0 }));
        assert!(out.close && out.say.unwrap().contains("No answer: back to 0 mV"));
        assert_eq!(s, Session::default());
        // Keep after the end is too late.
        assert_eq!(s.keep(11_500), Out::default());
        assert_eq!(s.tick(20_000), Out::default());
    }

    #[test]
    fn go_back_and_closing_go_back_to_the_value_checked_before() {
        let mut s = Session { on: Some(-20), ..Session::default() };
        s.tried(0, -40, false);
        assert_eq!(s.underway.as_ref().unwrap().back, -20);
        let out = s.back("Back");
        assert_eq!(out.run, Some(Do::Back { to: -20 }));
        assert_eq!(s.on, Some(-20));
        assert!(s.underway.is_none());
        // Going back twice runs nothing more.
        assert_eq!(s.back("Back"), Out::default());
        // Closing the window during the 10 seconds, and during the 5 minutes.
        s.tried(0, -40, false);
        assert_eq!(s.close().run, Some(Do::Back { to: -20 }));
        s.tried(0, -40, false);
        s.keep(5_000);
        let out = s.close();
        assert_eq!(out.run, Some(Do::Back { to: -20 }));
        assert!(out.say.unwrap().starts_with("Turbine Hub closed during the check"));
        // Nothing underway: closing leaves the checked value on.
        assert_eq!(s.close(), Out::default());
        assert_eq!(s.on, Some(-20));
    }

    #[test]
    fn keep_starts_five_minutes_then_asks_again_with_sixty_seconds() {
        let mut s = Session::default();
        s.tried(0, -30, false);
        let out = s.keep(4_000);
        assert!(out.run.is_none() && out.say.unwrap().contains("5 minutes"));
        assert_eq!(s.on, None, "not kept yet");
        assert_eq!(s.underway.as_ref().unwrap().stage, Stage::Check);
        assert_eq!(s.words(4_000 + 48_000), "Checking -30 mV: 4:12 left");
        assert_eq!(s.left(4_000), Some(300));
        // Keep can't be pressed in the middle of the 5 minutes.
        assert_eq!(s.keep(10_000), Out::default());
        assert_eq!(s.tick(303_999), Out::default());
        let out = s.tick(304_000);
        assert!(out.again && out.run.is_none());
        assert_eq!(s.underway.as_ref().unwrap().stage, Stage::Again);
        assert_eq!(s.left(304_000), Some(60));
        // Keep there: on until the restart.
        let out = s.keep(330_000);
        assert!(out.say.unwrap().contains("on until the machine restarts"));
        assert_eq!((s.on, s.underway.is_none()), (Some(-30), true));
    }

    #[test]
    fn the_second_question_unanswered_goes_back() {
        let mut s = Session { on: Some(-10), ..Session::default() };
        s.tried(0, -30, false);
        s.keep(1_000);
        assert!(s.tick(301_000).again);
        let out = s.tick(361_000);
        assert_eq!(out.run, Some(Do::Back { to: -10 }));
        assert!(out.close);
        assert_eq!(s.on, Some(-10));
    }

    #[test]
    fn stop_during_the_five_minutes_goes_back_at_once() {
        let mut s = Session::default();
        s.tried(0, -30, false);
        s.keep(1_000);
        let out = s.back("Stopped");
        assert_eq!(out.run, Some(Do::Back { to: 0 }));
        assert_eq!(out.say.as_deref(), Some("Stopped: back to 0 mV"));
        assert_eq!(s.tick(400_000), Out::default());
    }

    #[test]
    fn a_driver_reset_a_sleep_or_an_unreadable_card_ends_the_check_at_0() {
        let start = |s: &mut Session| {
            s.look(look(0, 3));
            s.tried(0, -30, false);
            s.keep(1_000);
            assert_eq!(s.look(look(-30, 3)), Out::default());
        };
        // The card reads 0 with no sleep: the driver reset it. The helper notes the trip.
        let mut s = Session { on: Some(-10), ..Session::default() };
        start(&mut s);
        let out = s.look(look(0, 3));
        assert_eq!(out.run, Some(Do::Trip { offset: -30 }));
        assert!(out.close && out.say.unwrap().contains("driver reset"));
        assert_eq!((s.on, s.underway.is_none()), (None, true));
        // A sleep in the middle: back to 0, not to the value from before.
        let mut s = Session { on: Some(-10), ..Session::default() };
        start(&mut s);
        let out = s.look(look(-30, 4));
        assert_eq!(out.run, Some(Do::Back { to: 0 }));
        assert!(out.say.unwrap().contains("slept"));
        // Unreadable, or changed by something else.
        let mut s = Session::default();
        start(&mut s);
        assert_eq!(s.look(Look { card: None, suspends: Some(3), armed: Some(-30) }).run, Some(Do::Back { to: 0 }));
        let mut s = Session::default();
        start(&mut s);
        assert!(s.look(look(-50, 3)).say.unwrap().contains("changed to -50 mV"));
        // In the 10 second question too.
        let mut s = Session::default();
        s.look(look(0, 3));
        s.tried(0, -30, false);
        assert_eq!(s.look(look(0, 3)).run, Some(Do::Trip { offset: -30 }));
    }

    #[test]
    fn a_checked_value_is_watched_after_a_wake_and_never_put_back() {
        let mut s = Session { on: Some(-30), ..Session::default() };
        s.look(look(-30, 5));
        // Still on after a sleep: said once, nothing written.
        let out = s.look(look(-30, 6));
        assert_eq!((out.run, out.say.as_deref()), (None, Some("-30 mV is still on after the sleep")));
        assert_eq!(s.look(look(-30, 6)), Out::default());
        // Off after a sleep: said, offered again on the page, never written back.
        let out = s.look(look(0, 7));
        assert!(out.run.is_none() && out.say.unwrap().contains("off after the sleep"));
        assert_eq!(s.on, None);
        // A 0 with no sleep: a driver reset, noted.
        let mut s = Session { on: Some(-30), ..Session::default() };
        s.look(look(-30, 5));
        assert_eq!(s.look(look(0, 5)).run, Some(Do::Trip { offset: -30 }));
        assert_eq!(s.on, None);
        // A 0 that was asked for (Safe reset, Undo: the mark says 0 or is gone): no longer on, nothing noted.
        for armed in [Some(0), None] {
            let mut s = Session { on: Some(-30), ..Session::default() };
            s.look(look(-30, 5));
            assert_eq!(s.look(Look { card: Some(0), suspends: Some(5), armed }), Out::default());
            assert_eq!(s.on, None);
        }
    }

    #[test]
    fn a_value_checked_before_on_this_kernel_skips_the_five_minutes() {
        let mut s = Session::default();
        s.tried(0, -30, true);
        let out = s.keep(3_000);
        assert!(out.run.is_none() && out.say.unwrap().contains("until the machine restarts"));
        assert_eq!(s.on, Some(-30));
        // But it is still only a try until Keep.
        let mut s = Session::default();
        s.tried(0, -30, true);
        assert_eq!(s.tick(10_000).run, Some(Do::Back { to: 0 }));
    }

    #[test]
    fn words_and_clock() {
        assert_eq!(clock_words(300_000), "5:00");
        assert_eq!(clock_words(252_000), "4:12");
        assert_eq!(clock_words(1), "0:01");
        let mut c = Clock::default();
        c.set(42);
        assert_eq!(c.now(), 42);
        assert_eq!(Stage::from_key("again"), Some(Stage::Again));
    }
}
