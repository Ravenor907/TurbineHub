//! Turbine Hub's Steam compatibility tool. The script runs here behind stubs of Steam's chain (launch wrapper and
//! reaper in front, as Steam puts them) with stub runtimes, Protons and a stub game: each stub writes its arguments
//! to a trace and passes control on the way the real one does. Paths hold spaces, as on a real machine. Everything
//! lives in a scratch folder: a made-up Steam, a made-up home, Turbine Hub's own folder.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use support::{has_bash, scratch as tmp};
use turbinehub_engine::compat_tool::{self as tool, ProtonKind, State};
use turbinehub_engine::switches::{GameSwitches, Switch};

const APP: &str = "1086940";

fn write(p: &Path, text: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn script(p: &Path, body: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    // Written by a child process, never by this one. A file this process holds open for writing is inherited, for
    // a moment, by any child another test's thread starts just then; running the script in that moment fails with
    // "Text file busy". This process never holds it, so nothing inherits it.
    use std::io::Write;
    let mut c = Command::new("sh").arg("-c").arg("cat > \"$1\" && chmod 755 \"$1\"").arg("sh").arg(p).stdin(std::process::Stdio::piped()).spawn().unwrap();
    c.stdin.take().unwrap().write_all(format!("#!/bin/sh\n{body}").as_bytes()).unwrap();
    assert!(c.wait().unwrap().success(), "couldn't write {}", p.display());
}

fn trace_line(name: &str, trace: &Path) -> String {
    format!("{{ printf '%s' '{name}'; for a in \"$@\"; do printf '|%s' \"$a\"; done; printf '\\n'; }} >> '{}'\n", trace.display())
}

/// A Proton folder: its program (traces its arguments and the tool paths it was given, then runs the program after
/// the verb) and its manifest naming the runtime it needs.
fn proton(dir: &Path, trace: &Path, name: &str, needs: Option<&str>) {
    script(&dir.join("proton"), &format!("{}printf '{name}-env|%s|%s\\n' \"$STEAM_COMPAT_TOOL_PATHS\" \"$STEAM_COMPAT_MOUNTS\" >> '{t}'\nshift; exec \"$@\"\n", trace_line(name, trace), t = trace.display()));
    let need = needs.map(|n| format!("  \"require_tool_appid\" \"{n}\"\n")).unwrap_or_default();
    write(&dir.join("toolmanifest.vdf"), &format!("\"manifest\"\n{{\n  \"version\" \"2\"\n  \"commandline\" \"/proton %verb%\"\n{need}  \"use_sessions\" \"1\"\n  \"compatmanager_layer_name\" \"proton\"\n}}\n"));
}

/// A runtime: its app manifest in the library, its folder with the entry point and its own manifest.
fn runtime(lib: &Path, trace: &Path, app: &str, dir: &str, name: &str) -> PathBuf {
    write(&lib.join(format!("steamapps/appmanifest_{app}.acf")), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{app}\"\n\t\"name\"\t\t\"{dir}\"\n\t\"installdir\"\t\t\"{dir}\"\n}}\n"));
    let folder = lib.join("steamapps/common").join(dir);
    let skip = "while [ $# -gt 0 ] && [ \"$1\" != -- ]; do shift; done; shift; exec \"$@\"\n";
    script(&folder.join("_v2-entry-point"), &format!("{}{skip}", trace_line(name, trace)));
    write(&folder.join("toolmanifest.vdf"), "\"manifest\"\n{\n  \"version\" \"2\"\n  \"commandline\" \"/_v2-entry-point --verb=%verb% --\"\n  \"compatmanager_layer_name\" \"container-runtime\"\n}\n");
    folder
}

/// A Steam folder with a second library, three Protons with their runtimes, the game, a home folder, Turbine Hub's
/// folder, and the tool installed.
struct Steam {
    d: PathBuf,
    root: PathBuf,
    sd: PathBuf,
    home: PathBuf,
    hub: PathBuf,
    trace: PathBuf,
    wrapper: PathBuf,
    reaper: PathBuf,
    proton11: PathBuf,
    proton9: PathBuf,
    ge: PathBuf,
    slr4: PathBuf,
    game: PathBuf,
}

