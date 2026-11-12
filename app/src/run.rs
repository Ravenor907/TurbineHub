//! What runs off the interface thread: the steps of a confirmed dialog, and the facts only the host or the internet
//! can give (installed apps, updates, the password state, sound outputs, ratings). Nothing here decides anything:
//! the engine built the steps, and the facts go back to it as they are.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use turbinehub_engine::agent::{self, Found};
use turbinehub_engine::json::Json;
use turbinehub_engine::net::Client;
use turbinehub_engine::selfupdate::{self, Check};
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
    pub steam_running: bool,
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
    /// Some(version) when a newer Turbine Hub is out, Some("") when this one is the newest.
    pub update: Option<String>,
    /// Decky plugins with a newer version, when the plugin store's list could be read.
    pub plugins: Option<Vec<String>>,
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

/// Runs one command line to its end, with `stdin` typed into it when given.
pub fn run(line: &[String], stdin: Option<&str>) -> Result<Output, String> {
    let mut c = command(line).ok_or("Nothing to run")?;
    c.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| format!("Couldn't start {}: {e}", line.first().map(String::as_str).unwrap_or("")))?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(text.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    Ok(Output { code: out.status.code().unwrap_or(-1), stdout: String::from_utf8_lossy(&out.stdout).into_owned(), stderr: String::from_utf8_lossy(&out.stderr).into_owned() })
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

/// What the steps of a plan need besides themselves.
pub struct Job {
    pub steps: Vec<Step>,
    pub password: String,
    pub home: PathBuf,
    /// Turbine Hub's own folder (the games file, the launch folder).
    pub folder: PathBuf,
    /// Turbine Hub's data folder (the save snapshots).
    pub data: PathBuf,
    pub steam_running: bool,
    pub version: String,
    pub cache: PathBuf,
    pub test_builds: bool,
}

fn step(job: &Job, st: &Step, say: &dyn Fn(String)) -> Result<(), String> {
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
            turbinehub_engine::shortcuts::add_shortcut(&file, &ui::store::tile_shortcut(name, url), &ui::store::tile_marker(url), job.steam_running).map(|_| ())
        }
        Step::CopyGame { name, pairs } => {
            say(format!("Copying {name}"));
            for (from, to) in pairs {
                ui::bring::copy_tree(from, to).map_err(|e| format!("Couldn't copy {name}: {e}"))?;
            }
            Ok(())
        }
        Step::ResetPrefix { pfx } => ui::games::reset_windows_files(pfx, turbinehub_engine::now()).map(|_| ()),
        Step::Tool { .. } | Step::ProtonPick { .. } | Step::ProtonUndo { .. } => ui::launch::run_step(st, &job.home, &job.folder).unwrap_or(Ok(())),
        Step::Saves(work) => {
            let mut runner = |line: &[String]| run(line, None).map(|o| turbinehub_engine::saves::Ran { code: o.code, stdout: o.stdout, stderr: o.stderr });
            let places = turbinehub_engine::saves::Places { folder: &job.folder, data: &job.data, home: &job.home };
            turbinehub_engine::saves::run(work, &places, turbinehub_engine::now(), say, &mut runner).map(|_| ())
        }
        Step::TileRemove { url } => {
            let file = turbinehub_engine::shortcuts::tile_file(&job.home).ok_or("Steam's list of non-Steam games wasn't found.")?;
            turbinehub_engine::shortcuts::remove_shortcut(&file, &ui::store::tile_marker(url), job.steam_running).map(|_| ())
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
            if o.code == 0 { Ok(()) } else { Err(reason(&o)) }
        }
        // These need the engine's state: the interface thread does them once the rest worked.
        Step::Report { .. } | Step::SaveSetup | Step::Done { .. } | Step::SetupFinished { .. } => Ok(()),
    }
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
/// None. Also answers whether the internet was reached (Some(false): tried and nothing answered).
pub fn look_for_updates(home: &Path, cache: &Path, version: &str, test_builds: bool) -> (Found, Option<bool>) {
    let client = Client::new(version, cache);
    let mut found = Found { apps: lines(host_text(&["flatpak", "remote-ls", "--updates", "--app", "--columns=application"])), ..Found::default() };
    let (mut worked, mut failed) = (false, false);
    match selfupdate::check(&client, version, test_builds) {
        Ok(Check::Available(r)) => found.update = Some(r.version.clone()),
        Ok(Check::UpToDate) => found.update = Some(String::new()),
        Err(_) => failed = true,
    }
    worked |= found.update.is_some();
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
    f.steam_running = run(&host(&["pgrep", "-x", "steam"]), None).is_ok_and(|o| o.code == 0);
    f.steam_started = steam_started();
    f.sound = host_text(&["pactl", "list", "sinks"]).map(|t| agent::sinks(&t)).unwrap_or_default();
    f.sound_now = host_text(&["pactl", "get-default-sink"]).map(|t| t.trim().to_string()).unwrap_or_default();
    f.efibootmgr = host_text(&["efibootmgr"]);
    let steam = home.join(".local/share/Steam");
    f.free_bytes = Command::new("df").args(["-B1", "--output=avail"]).arg(if steam.is_dir() { &steam } else { home }).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).lines().nth(1).and_then(|l| l.trim().parse().ok()));
    // Without the helper there is no record, so nothing can have drifted.
    f.drift = if helper_there { verify() } else { Some(Vec::new()) };
    // systemctl says "disabled", or nothing at all for a unit that isn't there, with a failing exit code: only its
    // word counts.
    f.agent_on = run(&agent::unit_question(), None).ok().map(|o| agent::unit_enabled(&o.stdout));
    f
}

/// Where a gathering reads and writes.
pub struct Gather {
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub cache: PathBuf,
    pub version: String,
    pub test_builds: bool,
    pub helper_there: bool,
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
        let (found, updates_online) = look_for_updates(&g.home, &g.cache, &g.version, g.test_builds);
        let answers = [lists.online, if decky.origin == Origin::Live { Some(true) } else if decky.offline { Some(false) } else { None }, updates_online];
        let f = Facts {
            updates: found.apps,
            update: found.update,
            plugins: found.plugins,
            decky_tag: sources::release_tag(&decky.text),
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
