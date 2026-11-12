//! What runs off the interface thread: the steps of a confirmed dialog, and the facts only the host or the internet
//! can give (installed apps, updates, the password state, sound outputs, ratings). Nothing here decides anything:
//! the engine built the steps, and the facts go back to it as they are.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use turbinehub_engine::helper;
use turbinehub_engine::json::{self, Json};
use turbinehub_engine::net::Client;
use turbinehub_engine::selfupdate::{self, Check};
use turbinehub_engine::ui::{self, Step};

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
    pub sound: Vec<(String, String)>,
    pub sound_now: String,
    pub free_bytes: Option<u64>,
    pub efibootmgr: Option<String>,
    pub drift: Vec<String>,
    /// Some(true) when the internet answered, Some(false) when it didn't, None when it wasn't asked.
    pub online: Option<bool>,
    pub decky_tag: Option<String>,
    /// Some(version) when a newer Turbine Hub is out, Some("") when this one is the newest.
    pub update: Option<String>,
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

struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Runs one command line to its end, with `stdin` typed into it when given.
fn run(line: &[String], stdin: Option<&str>) -> Result<Output, String> {
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
fn host_text(args: &[&str]) -> Option<String> {
    run(&host(args), None).ok().filter(|o| o.code == 0).map(|o| o.stdout)
}

/// The last line a failed command printed, as the reason.
fn reason(o: &Output) -> String {
    let last = |text: &str| text.lines().map(str::trim).rfind(|l| !l.is_empty()).map(String::from);
    last(&o.stderr).or_else(|| last(&o.stdout)).unwrap_or_else(|| "It didn't work.".into())
}

/// A helper job's result: its own answer, or what sudo or pkexec said when it never ran.
fn helper_result(o: &Output) -> Result<(), String> {
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

/// The sound outputs from pactl's list: each sink's name and what a person calls it.
pub fn sinks(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for l in text.lines().map(str::trim) {
        if let Some(name) = l.strip_prefix("Name: ") {
            out.push((name.to_string(), name.to_string()));
        } else if let (Some(words), Some(last)) = (l.strip_prefix("Description: "), out.last_mut()) {
            last.1 = words.to_string();
        }
    }
    out
}

/// What the host can say without the internet. Quick, so screens fill in at once.
fn local_facts(home: &Path, helper_there: bool) -> Facts {
    let mut f = Facts { local: true, installed: lines(host_text(&["flatpak", "list", "--app", "--columns=application"])), ..Facts::default() };
    f.has_password = host_text(&["passwd", "-S"]).and_then(|t| t.split_whitespace().nth(1).map(|w| w != "NP"));
    f.steam_running = run(&host(&["pgrep", "-x", "steam"]), None).is_ok_and(|o| o.code == 0);
    f.sound = host_text(&["pactl", "list", "sinks"]).map(|t| sinks(&t)).unwrap_or_default();
    f.sound_now = host_text(&["pactl", "get-default-sink"]).map(|t| t.trim().to_string()).unwrap_or_default();
    f.efibootmgr = host_text(&["efibootmgr"]);
    let steam = home.join(".local/share/Steam");
    f.free_bytes = Command::new("df").args(["-B1", "--output=avail"]).arg(if steam.is_dir() { &steam } else { home }).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).lines().nth(1).and_then(|l| l.trim().parse().ok()));
    if helper_there
        && let Ok(o) = run(&helper::job("verify", None), None)
        && let Ok(j) = helper::answer(&o.stdout)
    {
        f.drift = j.get("drift").map(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect();
    }
    f
}

/// Gathers the facts on a new thread: the host's first, then what needs the internet. `downloads` are the rating
/// files to fetch (address, where to keep it). A file that can't be fetched keeps its last copy.
pub fn gather(home: PathBuf, cache: PathBuf, version: String, test_builds: bool, helper_there: bool, downloads: Vec<(String, PathBuf)>, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let _ = tx.send(Event::Facts(Box::new(local_facts(&home, helper_there))));
        let client = Client::new(&version, &cache);
        let mut f = Facts { updates: lines(host_text(&["flatpak", "remote-ls", "--updates", "--app", "--columns=application"])), ..Facts::default() };
        let mut reached = false;
        // At most 40 new files a run: the lists, then Valve's report for games that have none yet.
        for (url, file) in downloads.into_iter().take(40) {
            match client.get_text(&url, &[]) {
                Ok(text) if json::parse(&text).is_ok() => {
                    reached = true;
                    if let Some(d) = file.parent() {
                        let _ = std::fs::create_dir_all(d);
                    }
                    let _ = std::fs::write(&file, text);
                }
                Ok(_) => reached = true,
                Err(_) => {}
            }
        }
        let tag_file = cache.join("decky-tag");
        if let Ok(text) = client.get_text("https://api.github.com/repos/SteamDeckHomebrew/decky-loader/releases/latest", &[]) {
            reached = true;
            if let Some(tag) = json::parse(&text).ok().and_then(|j| j.str_at("tag_name").map(String::from)) {
                let _ = std::fs::write(&tag_file, &tag);
            }
        }
        f.decky_tag = std::fs::read_to_string(&tag_file).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        match selfupdate::check(&client, &version, test_builds) {
            Ok(Check::Available(r)) => {
                reached = true;
                f.update = Some(r.version.clone());
            }
            Ok(Check::UpToDate) => {
                reached = true;
                f.update = Some(String::new());
            }
            Err(_) => {}
        }
        f.online = Some(reached);
        if reached {
            ui::set_checked(&cache, turbinehub_engine::now());
        }
        let _ = tx.send(Event::Facts(Box::new(f)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sound_outputs_with_their_plain_names() {
        let text = "Sink #56\n\tState: RUNNING\n\tName: alsa_output.pci-0000_03_00.1.hdmi-stereo\n\tDescription: Navi 48 HDMI Digital Stereo\n\tDriver: PipeWire\nSink #71\n\tName: alsa_output.usb-headset.analog-stereo\n\tDescription: Arctis Nova 7\n";
        assert_eq!(sinks(text), vec![("alsa_output.pci-0000_03_00.1.hdmi-stereo".to_string(), "Navi 48 HDMI Digital Stereo".to_string()), ("alsa_output.usb-headset.analog-stereo".to_string(), "Arctis Nova 7".to_string())]);
        assert!(sinks("").is_empty());
    }

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
