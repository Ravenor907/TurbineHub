//! What runs off the interface thread: the steps of a confirmed dialog, and the facts only the host or the internet
//! can give (installed apps, updates, the password state, sound outputs, ratings). Nothing here decides anything:
//! the engine built the steps, and the facts go back to it as they are.

use std::io::{Read, Write};
use std::time::{Duration, Instant};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use turbinehub_engine::agent::{self, Found};
use turbinehub_engine::json::Json;
use turbinehub_engine::net::Client;
use turbinehub_engine::selfupdate::{self, Check};
use turbinehub_engine::shortcuts::Steam;
use turbinehub_engine::sources::{self, Origin};
use turbinehub_engine::ui::{self, Step};
use turbinehub_engine::{Store, helper};

/// What a worker sends back to the interface thread.
pub enum Event {
    /// What the job is doing now, for the busy line.
    Progress(String),
    /// The job ended: Ok, or the first step's problem in plain words.
    Done(Result<(), String>),
    Facts(Box<Facts>),
}

/// Facts gathered from the host and the internet.
#[derive(Default)]
pub struct Facts {
    /// These came from the host itself (the plain fields below are set), not from the internet.
    pub local: bool,
    pub installed: Option<Vec<String>>,
    pub updates: Option<Vec<String>>,
    pub has_password: Option<bool>,
    /// Is Steam running: yes, no, or the host couldn't say (then it counts as running).
    pub steam: Option<Steam>,
    /// When the running Steam started, in seconds since 1970.
    pub steam_started: Option<u64>,
    pub sound: Vec<(String, String)>,
    pub sound_now: String,
    pub free_bytes: Option<u64>,
    pub efibootmgr: Option<String>,
    /// What the helper's check found, when the helper is there and answered.
    pub drift: Option<Vec<String>>,
    /// Whether the background helper is set to start with the session, when the host could say.
    pub agent_on: Option<bool>,
    /// Some(true) when the internet answered, Some(false) when it didn't, None when it wasn't asked.
    pub online: Option<bool>,
    pub decky_tag: Option<String>,
    pub nsl_tag: Option<String>,
    /// Some(version) when a newer Turbine Hub is out, Some("") when this one is the newest.
    pub update: Option<String>,
    /// Decky plugins with a newer version, when the plugin store's list could be read.
    pub plugins: Option<Vec<String>>,
    /// What Desktop Mode has each screen set to, when it could be asked (not in Game Mode).
    pub screens: Option<Vec<turbinehub_engine::checkup::ScreenNow>>,
    /// What the host says about the lights (Ally Hub's saved choice, who else sets them, the helper's rule).
    pub lights: Option<ui::lighting::HostLights>,
}

pub fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Running inside Game Mode (gamescope) rather than the desktop.
pub fn in_game_mode() -> bool {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    var("XDG_CURRENT_DESKTOP").to_ascii_lowercase().contains("gamescope") || !var("GAMESCOPE_WAYLAND_DISPLAY").is_empty() || var("SteamGamepadUI") == "1"
}

/// The engine's command lines start with flatpak-spawn --host, as the Flatpak needs. Outside one (a development
/// run) the program is started directly.
fn command(line: &[String]) -> Option<Command> {
    let line = if !in_flatpak() && line.len() > 2 && line[0] == "flatpak-spawn" && line[1] == "--host" { &line[2..] } else { line };
    let mut c = Command::new(line.first()?);
    c.args(&line[1..]);
    Some(c)
}

pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// How long a command may take before it is stopped: installs and downloads get half an hour, everything else two
/// minutes. Nothing the app runs can leave it waiting for ever.
fn time_limit(line: &[String]) -> Duration {
    let has = |w: &str| line.iter().any(|a| a == w);
    // Downloads only: Flathub installs and updates, removing an app, Decky Loader. Setting up Turbine Hub's own
    // helper downloads nothing, so it gets the short limit like every other system change.
    let long = (has("flatpak") && (has("install") || has("update") || has("uninstall"))) || has("decky-install");
    Duration::from_secs(if long { 30 * 60 } else { 120 })
}

