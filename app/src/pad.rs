//! Controllers through SDL. In Game Mode, Steam hands non-Steam apps a virtual Xbox pad, the same path games use;
//! SDL's controller mappings cover Xbox pads and the handhelds' built-in controls the same way. Buttons and the
//! triggers become plain presses ("a", "lt", "r3"); the sticks are kept as places for the controller pointer
//! (pointer.rs): the right stick moves it, the left stick is the D-pad when pushed and a wheel when swept.
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
    /// Each pad's sticks: left x, left y, right x, right y (SDL's -32768..32767, y down).
    sticks: HashMap<u32, [i16; 4]>,
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
        let mut pads = Pads { sdl, subsystem, pump, open: Vec::new(), held: HashMap::new(), last_up: HashMap::new(), axis_down: HashMap::new(), sticks: HashMap::new(), last_rescan: now, gate: Gate::new(), lost: false };
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
        ups
    }

    /// Where the sticks are now, each -1 to 1 with y down. With more than one pad (Steam's virtual pad and the raw
    /// one report the same stick), each stick is taken from the pad that has it pushed furthest.
    pub fn sticks(&self) -> crate::pointer::Sticks {
        let mut s = crate::pointer::Sticks::default();
        for v in self.sticks.values() {
            let (lx, ly, rx, ry) = (crate::pointer::unit(v[0]), crate::pointer::unit(v[1]), crate::pointer::unit(v[2]), crate::pointer::unit(v[3]));
            if lx.hypot(ly) > s.lx.hypot(s.ly) {
                (s.lx, s.ly) = (lx, ly);
            }
            if rx.hypot(ry) > s.rx.hypot(s.ry) {
                (s.rx, s.ry) = (rx, ry);
            }
        }
        s
    }

    /// A direction the left stick pressed or let go (pointer.rs decides which): passed on like a button's, so the
    /// D-pad and the stick holding the same direction count once, and nothing is pressed behind another window.
    pub fn stick_event(&mut self, dir: &'static str, pressed: bool) -> Option<(&'static str, bool)> {
        let now = Instant::now();
        if !self.dedupe(STICK, dir, pressed, now) {
            return None;
        }
        self.gate.pass(vec![(dir, pressed)], now).pop()
    }

    /// Everything that happened since the last call, as (button, pressed) pairs. Behind another window, and for a
    /// moment after coming back, only the releases: nothing stays held down in the interface, and nothing is pressed.
    pub fn poll(&mut self) -> Vec<(&'static str, bool)> {
        let events = self.read();
        self.gate.pass(events, Instant::now())
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
                            Axis::LeftX => self.sticks.entry(which).or_insert([0; 4])[0] = value,
                            Axis::LeftY => self.sticks.entry(which).or_insert([0; 4])[1] = value,
                            Axis::RightX => self.sticks.entry(which).or_insert([0; 4])[2] = value,
                            Axis::RightY => self.sticks.entry(which).or_insert([0; 4])[3] = value,
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
        // The right stick pressed in: the pointer's click.
        Button::RightStick => "r3",
        // The Steam / Guide button belongs to Steam.
        _ => return None,
    })
}

/// The triggers act as buttons (the sticks are read as places instead, see `sticks`). Yields (name, strength),
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
}
