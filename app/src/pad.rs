//! Controllers through SDL. In Game Mode, Steam hands non-Steam apps a virtual Xbox pad, the same path games use;
//! SDL's controller mappings cover Xbox pads and the handhelds' built-in controls the same way. Buttons, the triggers
//! and the left stick pushed one way all become plain presses ("a", "lt", "up").

use sdl2::controller::{Axis, Button, GameController};
use sdl2::event::Event;
use sdl2::{EventPump, GameControllerSubsystem, Sdl};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How far a trigger or the stick must move to count as a press, and where it lets go again (so it never flickers).
const PRESS_AT: i16 = 20_000;
const RELEASE_AT: i16 = 12_000;
/// The same tap seen on two devices (Steam's virtual pad and the raw one) arrives within this many milliseconds.
const TWIN_MS: u128 = 40;

pub struct Pads {
    sdl: Option<Sdl>,
    subsystem: Option<GameControllerSubsystem>,
    pump: Option<EventPump>,
    open: Vec<GameController>,
    /// Which pad holds each button down right now.
    held: HashMap<&'static str, u32>,
    /// When each button was last let go, and on which pad.
    last_up: HashMap<&'static str, (u32, Instant)>,
    /// Triggers and stick directions currently counted as pressed, per pad.
    axis_down: HashMap<(u32, &'static str), bool>,
    last_rescan: Instant,
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

impl Pads {
    /// Opens SDL's controller support. If SDL isn't available the app still runs, with keyboard, mouse and touch.
    pub fn open() -> Self {
        sdl2::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
        // SDL would catch SIGTERM and SIGINT and turn them into a quit event nobody reads, so Steam, a logout or a
        // shutdown couldn't close the app. Qt and the system keep their usual handling.
        sdl2::hint::set("SDL_NO_SIGNAL_HANDLERS", "1");
        let sdl = sdl2::init().ok();
        let subsystem = sdl.as_ref().and_then(|s| s.game_controller().ok());
        let pump = sdl.as_ref().and_then(|s| s.event_pump().ok());
        let mut pads = Pads { sdl, subsystem, pump, open: Vec::new(), held: HashMap::new(), last_up: HashMap::new(), axis_down: HashMap::new(), last_rescan: Instant::now() };
        pads.enumerate();
        pads
    }

    fn enumerate(&mut self) {
        let Some(sub) = self.subsystem.as_ref() else { return };
        for i in 0..sub.num_joysticks().unwrap_or(0) {
            if sub.is_game_controller(i)
                && let Ok(c) = sub.open(i)
                && !self.open.iter().any(|o| o.instance_id() == c.instance_id())
            {
                self.open.push(c);
            }
        }
    }

    /// With no controller open, asks SDL again from scratch (at most every `every`): after leaving Game Mode the
    /// controller can come back without SDL hearing it arrive.
    pub fn rescan(&mut self, every: Duration) {
        if !self.open.is_empty() || self.sdl.is_none() || self.last_rescan.elapsed() < every {
            return;
        }
        self.last_rescan = Instant::now();
        // Letting go of the only handle shuts SDL's controller part down; taking it again lists the devices afresh.
        self.subsystem = None;
        self.subsystem = self.sdl.as_ref().and_then(|s| s.game_controller().ok());
        self.enumerate();
    }

    /// Everything that happened since the last call, as (button, pressed) pairs.
    pub fn poll(&mut self) -> Vec<(&'static str, bool)> {
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

/// The triggers and the left stick act as buttons. Yields (name, strength), 0 when idle.
fn axis_names(axis: Axis, value: i16) -> Vec<(&'static str, i16)> {
    let pos = value.max(0);
    let neg = if value == i16::MIN { i16::MAX } else { (-value).max(0) };
    match axis {
        Axis::TriggerLeft => vec![("lt", pos)],
        Axis::TriggerRight => vec![("rt", pos)],
        Axis::LeftX => vec![("right", pos), ("left", neg)],
        Axis::LeftY => vec![("down", pos), ("up", neg)],
        _ => vec![],
    }
}