impl Steam {
    fn new(name: &str) -> Steam {
        let d = tmp(name);
        let trace = d.join("trace.txt");
        let root = d.join("Steam Root");
        let sd = d.join("SD Card");
        let wrapper = root.join("steamrt64/steam-launch-wrapper");
        let reaper = root.join("steamrt64/reaper");
        let skip = "while [ $# -gt 0 ] && [ \"$1\" != -- ]; do shift; done; shift; ";
        script(&wrapper, &format!("{}{skip}exec \"$@\"\n", trace_line("wrapper", &trace)));
        // Steam's reaper stays as the parent of what it starts.
        script(&reaper, &format!("{}{skip}\"$@\"\ncode=$?\nexit $code\n", trace_line("reaper", &trace)));
        let proton11 = root.join("steamapps/common/Proton 11.0");
        proton(&proton11, &trace, "proton11", Some("4183110"));
        let slr4 = runtime(&root, &trace, "4183110", "SteamLinuxRuntime_4", "slr4");
        // The older Proton and its runtime are in the second library.
        let proton9 = sd.join("steamapps/common/Proton 9.0 (Beta)");
        proton(&proton9, &trace, "proton9", Some("1628350"));
        runtime(&sd, &trace, "1628350", "SteamLinuxRuntime_sniper", "sniper");
        // One the player added, which needs no runtime.
        let ge = root.join("compatibilitytools.d/GE-Proton10-4");
        proton(&ge, &trace, "ge", None);
        write(&ge.join("compatibilitytool.vdf"), "\"compatibilitytools\"\n{\n  \"compat_tools\"\n  {\n    \"GE-Proton10-4\"\n    {\n      \"install_path\" \".\"\n    }\n  }\n}\n");
        write(&root.join("steamapps/libraryfolders.vdf"), &format!("\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n", root.display(), sd.display()));
        // The game says what it was started with: its arguments and what the switches set.
        let game = root.join("steamapps/common/Baldurs Gate 3/bin/bg3.exe");
        script(&game, &format!("{}printf 'env|%s|%s|%s\\n' \"${{PROTON_FSR4_UPGRADE:-}}\" \"${{SteamDeck:-}}\" \"${{PROTON_LOG:-}}\" >> '{}'\n", trace_line("game", &trace), trace.display()));
        let home = d.join("home dir");
        fs::create_dir_all(&home).unwrap();
        let hub = d.join("config dir/turbinehub");
        tool::install(&root, &hub).unwrap();
        Steam { d, root, sd, home, hub, trace, wrapper, reaper, proton11, proton9, ge, slr4, game }
    }

    fn tool(&self) -> PathBuf {
        tool::tool_dir(&self.root).join(tool::SCRIPT)
    }

    /// The frame generation wrapper the Lossless Scaling plugin leaves in the home folder.
    fn framegen(&self) {
        script(&tool::framegen_wrapper(&self.home), &format!("{}exec \"$@\"\n", trace_line("lsfg", &self.trace)));
    }

    fn switches(&self, on: &[Switch], fsr4_all: bool) {
        let mut g = GameSwitches { fsr4_all, ..GameSwitches::default() };
        // Another game's switches never reach this one, not even one whose id starts the same.
        g.set(&format!("steam:{APP}0"), Switch::ProtonLog, true);
        g.set("steam:108694", Switch::Handheld, true);
        for s in on {
            g.set(&format!("steam:{APP}"), *s, true);
        }
        g.save(&self.hub);
    }