/// Runs one command line to its end, with `stdin` typed into it when given. Stopped (TERM, then KILL) when it runs
/// past its time limit. The helper runs as root, so nothing here can stop it: the app only stops waiting for it.
/// That is why the helper keeps its own, shorter limits (its wait for the lock and every program it starts), and
/// ends by itself well inside this one.
pub fn run(line: &[String], stdin: Option<&str>) -> Result<Output, String> {
    let limit = time_limit(line);
    let mut c = command(line).ok_or("Nothing to run")?;
    c.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| format!("Couldn't start {}: {e}", line.first().map(String::as_str).unwrap_or("")))?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(text.as_bytes());
    }
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let out = read(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = read(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let start = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(st) => break Some(st),
            None if start.elapsed() > limit => break None,
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let Some(status) = status else {
        // flatpak-spawn passes TERM on to the host command; KILL would only end flatpak-spawn itself.
        let _ = Command::new("kill").arg("-TERM").arg(child.id().to_string()).status();
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(5) && matches!(child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
        return Err("It took too long, so Turbine Hub stopped it and skipped the rest.".into());
    };
    Ok(Output { code: status.code().unwrap_or(-1), stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() })
}

fn host(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host"].iter().chain(args).map(|x| x.to_string()).collect()
}

/// A host command's output when it ran and ended well.
pub fn host_text(args: &[&str]) -> Option<String> {
    run(&host(args), None).ok().filter(|o| o.code == 0).map(|o| o.stdout)
}

/// The last line a failed command printed, as the reason.
fn reason(o: &Output) -> String {
    let last = |text: &str| text.lines().map(str::trim).rfind(|l| !l.is_empty()).map(String::from);
    last(&o.stderr).or_else(|| last(&o.stdout)).unwrap_or_else(|| "It didn't work.".into())
}

/// A helper job's result: its own answer, or what sudo or pkexec said when it never ran.
pub fn helper_result(o: &Output) -> Result<(), String> {
    if let Some(problem) = helper::launch_problem(&o.stderr, o.code) {
        return Err(problem);
    }
    helper::answer(&o.stdout).map(|_| ())
}

/// The helper inside the installed Flatpak, as the host sees it.
fn helper_in_flatpak() -> Option<PathBuf> {
    let info = std::fs::read_to_string("/.flatpak-info").ok()?;
    let app = info.lines().find_map(|l| l.trim().strip_prefix("app-path="))?;
    Some(Path::new(app.trim()).join("bin/turbinehub-helper"))
}

/// Where Turbine Hub's shared files are (fonts, pictures, the Steam art set).
pub fn share_dir() -> PathBuf {
    std::env::var("TURBINEHUB_SHARE").unwrap_or_else(|_| "/app/share/turbinehub".into()).into()
}

/// What the steps of a plan need besides themselves.
pub struct Job {
    pub steps: Vec<Step>,
    pub password: String,
    pub home: PathBuf,
    /// Turbine Hub's own folder (the games file, the launch folder).
    pub folder: PathBuf,
    /// Turbine Hub's data folder (the save snapshots).
    pub data: PathBuf,
    pub version: String,
    pub cache: PathBuf,
    pub test_builds: bool,
}

/// Is Steam running on the host, asked now. Every step that writes Steam's list of non-Steam games asks right
/// before it writes: Steam may have been started since the dialog was shown, and a question that couldn't be asked
/// counts as "running" (the engine refuses to write then).
pub fn steam_now() -> Steam {
    let ran = run(&Steam::question(), None).ok();
    let steam = Steam::from_answer(ran.as_ref().map(|o| (o.code, o.stdout.as_str())));
    if steam == Steam::Unknown {
        crate::log::line("steam", &format!("Couldn't tell whether Steam is running ({}): treated as running", ran.map(|o| format!("pgrep ended with {}: {}", o.code, reason(&o))).unwrap_or_else(|| "pgrep couldn't be started".into())));
    }
    steam
}

/// A step's name for the log: what it is, never what was typed.
fn step_name(st: &Step) -> String {
    match st {
        Step::Host(line) => format!("Host {}", line.iter().skip_while(|a| a.as_str() == "flatpak-spawn" || a.as_str() == "--host").cloned().collect::<Vec<_>>().join(" ")),
        Step::HostWrite { path, .. } => format!("HostWrite {path}"),
        Step::Tile { name, .. } => format!("Tile {name}"),
        Step::TileRemove { url } => format!("TileRemove {url}"),
        Step::CopyGame { name, .. } => format!("CopyGame {name}"),
        Step::Report { game } => format!("Report {game}"),
        Step::Nsl { tag, plugin } => format!("Nsl {tag} plugin={plugin}"),
        Step::ResetPrefix { app_id, .. } => format!("ResetPrefix {app_id}"),
        Step::ResetUndo { app_id, .. } => format!("ResetUndo {app_id}"),
        Step::ProtonPick { app_id, .. } => format!("ProtonPick {app_id}"),
        Step::ProtonUndo { app_id } => format!("ProtonUndo {app_id}"),
        Step::Saves(_) => "Saves".into(),
        Step::Storage(turbinehub_engine::storage::Job::Clean(items)) => format!("Storage clean ({} things)", items.len()),
        Step::Storage(turbinehub_engine::storage::Job::Undo) => "Storage undo".into(),
        other => format!("{other:?}"),
    }
}

/// One step, with its start and how it ended in the log.
fn step(job: &Job, st: &Step, say: &dyn Fn(String)) -> Result<(), String> {
    let name = step_name(st);
    let result = step_inner(job, st, say);
    match &result {
        Ok(()) => crate::log::line("step", &format!("{name}: done")),
        Err(why) => crate::log::line("step", &format!("{name}: failed: {why}")),
    }
    result
}

fn step_inner(job: &Job, st: &Step, say: &dyn Fn(String)) -> Result<(), String> {
    match st {
        Step::Host(line) => {
            let is_helper = line.iter().any(|a| a == helper::BIN);
            if line.iter().any(|a| a == "flathub") {
                say(format!("Installing {}", line.last().map(String::as_str).unwrap_or("")));
            }
            let o = run(line, None)?;
            if is_helper {
                helper_result(&o)
            } else if o.code == 0 {
                Ok(())
            } else {
                Err(reason(&o))
            }
        }
        Step::HelperInstall => {
            say("Setting up Turbine Hub's helper".into());
            let path = helper_in_flatpak().ok_or("The helper is only there when Turbine Hub runs as its Flatpak.")?;
            let o = run(&helper::install(&path), Some(&format!("{}\n", job.password)))?;
            helper_result(&o)
        }
        Step::SetPassword => {
            let o = run(&host(&["passwd"]), Some(&format!("{0}\n{0}\n", job.password)))?;
            if o.code == 0 { Ok(()) } else { Err(reason(&o)) }
        }
        Step::HostWrite { path, value } => {
            let o = run(&host(&["tee", path]), Some(value))?;
            if o.code == 0 { Ok(()) } else { Err(reason(&o)) }
        }
        Step::Tile { name, url } => {
            let file = turbinehub_engine::shortcuts::tile_file(&job.home).ok_or("Steam's list of non-Steam games wasn't found. Start Steam once and sign in.")?;
            turbinehub_engine::shortcuts::add_shortcut(&file, &ui::store::tile_shortcut(name, url), &ui::store::tile_marker(url), steam_now()).map(|_| ())
        }
        Step::CopyGame { name, pairs } => {
            say(format!("Copying {name}"));
            for (from, to) in pairs {
                ui::bring::copy_tree(from, to).map_err(|e| format!("Couldn't copy {name}: {e}"))?;
            }
            Ok(())
        }
        Step::ResetPrefix { .. } | Step::ResetUndo { .. } => ui::games::reset_step(st, &job.folder, turbinehub_engine::now()).unwrap_or(Ok(())),
        Step::Tool { .. } | Step::ProtonPick { .. } | Step::ProtonUndo { .. } => ui::launch::run_step(st, &job.home, &job.folder).unwrap_or(Ok(())),
        Step::Saves(work) => {
            let mut runner = |line: &[String]| run(line, None).map(|o| turbinehub_engine::saves::Ran { code: o.code, stdout: o.stdout, stderr: o.stderr });
            let places = turbinehub_engine::saves::Places { folder: &job.folder, data: &job.data, home: &job.home };
            turbinehub_engine::saves::run(work, &places, turbinehub_engine::now(), say, &mut runner).map(|_| ())
        }
        Step::Storage(work) => {
            // Steam and the running games are asked on the host right before anything is touched.
            let mut ask = |line: &[String]| run(line, None).ok().map(|o| (o.code, o.stdout));
            let done = turbinehub_engine::storage::run_here(work, &job.home, &job.folder, &job.data, say, &mut ask)?;
            crate::log::line("storage", &done);
            Ok(())
        }
        Step::SelfTile { .. } | Step::SelfArt | Step::FilesTile { .. } => {
            // Only the entry itself is in Steam's list; the pictures go beside it and need no answer.
            let steam = if matches!(st, Step::SelfTile { .. } | Step::FilesTile { .. }) { steam_now() } else { Steam::Closed };
            ui::selftile::run_step(st, &job.home, &job.folder, &share_dir(), steam).unwrap_or(Ok(()))
        }
        Step::TileRemove { url } => {
            let file = turbinehub_engine::shortcuts::tile_file(&job.home).ok_or("Steam's list of non-Steam games wasn't found.")?;
            // The tile is the browser's entry with this address: Turbine Hub's own entry never matches.
            match turbinehub_engine::shortcuts::remove_app_tile(&file, ui::store::CHROME, &ui::store::tile_marker(url), steam_now())? {
                turbinehub_engine::shortcuts::TileChange::NotThere => Err("That tile isn't in Steam's list any more. If it still shows, remove it in Steam: its options, Manage, Remove non-Steam game.".into()),
                _ => Ok(()),
            }
        }
        Step::SelfUpdate => {
            say("Getting the new version".into());
            let client = Client::new(&job.version, &job.cache);
            let release = match selfupdate::check(&client, &job.version, job.test_builds).map_err(|e| e.to_string())? {
                Check::Available(r) => r,
                Check::UpToDate => return Err("This is the newest version already.".into()),
            };
            let got = selfupdate::download(&client, &release, None).map_err(|e| e.to_string())?;
            say("Installing the new version".into());
            let scope = selfupdate::install_scope(&std::fs::read_to_string("/.flatpak-info").unwrap_or_default());
            let o = run(&selfupdate::install_args_to(&got.path, scope), None)?;
            let _ = std::fs::remove_file(&got.path);
            if o.code != 0 {
                // Flatpak's own words go into the log; the player gets what to do.
                crate::log::line("update", &format!("Installing {} failed ({}): {} {}", release.version, o.code, o.stderr.trim(), o.stdout.trim()));
                return Err(selfupdate::install_problem(&format!("{}\n{}", o.stdout, o.stderr)));
            }
            crate::log::line("update", &format!("Installed Turbine Hub {} ({scope})", release.version));
            // The desktop's menu keeps the entry and icon from before until its list is rebuilt (Desktop Mode's
            // own tool; nothing happens where it isn't there).
            if selfupdate::menu_refresh().iter().any(|line| run(line, None).is_ok_and(|o| o.code == 0)) {
                crate::log::line("update", "The desktop's menu was asked to read Turbine Hub's entry and icon again");
            }
            Ok(())
        }
        Step::ProtonGet { .. } | Step::ProtonRemove { .. } | Step::TileArt | Step::TileArtUndo => {
            // Only the pictures touch Steam's library: they ask whether Steam runs right before they write.
            let steam = if matches!(st, Step::TileArt | Step::TileArtUndo) { steam_now() } else { Steam::Closed };
            ui::onepress::run_step(st, &job.home, &job.folder, &job.cache, &Client::new(&job.version, &job.cache), steam, say).unwrap_or(Ok(()))
        }
        Step::Nsl { tag, plugin } => {
            say("Getting NonSteamLaunchers".into());
            start_nsl(job, tag, *plugin)
        }
        // These need the engine's state: the interface thread does them once the rest worked.
        Step::Report { .. } | Step::SaveSetup | Step::Done { .. } | Step::SetupFinished { .. } | Step::Choice { .. } => Ok(()),
        // The helper just opened the lights: are they open now? Says what isn't, and why, when not.
        Step::LightsCheck { need, exact } => ui::lighting::check_open(Path::new("/"), &job.folder, need, *exact),
    }
}

/// Fetches NonSteamLaunchers' script (or its plugin's installer) at the release tag into the cache folder, which the
/// host sees at the same path, checks it is that script, and starts it on the host without waiting: the player
/// works in its own window from here on. Its output goes to its own log.
fn start_nsl(job: &Job, tag: &str, plugin: bool) -> Result<(), String> {
    use turbinehub_engine::nsl;
    if !nsl::tag_ok(tag) {
        return Err("That isn't a NonSteamLaunchers release.".into());
    }
    let client = Client::new(&job.version, &job.cache);
    let got = client.download(&nsl::script_url(tag, plugin), &turbinehub_engine::net::Expect::up_to(4 << 20), None).map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&got.path).unwrap_or_default();
    let _ = std::fs::remove_file(&got.path);
    if !nsl::script_ok(&text, tag, plugin) {
        return Err("What GitHub sent isn't NonSteamLaunchers' script. Nothing was started.".into());
    }
    let file = nsl::script_file(&job.cache, tag, plugin);
    // Only the newest script is kept.
    if let Some(dir) = file.parent() {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, text).map_err(|e| format!("Couldn't keep the script: {e}"))?;
    let mut c = command(&nsl::run_line(&file)).ok_or("Nothing to run")?;
    let mut child = c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("Couldn't start NonSteamLaunchers: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    if !plugin {
        let _ = nsl::remember(&job.folder, tag);
    }
    Ok(())
}

/// Runs the steps in order on a new thread, stopping at the first that fails.
pub fn start(job: Job, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let say = |words: String| {
            let _ = tx.send(Event::Progress(words));
        };
        let result = job.steps.iter().try_for_each(|st| step(&job, st, &say));
        let _ = tx.send(Event::Done(result));
    });
}

