//! Controllers through SDL. In Game Mode, Steam hands non-Steam apps a virtual Xbox pad, the same path games use;
//! SDL's controller mappings cover Xbox pads and the handhelds' built-in controls the same way. Buttons, the triggers
//! and the left stick pushed one way all become plain presses ("a", "lt", "up").
//!
//! Presses only count while Turbine Hub's window is in front: SDL keeps reading the controller behind a game (so
//! what is held and let go stays true), but only releases are passed on then, and for a moment after the window
//! comes back.

use sdl2::controller::{Axis, Button, GameController};
use sdl2::event::Event;
use sdl2::{EventPump, GameControllerSubsystem, Sdl};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How far a trigger must move to count as a press, and where it lets go again (so it never flickers).
const PRESS_AT: i16 = 20_000;
const RELEASE_AT: i16 = 12_000;
/// The same tap seen on two devices (Steam's virtual pad and the raw one) arrives within this many milliseconds.
const TWIN_MS: u128 = 40;
/// The left stick in `dedupe`'s books: a pad of its own, so a direction held on the D-pad and on the stick counts once.
const STICK: u32 = u32::MAX;

/// Left stick, as a part of its full travel: past `PUSH_ON` it counts as pushed (a worn stick resting off center
/// never gets there), and it has let go once it is back under `LET_GO`.
pub const PUSH_ON: f64 = 0.5;
pub const LET_GO: f64 = 0.35;
/// A push must last this long to be a press, so a flick across the middle is nothing.
pub const COMMIT_MS: f64 = 80.0;
/// A held push turns into another direction only once it points this far (degrees) from the one it holds, well past
/// the diagonal, so a thumb resting near a corner never flips between two.
pub const TURN_DEG: f64 = 55.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Push {
    #[default]
    Idle,
    /// Pushed, for this many milliseconds so far.
    Pending(f64),
    Held(&'static str),
    /// Ignored until it has been back in the middle (it was pushed while the window was away).
    Spent,
}

/// The left stick as a D-pad: one direction per push, the larger axis wins.
#[derive(Default)]
pub struct Stick {
    push: Push,
}

/// The direction a stick position points (y down): the larger axis wins.
pub fn direction(x: f64, y: f64) -> &'static str {
    if x.abs() >= y.abs() {
        if x >= 0.0 { "right" } else { "left" }
    } else if y >= 0.0 {
        "down"
    } else {
        "up"
    }
}

/// How far (degrees) a stick position points away from a direction's own axis.
fn off_axis(dir: &str, x: f64, y: f64) -> f64 {
    let (ax, ay) = match dir {
        "right" => (1.0, 0.0),
        "left" => (-1.0, 0.0),
        "down" => (0.0, 1.0),
        _ => (0.0, -1.0),
    };
    let m = x.hypot(y);
    if m <= 0.0 {
        return 0.0;
    }
    ((x * ax + y * ay) / m).clamp(-1.0, 1.0).acos().to_degrees()
}

impl Stick {
    /// The stick is at (x, y), each -1 to 1 with y down, `dt_ms` after the last look. Answers the direction let go
    /// and the direction pressed, if any (both when a held push turned on to another direction).
    pub fn step(&mut self, x: f64, y: f64, dt_ms: f64) -> (Option<&'static str>, Option<&'static str>) {
        let m = x.hypot(y);
        let (mut up, mut down) = (None, None);
        self.push = match self.push {
            Push::Idle if m >= PUSH_ON => Push::Pending(0.0),
            Push::Idle => Push::Idle,
            Push::Pending(_) if m < LET_GO => Push::Idle,
            Push::Pending(ms) if ms + dt_ms >= COMMIT_MS => {
                // Where it points now: a push that curved on the way out still counts.
                let dir = direction(x, y);
                down = Some(dir);
                Push::Held(dir)
            }
            Push::Pending(ms) => Push::Pending(ms + dt_ms),
            Push::Held(dir) if m < LET_GO => {
                up = Some(dir);
                Push::Idle
            }
            Push::Held(dir) => {
                let now = direction(x, y);
                if now != dir && off_axis(dir, x, y) >= TURN_DEG {
                    up = Some(dir);
                    down = Some(now);
                    Push::Held(now)
                } else {
                    Push::Held(dir)
                }
            }
            Push::Spent if m < LET_GO => Push::Idle,
            Push::Spent => Push::Spent,
        };
        (up, down)
    }