    /// Steam's command for a game that uses the tool: its launch wrapper, its reaper, then the tool with the verb
    /// and the game's program.
    fn run(&self, verb: &str) -> (i32, Vec<Vec<String>>) {
        let line = [self.wrapper.display().to_string(), "--oom-score-adjust".into(), "900".into(), "--".into(), self.reaper.display().to_string(), "SteamLaunch".into(), format!("AppId={APP}"), "--".into(), self.tool().display().to_string(), verb.into(), self.game.display().to_string(), "-windowed".into()];
        let _ = fs::remove_file(&self.trace);
        let mut c = Command::new(&line[0]);
        c.args(&line[1..])
            .env("HOME", &self.home)
            .env("SteamAppId", APP)
            .env("STEAM_COMPAT_APP_ID", APP)
            .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &self.root)
            .env("STEAM_COMPAT_LIBRARY_PATHS", format!("{}/steamapps", self.root.display()))
            .env("STEAM_COMPAT_TOOL_PATHS", tool::tool_dir(&self.root))
            .env_remove("STEAM_COMPAT_MOUNTS")
            .env_remove("PROTON_FSR4_UPGRADE")
            .env_remove("SteamDeck")
            .env_remove("PROTON_LOG");
        let mut code = c.status().unwrap().code().unwrap_or(-1);
        // 126 with nothing run: the script was still held open for writing by a child of another test's thread
        // ("Text file busy"). Nothing was used up; once more after a moment.
        for _ in 0..3 {
            if code != 126 || fs::read_to_string(&self.trace).is_ok_and(|t| !t.is_empty()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            code = c.status().unwrap().code().unwrap_or(-1);
        }
        let t = fs::read_to_string(&self.trace).unwrap_or_default();
        (code, t.lines().map(|l| l.split('|').map(str::to_string).collect()).collect())
    }

    fn play(&self) -> (i32, Vec<Vec<String>>) {
        self.run("waitforexitandrun")
    }

    fn log(&self) -> String {
        fs::read_to_string(tool::launch_log(&self.hub)).unwrap_or_default()
    }

    fn done(self) {
        let _ = fs::remove_dir_all(&self.d);
    }
}

/// Who ran, in order (without the lines that only report the environment).
fn names(t: &[Vec<String>]) -> Vec<&str> {
    t.iter().map(|l| l[0].as_str()).filter(|n| !n.ends_with("env")).collect()
}

fn line<'a>(t: &'a [Vec<String>], name: &str) -> &'a [String] {
    t.iter().find(|l| l[0] == name).unwrap_or_else(|| panic!("no {name} in {t:?}"))
}

/// What the game saw of the three switches that are environment variables: FSR 4, Handheld mode, Proton log.
fn env(t: &[Vec<String>]) -> (String, String, String) {
    let l = line(t, "env");
    (l[1].clone(), l[2].clone(), l[3].clone())
}

#[test]
fn the_three_files() {
    let list = tool::compatibilitytool_vdf();
    let v = turbinehub_engine::vdf::parse(&list).unwrap();
    let entry = v.path(&["compatibilitytools", "compat_tools", tool::TOOL_NAME]).unwrap();
    assert_eq!((entry.str_at(&["display_name"]), entry.str_at(&["install_path"]), entry.str_at(&["from_oslist"]), entry.str_at(&["to_oslist"])), (Some("Turbine Hub"), Some("."), Some("windows"), Some("linux")));
    let m = turbinehub_engine::vdf::parse(&tool::toolmanifest_vdf()).unwrap();
    assert_eq!(m.str_at(&["manifest", "commandline"]), Some("/turbinehub-tool %verb%"));
    // No runtime named: the script starts the one the chosen Proton asks for.
    assert_eq!(m.str_at(&["manifest", "require_tool_appid"]), None);
    let text = tool::script_text(Path::new("/home/deck/.config/turbine'hub"));
    assert!(text.starts_with("#!/bin/bash\n") && text.contains("hub='/home/deck/.config/turbinehub'\n") && !text.contains("@HUB_DIR@"));
    // Nothing in it reaches the internet or Steam's own files.
    for word in ["curl", "wget", "config.vdf", "localconfig", "shortcuts.vdf", "sudo", "rm -"] {
        assert!(!text.contains(word), "{word}");
    }
}