/// One harmless host command the engine asks for without a dialog (the sound output).
pub fn quick(line: Vec<String>) {
    std::thread::spawn(move || {
        let _ = run(&line, None);
    });
}

fn lines(text: Option<String>) -> Option<Vec<String>> {
    text.map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

/// What the helper's `verify` says differs between its record and the machine. None when it couldn't be asked.
pub fn verify() -> Option<Vec<String>> {
    let o = run(&helper::job("verify", None), None).ok()?;
    let j = helper::answer(&o.stdout).ok()?;
    Some(j.get("drift").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect())
}

/// Looks for updates: a newer Turbine Hub, Flathub apps, Decky plugins. Each part that couldn't be checked stays
/// None. Also answers whether the internet was reached (Some(false): tried and nothing answered). `ask_anyway`:
/// the player pressed Check now, or the background helper's daily look; without it GitHub is asked about Turbine
/// Hub itself at most once in six hours, and the answer from then is used in between.
pub fn look_for_updates(home: &Path, cache: &Path, version: &str, test_builds: bool, ask_anyway: bool) -> (Found, Option<bool>) {
    let client = Client::new(version, cache);
    let mut found = Found { apps: lines(host_text(&["flatpak", "remote-ls", "--updates", "--app", "--columns=application"])), ..Found::default() };
    let (mut worked, mut failed) = (false, false);
    let kept = selfupdate::remembered_file(cache);
    match selfupdate::check_kept(&kept, version, test_builds, turbinehub_engine::now(), ask_anyway, &mut || selfupdate::newest(&client, test_builds)) {
        Ok((check, asked)) => {
            found.update = Some(match check {
                Check::Available(r) => r.version.clone(),
                Check::UpToDate => String::new(),
            });
            // Only an answer from GitHub itself says the internet is there.
            worked |= asked;
            crate::log::line("update", if asked { "Asked GitHub for the newest Turbine Hub" } else { "The newest Turbine Hub: the answer from the last six hours is used" });
        }
        Err(e) => {
            failed = true;
            crate::log::line("update", &format!("Couldn't ask for the newest Turbine Hub: {e}"));
        }
    }
    // Only with Decky there: the plugin store is asked for nothing otherwise.
    if home.join("homebrew/plugins").is_dir() {
        let got = sources::get(&sources::DECKY_PLUGINS, cache, turbinehub_engine::now(), &mut |u| client.get_text(u, &[]).map_err(|e| e.to_string()));
        worked |= got.origin == Origin::Live;
        failed |= got.offline;
        if got.origin != Origin::Missing {
            found.plugins = Some(agent::plugin_updates(home, &got.text));
        }
    }
    (found, if worked { Some(true) } else if failed { Some(false) } else { None })
}

/// When the running Steam started (its oldest process, asked on the host). None without a running Steam.
fn steam_started() -> Option<u64> {
    let pid = host_text(&["pgrep", "-o", "-x", "steam"])?.trim().to_string();
    if pid.is_empty() || !pid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let secs: u64 = host_text(&["ps", "-o", "etimes=", "-p", &pid])?.trim().parse().ok()?;
    turbinehub_engine::now().checked_sub(secs + 1)
}

/// What the host can say without the internet. Quick, so screens fill in at once.
fn local_facts(home: &Path, cache: &Path, helper_there: bool) -> Facts {
    let mut f = Facts { local: true, installed: lines(host_text(&["flatpak", "list", "--app", "--columns=application"])), ..Facts::default() };
    // Each installed app's version and size, for the Store's pages.
    if let Some(text) = host_text(&["flatpak", "list", "--app", ui::store::INSTALLED_COLUMNS]) {
        ui::store::remember_installed(cache, &text);
    }
    f.has_password = host_text(&["passwd", "-S"]).and_then(|t| t.split_whitespace().nth(1).map(|w| w != "NP"));
    f.steam = Some(steam_now());
    f.steam_started = steam_started();
    f.sound = host_text(&["pactl", "list", "sinks"]).map(|t| agent::sinks(&t)).unwrap_or_default();
    f.sound_now = host_text(&["pactl", "get-default-sink"]).map(|t| t.trim().to_string()).unwrap_or_default();
    f.efibootmgr = host_text(&["efibootmgr"]);
    // Desktop Mode's display settings know each screen's refresh, VRR and HDR; Game Mode has no such settings.
    if !in_game_mode() {
        f.screens = host_text(&["kscreen-doctor", "-o"]).map(|t| turbinehub_engine::checkup::screens_from_kscreen(&t)).filter(|v| !v.is_empty());
    }
    let steam = home.join(".local/share/Steam");
    f.free_bytes = Command::new("df").args(["-B1", "--output=avail"]).arg(if steam.is_dir() { &steam } else { home }).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).lines().nth(1).and_then(|l| l.trim().parse().ok()));
    // Without the helper there is no record, so nothing can have drifted.
    f.drift = if helper_there { verify() } else { Some(Vec::new()) };
    // systemctl says "disabled", or nothing at all for a unit that isn't there, with a failing exit code: only its
    // word counts.
    f.agent_on = run(&agent::unit_question(), None).ok().map(|o| agent::unit_enabled(&o.stdout));
    f.lights = Some(host_lights(home));
    crate::log::line("host", &format!("Steam {:?}, {} apps installed, password {:?}, background helper on {:?}, {} settings drifted", f.steam.unwrap_or(Steam::Unknown), f.installed.as_ref().map_or(0, Vec::len), f.has_password, f.agent_on, f.drift.as_ref().map_or(0, Vec::len)));
    f
}

