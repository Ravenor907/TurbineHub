//! `turbinehub --agent`: the background helper's side of the app. It only looks (which game runs, the battery, the
//! screens), runs the commands the engine hands it and brings the answers back. Every decision is in the engine's
//! `agent`, `lights`, `history` and `snapshots`.
//!
//! Inside the Flatpak the sandbox shows neither the host's processes nor a writable /sys, so the list of running
//! games comes from the host (`pgrep`) and the lights are written through one long-lived `tee` per file on the host.
//!
//!   turbinehub --agent                          run until stopped (the systemd user unit does this)
//!   turbinehub --agent --once                   one look at everything, printing what it would run instead of
//!                                               running it
//!   turbinehub --agent --once --machine=handheld|desktop|tv
//!                                               the same on a made-up machine in the middle of a game (the build's
//!                                               checks run this)
//!   turbinehub --agent --once --root=<dir> --home=<dir> --config=<dir> --data=<dir>
//!                                               the same on a machine made up by hand

use crate::backend::{cache_dir, data_dir, option, real_ctx};
use crate::run;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use turbinehub_engine::agent::{self, Agent, Do, Found, Paths};
use turbinehub_engine::fake::Preset;
use turbinehub_engine::hardware::{self, Machine};
use turbinehub_engine::lights::{self, Led, Shown};
use turbinehub_engine::settings::Settings;
use turbinehub_engine::snapshots::Snapshot;
use turbinehub_engine::ui::machine::Lighting;
use turbinehub_engine::{helper, now, sensors};

/// A wall clock this far ahead of the steady one since the last pass means the machine slept in between.
const SLEEP_GAP_SECS: u64 = 30;

/// How this run reads and acts.
struct Mode {
    paths: Paths,
    config_home: PathBuf,
    cache: PathBuf,
    /// Print what would be run instead of running it.
    dry: bool,
    /// A made-up machine under `paths.root`: its files are read and written directly.
    fake: bool,
    /// The folder of a machine made up for this run, removed at the end.
    made_up: Option<PathBuf>,
}

impl Mode {
    fn from_args() -> Mode {
        if let Some(kind) = option("machine") {
            let preset = match kind.as_str() {
                "desktop" => Preset::Desktop,
                "tv" => Preset::TvPc,
                _ => Preset::Handheld,
            };
            let dir = std::env::temp_dir().join(format!("turbinehub-agent-{}", std::process::id()));
            let (paths, config_home) = agent::sample(&dir, preset);
            return Mode { paths, config_home, cache: dir.join("cache"), dry: true, fake: true, made_up: Some(dir) };
        }
        let ctx = real_ctx();
        let root = option("root").map(PathBuf::from);
        let dir = |name: &str, real: PathBuf| option(name).map(PathBuf::from).unwrap_or(real);
        let config_home = dir("config", ctx.config_home.clone());
        Mode {
            paths: Paths { root: root.clone().unwrap_or(ctx.root), home: dir("home", ctx.home), folder: config_home.join("turbinehub"), data: dir("data", data_dir()) },
            config_home,
            cache: cache_dir(),
            dry: option("once").is_some(),
            fake: root.is_some(),
            made_up: None,
        }
    }

    /// Reads the host's files directly: a made-up machine, or a run outside the Flatpak.
    fn direct(&self) -> bool {
        self.fake || !run::in_flatpak()
    }

    fn host_file(&self, path: &str) -> Option<String> {
        if self.direct() {
            return std::fs::read_to_string(self.paths.root.join(path.trim_start_matches('/'))).ok();
        }
        run::host_text(&["cat", path])
    }

    /// The Steam app ids of the running games. None when the host couldn't be asked: that is never "no game".
    fn games(&self) -> Option<Vec<String>> {
        if self.direct() {
            return Some(agent::running_games(&agent::processes(&self.paths.root)));
        }
        // pgrep ends with 1 when nothing matches; anything else means it didn't run.
        let o = run::run(&agent::games_question(), None).ok().filter(|o| o.code == 0 || o.code == 1)?;
        Some(agent::running_games(&o.stdout))
    }