#[test]
fn install_writes_only_what_differs_and_never_goes_while_a_game_uses_it() {
    let d = tmp("tool-install");
    let (root, hub) = (d.join("Steam"), d.join("hub"));
    assert!(tool::install(&root, &hub).is_err());
    fs::create_dir_all(root.join("steamapps")).unwrap();
    assert_eq!(tool::state(&root, &hub), State::Missing);
    let first = tool::install(&root, &hub).unwrap();
    assert_eq!((first.written.len(), first.new_folder), (3, true));
    assert_eq!(tool::state(&root, &hub), State::Current);
    assert!(tool::launch_dir(&hub).is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(tool::tool_dir(&root).join(tool::SCRIPT)).unwrap().permissions().mode() & 0o777, 0o755);
    }
    let again = tool::install(&root, &hub).unwrap();
    assert_eq!((again.written.len(), again.new_folder), (0, false));
    // The same tool written for another folder of Turbine Hub's is an update.
    assert_eq!(tool::state(&root, &d.join("other")), State::Outdated);
    fs::write(tool::tool_dir(&root).join("toolmanifest.vdf"), "old").unwrap();
    assert_eq!(tool::state(&root, &hub), State::Outdated);
    assert_eq!(tool::install(&root, &hub).unwrap().written, vec!["toolmanifest.vdf"]);
    // Steam sees it once Steam has started after it was written.
    let made = fs::metadata(tool::tool_dir(&root).join("compatibilitytool.vdf")).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    assert!(!tool::seen_by_steam(&root, None) && !tool::seen_by_steam(&root, Some(made - 10)) && tool::seen_by_steam(&root, Some(made + 10)));
    // A game names the tool: it stays.
    write(&tool::config_file(&root), &format!("\"InstallConfigStore\"\n{{\n\"Software\"\n{{\n\"Valve\"\n{{\n\"Steam\"\n{{\n\"CompatToolMapping\"\n{{\n\"{APP}\"\n{{\n\"name\" \"{}\"\n}}\n\"7\"\n{{\n\"name\" \"{}\"\n}}\n}}\n}}\n}}\n}}\n}}\n", tool::TOOL_NAME, tool::MOD_TOOL_NAME));
    assert_eq!(tool::games_using(&root), vec![APP]);
    assert_eq!(tool::remove(&root), Err(vec![APP.to_string()]));
    assert_eq!(tool::state(&root, &hub), State::Current);
    // A file of someone else's in the folder is left, and so is the folder then.
    fs::remove_file(tool::config_file(&root)).unwrap();
    fs::write(tool::tool_dir(&root).join("notes.txt"), "mine").unwrap();
    assert_eq!(tool::remove(&root), Ok(true));
    assert!(tool::tool_dir(&root).join("notes.txt").exists() && !tool::tool_dir(&root).join(tool::SCRIPT).exists());
    assert_eq!(tool::state(&root, &hub), State::Missing);
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_game_starts_through_the_runtime_and_proton_with_its_switches() {
    if !has_bash() {
        return;
    }
    let s = Steam::new("tool-switches");
    s.framegen();
    s.switches(&[Switch::Fsr4, Switch::FrameGen, Switch::Handheld, Switch::ProtonLog], false);
    let (code, t) = s.play();
    assert_eq!(code, 0, "{t:?}\n{}", s.log());
    // Steam's wrapper and reaper, the frame generation wrapper around the whole line, the runtime Proton 11 asks
    // for, Proton, then the game with its own arguments.
    assert_eq!(names(&t), vec!["wrapper", "reaper", "lsfg", "slr4", "proton11", "game"]);
    assert_eq!(line(&t, "slr4")[1..], ["--verb=waitforexitandrun".to_string(), "--".into(), s.proton11.join("proton").display().to_string(), "waitforexitandrun".into(), s.game.display().to_string(), "-windowed".into()]);
    assert_eq!(line(&t, "game")[1..], ["-windowed".to_string()]);
    assert_eq!(env(&t), ("1".into(), "1".into(), "1".into()));
    // Proton gets itself and its runtime as the tool paths, and both are let into the runtime.
    let paths = format!("{}:{}", s.proton11.display(), s.slr4.display());
    assert_eq!(line(&t, "proton11-env")[1..], [paths.clone(), paths]);
    let log = s.log();
    assert!(log.contains(&format!("Turbine Hub: app {APP} runs with {} [why: the newest Proton installed", s.proton11.display())), "{log}");
    assert!(log.contains("Turbine Hub: switches on: fsr4 handheld log framegen: "), "{log}");
    assert_eq!(tool::last_launch_in(&log, APP).unwrap().0, s.proton11.display().to_string());
    s.done();
}