    /// The window stopped listening (it went to the back, or just came to the front): a held direction is let go
    /// (answered, to pass on as a release) and a stick still pushed does nothing until it has been in the middle.
    pub fn let_go(&mut self) -> Option<&'static str> {
        let held = match self.push {
            Push::Held(dir) => Some(dir),
            _ => None,
        };
        if self.push != Push::Idle {
            self.push = Push::Spent;
        }
        held
    }
}

/// Whether presses count right now: only while Turbine Hub's window is in front, and not for a moment after it
/// came back. Releases always pass, so nothing stays held down in the interface.
struct Gate {
    front: bool,
    quiet_until: Option<Instant>,
}

impl Gate {
    fn new() -> Self {
        Gate { front: true, quiet_until: None }
    }

    /// The window came to the front at `now` (presses stay out for `quiet`), or went behind another.
    fn set_front(&mut self, front: bool, now: Instant, quiet: Duration) {
        self.front = front;
        self.quiet_until = front.then_some(now + quiet);
    }

    /// What of `events` is passed on at `now`.
    fn pass(&mut self, mut events: Vec<(&'static str, bool)>, now: Instant) -> Vec<(&'static str, bool)> {
        let quiet = self.quiet_until.is_some_and(|until| now < until);
        if !quiet {
            self.quiet_until = None;
        }
        if !self.front || quiet {
            events.retain(|(_, pressed)| !*pressed);
        }
        events
    }
}

pub struct Pads {
    sdl: Option<Sdl>,
    subsystem: Option<GameControllerSubsystem>,
    pump: Option<EventPump>,
    open: Vec<GameController>,
    /// Which pad holds each button down right now.
    held: HashMap<&'static str, u32>,
    /// When each button was last let go, and on which pad.
    last_up: HashMap<&'static str, (u32, Instant)>,
    /// Triggers currently counted as pressed, per pad.
    axis_down: HashMap<(u32, &'static str), bool>,
    /// Each pad's left stick: x and y (SDL's -32768..32767, y down).
    sticks: HashMap<u32, [i16; 2]>,
    stick: Stick,
    last_tick: Instant,
    last_rescan: Instant,
    gate: Gate,
    /// A controller went away since the last `take_lost`.
    lost: bool,
}

/// Steam sets these for apps it starts so they only see its virtual pad; in Desktop Mode that can leave the app with
/// no pad at all. They are cleared so every pad SDL can see counts (twins are told apart by `dedupe`). Called by main
/// before any thread starts.
pub fn clear_steam_filters() {
    for k in ["SDL_GAMECONTROLLER_IGNORE_DEVICES", "SDL_GAMECONTROLLER_IGNORE_DEVICES_EXCEPT"] {
        if std::env::var_os(k).is_some() {
            // SAFETY: main calls this before it starts any thread, so nothing else reads the environment yet.
            unsafe { std::env::remove_var(k) };
        }
    }
}

/// One line about the controllers: into Turbine Hub's log (a window on the real machine has one), and on the
/// terminal for whoever started it from there.
fn note(line: &str) {
    crate::log::line("pad", line);
    eprintln!("turbinehub pad: {line}");
}

/// What the sandbox sees of the input devices, said when no controller shows: how many event devices there are and
/// how many Turbine Hub may read.
fn input_report() -> String {
    let Ok(rd) = std::fs::read_dir("/dev/input") else { return "the input devices folder isn't visible".into() };
    let events: Vec<std::path::PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event"))).collect();
    let readable = events.iter().filter(|p| std::fs::File::open(p).is_ok()).count();
    format!("{} input device(s), {readable} readable by Turbine Hub", events.len())
}

/// An axis value as -1 to 1.
fn unit(v: i16) -> f64 {
    (f64::from(v) / 32767.0).clamp(-1.0, 1.0)
}

impl Pads {
    /// Opens SDL's controller support. If SDL isn't available the app still runs, with keyboard, mouse and touch.
    pub fn open() -> Self {
        // SDL goes on reading the controller behind another window, so a button let go there is seen let go here;
        // what was pressed there never counts (`set_front`, `poll`).
        sdl2::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
        // SDL would catch SIGTERM and SIGINT and turn them into a quit event nobody reads, so Steam, a logout or a
        // shutdown couldn't close the app. Qt and the system keep their usual handling.
        sdl2::hint::set("SDL_NO_SIGNAL_HANDLERS", "1");
        let sdl = sdl2::init().ok();
        let subsystem = sdl.as_ref().and_then(|s| s.game_controller().ok());
        let pump = sdl.as_ref().and_then(|s| s.event_pump().ok());
        if sdl.is_none() {
            note("SDL isn't available: keyboard, mouse and touch only");
        }
        let now = Instant::now();
        let mut pads = Pads { sdl, subsystem, pump, open: Vec::new(), held: HashMap::new(), last_up: HashMap::new(), axis_down: HashMap::new(), sticks: HashMap::new(), stick: Stick::default(), last_tick: now, last_rescan: now, gate: Gate::new(), lost: false };
        pads.enumerate(true);
        pads
    }