/// What the host says about the lights, from the engine's read-only questions: Ally Hub's settings file (the
/// sandbox can't see ~/.config), whether its agent and Decky Loader run, the helper's lighting rule and its unit.
/// Nothing here is ever written. systemctl answers with a failing exit code for anything but "active", so only
/// its word counts; a file that isn't there is simply no answer.
fn host_lights(home: &Path) -> ui::lighting::HostLights {
    let q = ui::lighting::host_questions(home);
    let ask = |i: usize| {
        let line: Vec<String> = ["flatpak-spawn", "--host"].iter().map(|x| x.to_string()).chain(q[i].iter().cloned()).collect();
        run(&line, None).ok()
    };
    let file = |i: usize| ask(i).filter(|o| o.code == 0).map(|o| o.stdout);
    let word = |i: usize| ask(i).map(|o| o.stdout).unwrap_or_default();
    ui::lighting::HostLights::from_host(file(0).as_deref(), &word(1), &word(2), file(3).as_deref(), &word(4))
}

/// Where a gathering reads and writes.
pub struct Gather {
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub cache: PathBuf,
    pub version: String,
    pub test_builds: bool,
    pub helper_there: bool,
    /// The player pressed Check now: Turbine Hub's own update is asked for even within six hours of the last time.
    pub check_now: bool,
}