#[test]
fn only_this_games_switches_apply_and_none_means_a_plain_start() {
    if !has_bash() {
        return;
    }
    let s = Steam::new("tool-plain");
    s.framegen();
    // Other games have switches on (ids that start the same as this one's); this game has none.
    s.switches(&[], false);
    let (code, t) = s.play();
    assert_eq!(code, 0);
    assert_eq!(names(&t), vec!["wrapper", "reaper", "slr4", "proton11", "game"]);
    assert_eq!(env(&t), (String::new(), String::new(), String::new()));
    assert!(s.log().contains("switches on: none: "));
    // One switch on is one variable set.
    s.switches(&[Switch::Handheld], false);
    assert_eq!(env(&s.play().1), (String::new(), "1".into(), String::new()));
    // FSR 4 for every game reaches this one too.
    s.switches(&[], true);
    assert_eq!(env(&s.play().1), ("1".into(), String::new(), String::new()));
    s.done();
}

#[test]
fn frame_generation_without_its_wrapper_still_starts_the_game() {
    if !has_bash() {
        return;
    }
    let s = Steam::new("tool-nofg");
    s.switches(&[Switch::FrameGen, Switch::Fsr4], false);
    let (code, t) = s.play();
    assert_eq!(code, 0);
    assert_eq!(names(&t), vec!["wrapper", "reaper", "slr4", "proton11", "game"]);
    assert_eq!(env(&t).0, "1");
    assert!(s.log().contains("frame generation is on, but its wrapper") && s.log().contains("switches on: fsr4: "));
    s.done();
}

#[test]
fn a_game_with_online_anti_cheat_never_gets_a_switch() {
    if !has_bash() {
        return;
    }
    let s = Steam::new("tool-locked");
    s.framegen();
    s.switches(&[Switch::Fsr4, Switch::FrameGen, Switch::Handheld, Switch::ProtonLog], true);
    tool::write_locked(&s.hub, &["7".to_string(), APP.to_string(), "not an id".to_string(), APP.to_string()]).unwrap();
    assert_eq!(fs::read_to_string(tool::launch_dir(&s.hub).join("locked.list")).unwrap(), format!("{APP}\n7\n"));
    let (code, t) = s.play();
    assert_eq!(code, 0);
    assert_eq!(names(&t), vec!["wrapper", "reaper", "slr4", "proton11", "game"]);
    assert_eq!(env(&t), (String::new(), String::new(), String::new()));
    assert!(s.log().contains("anti-cheat runs online: it starts as it is, with no switch"));
    // Off the list again, the switches are back.
    tool::write_locked(&s.hub, &[format!("{APP}0")]).unwrap();
    assert_eq!(env(&s.play().1), ("1".into(), "1".into(), "1".into()));
    s.done();
}

#[test]
fn the_players_pick_then_steams_choice_then_the_newest() {
    if !has_bash() {
        return;
    }
    let s = Steam::new("tool-pick");
    s.switches(&[], false);
    // The player's pick for this game: the added Proton, which names no runtime.
    tool::set_proton_pick(&s.hub, APP, Some(&s.ge)).unwrap();
    let (code, t) = s.play();
    assert_eq!(code, 0);
    assert_eq!(names(&t), vec!["wrapper", "reaper", "ge", "game"]);
    assert_eq!(line(&t, "ge-env")[1], s.ge.display().to_string());
    assert!(s.log().contains("[why: your pick in Turbine Hub for this game]"));
    // Steam's own choice for all games, kept by Turbine Hub from Steam's settings (read only): the older Proton,
    // with its own runtime from the second library.
    write(&tool::config_file(&s.root), "\"InstallConfigStore\"\n{\n\"Software\"\n{\n\"Valve\"\n{\n\"Steam\"\n{\n\"CompatToolMapping\"\n{\n\"0\"\n{\n\"name\" \"proton_9\"\n}\n}\n}\n}\n}\n}\n");
    assert_eq!(tool::write_default_proton(&s.root, &s.hub), Some(s.proton9.clone()));
    assert_eq!(names(&s.play().1), vec!["wrapper", "reaper", "ge", "game"]);
    // Undo: back to Automatic, which is Steam's choice now.
    assert_eq!(tool::undo_proton_pick(&s.hub, APP).unwrap(), None);
    let (_, t) = s.play();
    assert_eq!(names(&t), vec!["wrapper", "reaper", "sniper", "proton9", "game"]);
    assert!(s.log().contains("[why: Steam's own choice for all games]"));
    // Steam has no choice of its own any more: the file goes and the newest finished Proton is taken.
    fs::remove_file(tool::config_file(&s.root)).unwrap();
    assert_eq!(tool::write_default_proton(&s.root, &s.hub), None);
    assert!(!tool::default_proton_file(&s.hub).exists());
    assert_eq!(names(&s.play().1), vec!["wrapper", "reaper", "slr4", "proton11", "game"]);
    // A pick whose folder is gone is passed over, and the log says so.
    tool::set_proton_pick(&s.hub, APP, Some(&s.ge)).unwrap();
    fs::remove_dir_all(&s.ge).unwrap();
    assert_eq!(names(&s.play().1), vec!["wrapper", "reaper", "slr4", "proton11", "game"]);
    assert!(s.log().contains("not used: ") && s.log().contains("(no Proton there)"));
    s.done();
}