    /// Opens every game controller SDL lists now. `say`: tell what SDL and the sandbox see even when that is
    /// nothing (at the start, not at every look again).
    fn enumerate(&mut self, say: bool) {
        let Some(sub) = self.subsystem.as_ref() else { return };
        let count = sub.num_joysticks().unwrap_or(0);
        if count == 0 && !say {
            return;
        }
        note(&format!("SDL lists {count} joystick device(s)"));
        for i in 0..count {
            let name = sub.name_for_index(i).unwrap_or_else(|_| "unnamed".into());
            let mapped = sub.is_game_controller(i);
            note(&format!("Device {i}: {name}, {}", if mapped { "a game controller" } else { "not a game controller (no mapping)" }));
            if mapped
                && let Ok(c) = sub.open(i)
                && !self.open.iter().any(|o| o.instance_id() == c.instance_id())
            {
                self.open.push(c);
            }
        }
        if self.open.is_empty() {
            note(&format!("No controller seen: {}", input_report()));
        }
    }

    /// With no controller open, asks SDL again from scratch (at most every `every`): after leaving Game Mode the
    /// controller can come back without SDL hearing it arrive. Answers whether one was found.
    pub fn rescan(&mut self, every: Duration) -> bool {
        if !self.open.is_empty() || self.sdl.is_none() || self.last_rescan.elapsed() < every {
            return false;
        }
        self.last_rescan = Instant::now();
        // Letting go of the only handle shuts SDL's controller part down; taking it again lists the devices afresh.
        self.subsystem = None;
        self.subsystem = self.sdl.as_ref().and_then(|s| s.game_controller().ok());
        self.enumerate(false);
        if !self.open.is_empty() {
            note("A controller showed after asking again");
        }
        !self.open.is_empty()
    }

    /// Is a controller there?
    pub fn connected(&self) -> bool {
        !self.open.is_empty()
    }

    /// The first controller's name ("" with none), for the glyphs.
    pub fn name(&self) -> String {
        self.open.first().map(|c| c.name()).unwrap_or_default()
    }

    /// Whether a controller went away since the last call (whatever was held in the interface stops).
    pub fn take_lost(&mut self) -> bool {
        std::mem::take(&mut self.lost)
    }