/// Gathers the facts on a new thread: the host's first, then what needs the internet. Everything online goes
/// through the engine's sources, which keep the last good copy of each list with its age, so a screen never waits
/// for the internet and never loses what it showed.
pub fn gather(g: Gather, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let _ = tx.send(Event::Facts(Box::new(local_facts(&g.home, &g.cache, g.helper_there))));
        let client = Client::new(&g.version, &g.cache);
        let now = turbinehub_engine::now();
        // The newest version and the sizes of the Store's apps. No answer keeps what was found last.
        if let Some(text) = host_text(&["flatpak", "remote-ls", "flathub", "--app", ui::store::REMOTE_COLUMNS]) {
            ui::store::remember_remote(&g.cache, &text, now);
        }
        let mut fetch = |u: &str| client.get_text(u, &[]).map_err(|e| e.to_string());
        // The lists, then Valve's rating for the installed Steam games: at most 40 fetched in one go.
        let ids: Vec<String> = turbinehub_engine::scan_games(&g.home).games.into_iter().filter(|x| x.store == Store::Steam).map(|x| x.store_id).collect();
        let lists = sources::refresh(&g.cache, &ids, Some(&turbinehub_engine::cli::anticheat_cache(&g.config_home)), now, 40, &mut fetch);
        let decky = sources::get(&sources::DECKY_RELEASE, &g.cache, now, &mut fetch);
        let nsl = sources::get(&sources::NSL_RELEASE, &g.cache, now, &mut fetch);
        // The newest GE-Proton's name for the Proton page, and what players say about the installed Steam games on
        // ProtonDB (a few games at a time). Both only fill the cache folder.
        sources::get(&turbinehub_engine::proton_get::RELEASE, &g.cache, now, &mut fetch);
        turbinehub_engine::protondb::refresh(&g.cache, &ids, now, 25, &mut fetch);
        let (found, updates_online) = look_for_updates(&g.home, &g.cache, &g.version, g.test_builds, g.check_now);
        let answers = [lists.online, if decky.origin == Origin::Live { Some(true) } else if decky.offline { Some(false) } else { None }, updates_online];
        let f = Facts {
            updates: found.apps,
            update: found.update,
            plugins: found.plugins,
            decky_tag: sources::release_tag(&decky.text),
            nsl_tag: sources::release_tag(&nsl.text),
            online: if answers.contains(&Some(true)) { Some(true) } else if answers.contains(&Some(false)) { Some(false) } else { None },
            ..Facts::default()
        };
        let _ = tx.send(Event::Facts(Box::new(f)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_run_directly_outside_the_flatpak() {
        // Only meaningful where the test itself isn't sandboxed.
        if in_flatpak() {
            return;
        }
        let o = run(&host(&["sh", "-c", "cat; echo err >&2; exit 3"]), Some("typed")).unwrap();
        assert_eq!((o.code, o.stdout.as_str()), (3, "typed"));
        assert_eq!(reason(&o), "err");
        assert!(run(&["turbinehub-no-such-program".to_string()], None).is_err());
    }
}