#[test]
fn a_missing_runtime_moves_on_and_no_proton_at_all_says_so() {
    if !has_bash() {
        return;
    }
    let s = Steam::new("tool-missing");
    s.switches(&[Switch::Fsr4], false);
    // Proton 11's runtime is gone: the Beta in the second library can still run.
    fs::remove_file(s.root.join("steamapps/appmanifest_4183110.acf")).unwrap();
    let (code, t) = s.play();
    assert_eq!(code, 0);
    assert_eq!(names(&t), vec!["wrapper", "reaper", "sniper", "proton9", "game"]);
    assert!(s.log().contains("isn't installed") && s.log().contains("is a Beta"));
    // Nothing that can run: nothing starts, and the log says what to do.
    fs::remove_dir_all(&s.proton9).unwrap();
    fs::remove_dir_all(&s.ge).unwrap();
    let (code, t) = s.play();
    assert_eq!(code, 1);
    assert_eq!(names(&t), vec!["wrapper", "reaper"]);
    assert!(s.log().contains("no Proton that can run was found") && s.log().contains("nothing started"));
    s.done();
}

#[test]
fn with_turbine_hubs_files_gone_the_game_still_starts() {
    if !has_bash() {
        return;
    }
    let s = Steam::new("tool-gone");
    s.framegen();
    s.switches(&[Switch::Fsr4, Switch::FrameGen], false);
    tool::set_proton_pick(&s.hub, APP, Some(&s.ge)).unwrap();
    // The games file alone is gone: the pick still holds, no switch applies.
    fs::remove_file(GameSwitches::file(&s.hub)).unwrap();
    let (code, t) = s.play();
    assert_eq!((code, names(&t)), (0, vec!["wrapper", "reaper", "ge", "game"]));
    assert!(s.log().contains("no games file"));
    // Everything of Turbine Hub's is gone (the app was removed): the newest Proton, no switches, and the log moves
    // into the tool's own folder.
    fs::remove_dir_all(s.hub.parent().unwrap()).unwrap();
    let (code, t) = s.play();
    assert_eq!((code, names(&t)), (0, vec!["wrapper", "reaper", "slr4", "proton11", "game"]));
    assert_eq!(env(&t), (String::new(), String::new(), String::new()));
    assert!(fs::read_to_string(tool::tool_dir(&s.root).join("launch.log")).unwrap().contains("runs with"));
    assert!(!s.hub.exists());
    s.done();
}

#[test]
fn steams_set_up_steps_never_get_switches_and_odd_calls_start_nothing() {
    if !has_bash() {
        return;
    }
    let s = Steam::new("tool-run");
    s.framegen();
    s.switches(&[Switch::Fsr4, Switch::FrameGen, Switch::ProtonLog], true);
    // "run" is what Steam uses for a game's first-time installers.
    let (code, t) = s.run("run");
    assert_eq!((code, names(&t)), (0, vec!["wrapper", "reaper", "slr4", "proton11", "game"]));
    assert_eq!(line(&t, "slr4")[1], "--verb=run");
    assert_eq!(env(&t), (String::new(), String::new(), String::new()));
    let (code, t) = s.run("destroyprefix");
    assert_eq!((code, names(&t)), (1, vec!["wrapper", "reaper"]));
    assert!(s.log().contains("not a launch Steam makes (destroyprefix): nothing started"));
    s.done();
}