    fn machine(&self) -> Machine {
        Machine::read(&self.paths.root, "", &if self.fake { String::new() } else { hardware::device_of(&self.paths.home) })
    }
}

/// Work that takes a while, done off the main loop so the lights keep moving.
enum Job {
    Host(Vec<String>),
    Sinks,
    Verify,
    Snapshot { game: String, steam_id: String, name: String },
    Updates { test_builds: bool },
}

enum Answer {
    /// A command through the privileged helper that didn't work, and why.
    HostFailed(Vec<String>, String),
    Sinks(String, String),
    Verified(Option<Vec<String>>),
    Snapshot(String, Result<Option<Snapshot>, String>),
    Updates(Found),
}

struct Worker {
    data: PathBuf,
    home: PathBuf,
    cache: PathBuf,
}

impl Worker {
    fn work(&self, job: Job) -> Option<Answer> {
        let host = |line: &[String]| run::run(line, None).ok().map(|o| (o.code, o.stdout));
        match job {
            Job::Host(line) => {
                let o = run::run(&line, None).ok()?;
                let through_helper = line.iter().any(|a| a == helper::BIN);
                through_helper.then(|| run::helper_result(&o).err()).flatten().map(|why| Answer::HostFailed(line, why))
            }
            Job::Sinks => Some(Answer::Sinks(run::host_text(&["pactl", "list", "sinks"])?, run::host_text(&["pactl", "get-default-sink"])?)),
            Job::Verify => Some(Answer::Verified(run::verify())),
            Job::Snapshot { game, steam_id, name } => {
                let result = agent::take_snapshot(&self.data, &game, &steam_id, &name, now(), &mut |line: &[String]| host(line));
                Some(Answer::Snapshot(name, result))
            }
            Job::Updates { test_builds } => Some(Answer::Updates(run::look_for_updates(&self.home, &self.cache, crate::VERSION, test_builds).0)),
        }
    }
}

/// Starts a thread that does the jobs sent to it, one after the other.
fn worker(w: Worker, back: Sender<Answer>) -> Sender<Job> {
    let (tx, rx): (Sender<Job>, Receiver<Job>) = mpsc::channel();
    std::thread::spawn(move || {
        for job in rx {
            if let Some(a) = w.work(job) {
                let _ = back.send(a);
            }
        }
    });
    tx
}

/// Writes the lights' files. On a made-up machine and outside the Flatpak the files are written as they are; inside
/// it each file gets one `tee` on the host that stays open, and every frame is a line typed into it (the kernel
/// takes each write to these files as a new value).
struct LightWriter {
    direct: Option<PathBuf>,
    pipes: BTreeMap<String, ChildStdin>,
}