    /// The window came to the front (`true`) or went behind another (`false`). Answers what must still be let go in
    /// the interface. Coming back, presses made while a game had the screen (still waiting in SDL's queue) are read
    /// and thrown away and new ones are ignored for `quiet`, so a button pressed in the game as it closes never acts
    /// here; a stick still pushed presses nothing until it has been back in the middle.
    pub fn set_front(&mut self, front: bool, quiet: Duration) -> Vec<(&'static str, bool)> {
        let mut ups: Vec<(&'static str, bool)> = Vec::new();
        if front {
            ups = self.read().into_iter().filter(|(_, down)| !*down).collect();
        }
        self.gate.set_front(front, Instant::now(), quiet);
        if let Some(dir) = self.stick.let_go()
            && self.dedupe(STICK, dir, false, Instant::now())
        {
            ups.push((dir, false));
        }
        ups
    }

    /// Everything that happened since the last call, as (button, pressed) pairs. Behind another window, and for a
    /// moment after coming back, only the releases: nothing stays held down in the interface, and nothing is pressed.
    pub fn poll(&mut self) -> Vec<(&'static str, bool)> {
        let mut events = self.read();
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f64() * 1000.0;
        self.last_tick = now;
        // The stick pushed furthest speaks (Steam's virtual pad and the raw one report the same stick).
        let (mut x, mut y) = (0.0_f64, 0.0_f64);
        for v in self.sticks.values() {
            let (vx, vy) = (unit(v[0]), unit(v[1]));
            if vx.hypot(vy) > x.hypot(y) {
                (x, y) = (vx, vy);
            }
        }
        let (up, down) = self.stick.step(x, y, dt.clamp(0.0, 100.0));
        for (dir, pressed) in up.map(|d| (d, false)).into_iter().chain(down.map(|d| (d, true))) {
            if self.dedupe(STICK, dir, pressed, now) {
                events.push((dir, pressed));
            }
        }
        self.gate.pass(events, now)
    }

    /// Reads SDL's queue: controllers coming and going, buttons, the triggers and the stick's place.
    fn read(&mut self) -> Vec<(&'static str, bool)> {
        let mut raw: Vec<(u32, &'static str, bool)> = Vec::new();
        let mut added = Vec::new();
        let mut removed = Vec::new();
        if let Some(pump) = self.pump.as_mut() {
            for ev in pump.poll_iter() {
                match ev {
                    Event::ControllerDeviceAdded { which, .. } => added.push(which),
                    Event::ControllerDeviceRemoved { which, .. } => removed.push(which),
                    Event::ControllerButtonDown { which, button, .. } => {
                        if let Some(b) = button_name(button) {
                            raw.push((which, b, true));
                        }
                    }
                    Event::ControllerButtonUp { which, button, .. } => {
                        if let Some(b) = button_name(button) {
                            raw.push((which, b, false));
                        }
                    }
                    Event::ControllerAxisMotion { which, axis, value, .. } => {
                        match axis {
                            Axis::LeftX => self.sticks.entry(which).or_insert([0; 2])[0] = value,
                            Axis::LeftY => self.sticks.entry(which).or_insert([0; 2])[1] = value,
                            _ => {}
                        }
                        for (name, v) in axis_names(axis, value) {
                            let key = (which, name);
                            let was = *self.axis_down.get(&key).unwrap_or(&false);
                            let now = if was { v > RELEASE_AT } else { v > PRESS_AT };
                            if now != was {
                                self.axis_down.insert(key, now);
                                raw.push((which, name, now));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(sub) = self.subsystem.as_ref() {
            for index in added {
                if let Ok(c) = sub.open(index)
                    && !self.open.iter().any(|o| o.instance_id() == c.instance_id())
                {
                    note(&format!("Controller connected: {}", c.name()));
                    self.open.push(c);
                }
            }
        }
        let now = Instant::now();
        let mut out: Vec<(&'static str, bool)> = raw.into_iter().filter_map(|(pad, b, down)| self.dedupe(pad, b, down, now).then_some((b, down))).collect();
        for id in removed {
            // Everything it held down is let go (it will never send those releases itself).
            let mut gone: Vec<&'static str> = self.held.iter().filter(|(_, pad)| **pad == id).map(|(b, _)| *b).collect();
            gone.sort_unstable();
            for b in gone {
                self.held.remove(b);
                out.push((b, false));
            }
            self.open.retain(|c| c.instance_id() != id);
            self.axis_down.retain(|(pad, _), _| *pad != id);
            self.sticks.remove(&id);
            self.lost = true;
            note("A controller went away");
        }
        out
    }

    /// One tap seen on two pads counts once.
    fn dedupe(&mut self, pad: u32, button: &'static str, down: bool, now: Instant) -> bool {
        if down {
            if self.held.contains_key(button) {
                return false;
            }
            if let Some((other, at)) = self.last_up.get(button)
                && *other != pad
                && now.duration_since(*at).as_millis() < TWIN_MS
            {
                return false;
            }
            self.held.insert(button, pad);
            true
        } else {
            if self.held.get(button) != Some(&pad) {
                return false;
            }
            self.held.remove(button);
            self.last_up.insert(button, (pad, now));
            true
        }
    }
}

fn button_name(b: Button) -> Option<&'static str> {
    Some(match b {
        Button::A => "a",
        Button::B => "b",
        Button::X => "x",
        Button::Y => "y",
        Button::LeftShoulder => "lb",
        Button::RightShoulder => "rb",
        Button::Start => "start",
        Button::Back => "view",
        Button::DPadUp => "up",
        Button::DPadDown => "down",
        Button::DPadLeft => "left",
        Button::DPadRight => "right",
        // The Steam / Guide button belongs to Steam.
        _ => return None,
    })
}

/// The triggers act as buttons (the left stick is read as a place instead, see `Stick`). Yields (name, strength),
/// 0 when idle.
fn axis_names(axis: Axis, value: i16) -> Vec<(&'static str, i16)> {
    let pos = value.max(0);
    match axis {
        Axis::TriggerLeft => vec![("lt", pos)],
        Axis::TriggerRight => vec![("rt", pos)],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Holds the stick at one place for `ms`, a frame at a time, and collects what it sent.
    fn hold(s: &mut Stick, x: f64, y: f64, ms: u32) -> Vec<(&'static str, bool)> {
        let mut out = Vec::new();
        for _ in 0..(ms / 16).max(1) {
            let (up, down) = s.step(x, y, 16.0);
            out.extend(up.map(|d| (d, false)));
            out.extend(down.map(|d| (d, true)));
        }
        out
    }

    #[test]
    fn behind_another_window_only_releases_pass() {
        let mut g = Gate::new();
        let t = Instant::now();
        assert_eq!(g.pass(vec![("a", true), ("a", false)], t), vec![("a", true), ("a", false)]);
        g.set_front(false, t, Duration::from_millis(250));
        assert_eq!(g.pass(vec![("down", true), ("a", true), ("down", false), ("b", true)], t + Duration::from_secs(5)), vec![("down", false)]);
    }

    #[test]
    fn back_in_front_presses_wait_a_moment_and_then_count_again() {
        let mut g = Gate::new();
        let t = Instant::now();
        g.set_front(false, t, Duration::from_millis(250));
        let back = t + Duration::from_secs(60);
        g.set_front(true, back, Duration::from_millis(250));
        // The press that closed the game, still on its way, and its release.
        assert_eq!(g.pass(vec![("a", true)], back + Duration::from_millis(40)), vec![]);
        assert_eq!(g.pass(vec![("a", false)], back + Duration::from_millis(120)), vec![("a", false)]);
        assert_eq!(g.pass(vec![("a", true)], back + Duration::from_millis(249)), vec![]);
        assert_eq!(g.pass(vec![("a", true)], back + Duration::from_millis(251)), vec![("a", true)]);
    }

    #[test]
    fn a_diagonal_push_is_one_direction_the_larger_axis() {
        let mut s = Stick::default();
        assert_eq!(hold(&mut s, 0.9, -0.7, 300), vec![("right", true)]);
        assert_eq!(hold(&mut s, 0.0, 0.0, 50), vec![("right", false)]);
        assert_eq!(hold(&mut s, -0.6, 0.8, 300), vec![("down", true)]);
    }

    #[test]
    fn a_worn_stick_resting_off_center_presses_nothing() {
        let mut s = Stick::default();
        assert!(hold(&mut s, 0.3, -0.35, 2000).is_empty());
    }

    #[test]
    fn a_flick_shorter_than_a_press_is_nothing() {
        let mut s = Stick::default();
        assert!(hold(&mut s, 1.0, 0.0, 48).is_empty());
        assert!(hold(&mut s, 0.0, 0.0, 100).is_empty());
    }

    #[test]
    fn it_lets_go_lower_than_it_pushes() {
        let mut s = Stick::default();
        assert_eq!(hold(&mut s, 0.0, -0.8, 200), vec![("up", true)]);
        // Easing off to under the push line but over the let-go line still holds.
        assert!(hold(&mut s, 0.0, -0.4, 200).is_empty());
        assert_eq!(hold(&mut s, 0.0, -0.3, 50), vec![("up", false)]);
        // And that far isn't enough to push again.
        assert!(hold(&mut s, 0.0, -0.45, 300).is_empty());
    }

    #[test]
    fn a_thumb_near_the_corner_keeps_its_direction_and_a_turn_changes_it() {
        let mut s = Stick::default();
        assert_eq!(hold(&mut s, 0.1, -0.9, 200), vec![("up", true)]);
        // Just past the diagonal: still up.
        assert!(hold(&mut s, 0.75, -0.65, 300).is_empty());
        // Round to the right: up is let go, right is pressed, once.
        assert_eq!(hold(&mut s, 0.95, -0.2, 300), vec![("up", false), ("right", true)]);
    }

    #[test]
    fn pushed_while_away_does_nothing_until_it_was_in_the_middle() {
        let mut s = Stick::default();
        assert_eq!(hold(&mut s, 0.0, 0.9, 200), vec![("down", true)]);
        assert_eq!(s.let_go(), Some("down"));
        assert!(hold(&mut s, 0.0, 0.9, 500).is_empty());
        assert!(hold(&mut s, 0.0, 0.0, 50).is_empty());
        assert_eq!(hold(&mut s, 0.0, 0.9, 200), vec![("down", true)]);
    }
}