#[test]
fn the_installed_protons_are_listed_best_first_and_picks_are_checked() {
    let s = Steam::new("tool-list");
    let list = tool::installed_protons(&s.root);
    assert_eq!(list.iter().map(|p| (p.name.as_str(), p.kind, p.usable)).collect::<Vec<_>>(), vec![("Proton 11.0", ProtonKind::Stable, true), ("Proton 9.0 (Beta)", ProtonKind::Beta, true), ("GE-Proton10-4", ProtonKind::Other, true)]);
    assert_eq!(list[2].steam_name.as_deref(), Some("GE-Proton10-4"));
    assert!(list[0].matches_steam_name("proton_11") && !list[0].matches_steam_name("proton_1") && list[1].matches_steam_name("proton_9") && list[2].matches_steam_name("GE-Proton10-4"));
    // Automatic: Steam's own choice when it names one that is here, else the best installed. Never one of the two
    // launch tools themselves.
    let config = |name: &str| format!("\"InstallConfigStore\"\n{{\n\"Software\"\n{{\n\"Valve\"\n{{\n\"Steam\"\n{{\n\"CompatToolMapping\"\n{{\n\"0\"\n{{\n\"name\" \"{name}\"\n}}\n}}\n}}\n}}\n}}\n}}\n");
    assert_eq!(tool::automatic(&list, &config("GE-Proton10-4")).map(|p| p.name.as_str()), Some("GE-Proton10-4"));
    assert_eq!(tool::automatic(&list, &config("proton_experimental")).map(|p| p.name.as_str()), Some("Proton 11.0"));
    assert_eq!(tool::automatic(&list, &config(tool::TOOL_NAME)).map(|p| p.name.as_str()), Some("Proton 11.0"));
    assert_eq!(tool::automatic(&[], ""), None);
    // A runtime that is missing makes its Proton unusable, and it sorts behind.
    fs::remove_file(s.root.join("steamapps/appmanifest_4183110.acf")).unwrap();
    let list = tool::installed_protons(&s.root);
    assert!(!list[0].usable && list[0].name == "Proton 11.0");
    assert_eq!(tool::automatic(&list, "").map(|p| p.name.as_str()), Some("Proton 9.0 (Beta)"));
    // Picks: only a Steam app id, only a Proton that is there; the same pick twice keeps the first Undo.
    assert!(tool::set_proton_pick(&s.hub, "../x", Some(&s.ge)).is_err());
    assert!(tool::set_proton_pick(&s.hub, APP, Some(&s.sd)).is_err());
    assert!(tool::undo_proton_pick(&s.hub, APP).is_err());
    assert_eq!(tool::proton_before(&s.hub, APP), None);
    tool::set_proton_pick(&s.hub, APP, Some(&s.ge)).unwrap();
    tool::set_proton_pick(&s.hub, APP, Some(&s.ge)).unwrap();
    assert_eq!(tool::proton_before(&s.hub, APP), Some(None));
    tool::set_proton_pick(&s.hub, APP, Some(&s.proton9)).unwrap();
    assert_eq!((tool::proton_pick(&s.hub, APP), tool::proton_before(&s.hub, APP)), (Some(s.proton9.clone()), Some(Some(s.ge.clone()))));
    assert_eq!(tool::undo_proton_pick(&s.hub, APP).unwrap(), Some(s.ge.clone()));
    assert_eq!((tool::proton_pick(&s.hub, APP), tool::proton_before(&s.hub, APP)), (Some(s.ge.clone()), Some(Some(s.proton9.clone()))));
    tool::set_proton_pick(&s.hub, APP, None).unwrap();
    assert_eq!((tool::proton_pick(&s.hub, APP), tool::proton_before(&s.hub, APP)), (None, Some(Some(s.ge.clone()))));
    s.done();
}