impl LightWriter {
    fn write(&mut self, path: &str, value: &str) {
        if let Some(root) = &self.direct {
            let _ = std::fs::write(root.join(path.trim_start_matches('/')), format!("{value}\n"));
            return;
        }
        if !self.pipes.contains_key(path) {
            let child = Command::new("flatpak-spawn").args(["--host", "tee", path]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
            let Some(pipe) = child.ok().and_then(|mut c| c.stdin.take()) else { return };
            self.pipes.insert(path.to_string(), pipe);
        }
        let sent = self.pipes.get_mut(path).is_some_and(|p| p.write_all(format!("{value}\n").as_bytes()).and_then(|_| p.flush()).is_ok());
        if !sent {
            // The tee is gone (the light went away, or the session's helper restarted): a new one next time.
            self.pipes.remove(path);
        }
    }
}

/// Runs something every so many seconds.
struct Every(u64, u64);

impl Every {
    fn due(&mut self, now: u64) -> bool {
        let due = now < self.1 || now - self.1 >= self.0;
        if due {
            self.1 = now;
        }
        due
    }
}

pub fn main() -> i32 {
    let mode = Mode::from_args();
    // One background helper at a time: a second one would fight the first over the lights and the processor.
    let lock_file = mode.paths.data.join("agent/lock");
    let _ = std::fs::create_dir_all(mode.paths.data.join("agent"));
    let lock = File::options().create(true).truncate(false).write(true).open(&lock_file);
    let Ok(lock) = lock else {
        eprintln!("Couldn't open {}", lock_file.display());
        return 1;
    };
    if lock.try_lock().is_err() {
        eprintln!("The background helper is running already.");
        return if mode.dry { 1 } else { 0 };
    }

    let mut m = mode.machine();
    let mut core = Agent::start(mode.paths.clone(), crate::VERSION, &m);
    let (back, answers) = mpsc::channel();
    let w = || Worker { data: mode.paths.data.clone(), home: mode.paths.home.clone(), cache: mode.cache.clone() };
    // Quick things (the processor setting, the sound output) never wait behind a snapshot or an update check.
    let (quick, slow) = (worker(w(), back.clone()), worker(w(), back));
    let mut lights_out = LightWriter { direct: mode.direct().then(|| mode.paths.root.clone()), pipes: BTreeMap::new() };
    let mut shown = Shown::default();
    let mut leds: Vec<Led> = Vec::new();

    let read_count = |mode: &Mode| mode.host_file(agent::SUSPEND_COUNT).as_deref().and_then(agent::suspend_count);
    core.woke(read_count(&mode));

    let started = Instant::now();
    let (mut last_wall, mut last_steady) = (now(), 0u64);
    let (mut every_battery, mut every_games, mut every_screens, mut every_minute, mut every_machine, mut every_second) = (Every(15, 0), Every(4, 0), Every(3, 0), Every(60, 0), Every(600, 0), Every(1, 0));
    let mut lighting = Lighting::load(&mode.paths.folder);
    let mut lights_on = false;
    let mut passes = 0;

    loop {
        let wall = now();
        let steady = started.elapsed().as_secs();
        // The steady clock stands still while the machine sleeps and the wall clock doesn't.
        let away = wall.saturating_sub(last_wall);
        let slept = away >= steady.saturating_sub(last_steady) + SLEEP_GAP_SECS;
        (last_wall, last_steady) = (wall, steady);
        let mut todo: Vec<Do> = Vec::new();

        if every_second.due(wall) || slept {
            let st = Settings::load(&mode.config_home);
            lighting = Lighting::load(&mode.paths.folder);
            let helper_there = helper::installed(&mode.paths.root);
            if every_machine.due(wall) || slept {
                m = mode.machine();
                leds = lights::leds(&mode.paths.root, &m.rgb_leds);
            }
            if slept && core.woke(read_count(&mode)) {
                let r = sensors::read(&mode.paths.root);
                core.slept(wall, away, &r, agent::wake_counts(&mode.paths.root, &m));
                // Lights often come back from a sleep in their own state: every file is written again.
                shown.forget();
                every_battery.1 = wall;
            } else if every_battery.due(wall) {
                core.battery(wall, &sensors::read(&mode.paths.root), agent::wake_counts(&mode.paths.root, &m));
            }
            // While a game runs the only news is its end, so the host is asked less often.
            every_games.0 = if core.game().is_empty() { 4 } else { 10 };
            if every_games.due(wall)
                && let Some(ids) = mode.games()
            {
                todo.extend(core.games(wall, &ids, &st, helper_there));
            }
            if every_screens.due(wall) {
                todo.extend(core.screens(wall, agent::external_screen(&mode.paths.root), &lighting));
            }
            if every_minute.due(wall) {
                todo.extend(core.steamos(wall, &agent::steamos(&mode.paths.root), helper_there));
                todo.extend(core.daily(wall, &st));
                core.heartbeat(wall);
            }
            // The lights are driven once the player has set them up: their own choices saved, and the files open
            // to this user (the helper's lighting access rule).
            lights_on = mode.paths.folder.join("lighting.json").is_file() && !leds.is_empty() && leds.iter().all(|l| lights::writable(&mode.paths.root, l));
        }

        while let Ok(a) = answers.try_recv() {
            match a {
                Answer::HostFailed(line, why) => core.host_failed(wall, &line, &why),
                Answer::Sinks(list, default) => todo.extend(core.sinks_listed(wall, &list, &default)),
                Answer::Verified(drift) => core.verified(wall, drift),
                Answer::Snapshot(name, result) => core.snapshot_taken(wall, &name, &result),
                Answer::Updates(found) => core.updates_found(wall, found),
            }
        }

        for d in todo {
            if mode.dry || mode.fake {
                match &d {
                    Do::Host(line) => println!("would run: {}", line.join(" ")),
                    other => println!("would do: {other:?}"),
                }
                continue;
            }
            let _ = match d {
                Do::Host(line) => quick.send(Job::Host(line)),
                Do::Sinks => quick.send(Job::Sinks),
                Do::Verify => quick.send(Job::Verify),
                Do::Snapshot { game, steam_id, name } => slow.send(Job::Snapshot { game, steam_id, name }),
                Do::CheckUpdates { test_builds } => slow.send(Job::Updates { test_builds }),
            };
        }

        let scene = core.scene(&lighting);
        if lights_on {
            for (path, value) in shown.changed(lights::frame(&scene, &leds, started.elapsed().as_millis() as u64)) {
                if mode.dry && !mode.fake {
                    println!("would write: {path} = {value}");
                } else {
                    lights_out.write(&path, &value);
                }
            }
        }

        passes += 1;
        if mode.dry {
            if passes >= 2 {
                println!("status: {}", std::fs::read_to_string(mode.paths.data.join("agent/status.json")).unwrap_or_default().trim());
                for led in &leds {
                    let file = |f: &str| std::fs::read_to_string(mode.paths.root.join("sys/class/leds").join(&led.name).join(f)).unwrap_or_default().trim().to_string();
                    if mode.fake {
                        println!("light: {} shows {} at {}", led.name, file("multi_intensity"), file("brightness"));
                    }
                }
                if let Some(dir) = &mode.made_up {
                    let _ = std::fs::remove_dir_all(dir);
                }
                return 0;
            }
            continue;
        }
        std::thread::sleep(Duration::from_millis(if lights_on { lights::frame_ms(&scene) } else { 1000 }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("turbinehub-agent-app-{tag}-{}", std::process::id()))
    }

    #[test]
    fn lights_are_written_as_they_are_on_a_made_up_machine() {
        let root = test_dir("lights");
        let led = root.join("sys/class/leds/a:rgb:b");
        std::fs::create_dir_all(&led).unwrap();
        let mut w = LightWriter { direct: Some(root.clone()), pipes: BTreeMap::new() };
        w.write("/sys/class/leds/a:rgb:b/brightness", "128");
        assert_eq!(std::fs::read_to_string(led.join("brightness")).unwrap(), "128\n");
        assert!(w.pipes.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn timers_fire_at_once_then_at_their_pace_and_survive_a_clock_set_back() {
        let mut e = Every(15, 0);
        assert!(e.due(1000));
        assert!(!e.due(1014));
        assert!(e.due(1015));
        assert!(e.due(500));
        assert!(!e.due(510));
    }

    #[test]
    fn a_made_up_machine_is_never_asked_through_the_host() {
        let dir = test_dir("mode");
        let p = Paths { root: dir.join("root"), home: dir.join("home"), folder: dir.join("config/turbinehub"), data: dir.join("data") };
        let mode = Mode { paths: p.clone(), config_home: dir.join("config"), cache: dir.join("cache"), dry: true, fake: true, made_up: None };
        assert!(mode.direct());
        std::fs::create_dir_all(p.root.join("proc/77")).unwrap();
        std::fs::write(p.root.join("proc/77/cmdline"), "/x/reaper\0SteamLaunch\0AppId=1145360\0--\0game\0").unwrap();
        std::fs::create_dir_all(p.root.join("sys/power/suspend_stats")).unwrap();
        std::fs::write(p.root.join("sys/power/suspend_stats/success"), "4\n").unwrap();
        assert_eq!(mode.games(), Some(vec!["1145360".to_string()]));
        assert_eq!(mode.host_file(agent::SUSPEND_COUNT).as_deref().and_then(agent::suspend_count), Some(4));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
