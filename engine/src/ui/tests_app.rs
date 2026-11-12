//! What the app itself needs from the screens: Steam's answer in three kinds, Steam libraries Turbine Hub may not
//! read, what follows an update, Turbine Hub's own log in a report and its library art at every start.
use super::plan::Step;
use super::selftile;
use super::*;
use crate::fake::Preset;
use crate::json::parse;
use crate::shortcuts::Steam;

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-app-{tag}-{}", std::process::id()))
}

fn ui(tag: &str, preset: Preset, mode: Mode) -> Ui {
    Ui::sample(&dir(tag), preset, Some(mode))
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

fn rows(j: &Json) -> Vec<Json> {
    j.get("sections").map(Json::as_array).unwrap_or(&[]).iter().flat_map(|sec| sec.get("rows").map(Json::as_array).unwrap_or(&[]).to_vec()).collect()
}

fn checkup(u: &mut Ui) -> Vec<Json> {
    u.home_all = true;
    let j = parse(&u.page("home", "")).unwrap();
    let mut out = Vec::new();
    find_items(&j, &mut out);
    out
}

/// Every object with an `opens` key: the checkup's cards, wherever the page keeps them.
fn find_items(j: &Json, out: &mut Vec<Json>) {
    match j {
        Json::Obj(e) => {
            if e.iter().any(|(k, _)| k == "opens") {
                out.push(j.clone());
            }
            for (_, v) in e {
                find_items(v, out);
            }
        }
        Json::Arr(a) => a.iter().for_each(|v| find_items(v, out)),
        _ => {}
    }
}

fn art_set(tag: &str) -> PathBuf {
    let share = dir(tag).join("share");
    for part in ["capsule", "wide", "hero", "logo", "icon"] {
        let f = selftile::art_dir(&share).join(format!("{part}.png"));
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, format!("png {part}")).unwrap();
    }
    share
}

#[test]
fn when_steam_cant_be_asked_nothing_is_offered_and_nothing_is_written() {
    let mut u = ui("unsure", HANDHELD, Mode::Guided);
    u.sample_steam_unsure();
    let share = art_set("unsure");
    let (home, folder) = (u.ctx.home.clone(), u.folder());
    let file = home.join(".local/share/Steam/userdata/1001/config/shortcuts.vdf");
    // Adding itself: the steps, in words that say Turbine Hub couldn't check, and no dialog to confirm.
    let d = parse(&u.act("self-tile", "add", "")).unwrap();
    let d = d.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.str_at("confirm")), (Some("Steam has to be closed for this"), Some("")));
    assert!(d.str_at("text").unwrap().starts_with("Turbine Hub couldn't check whether Steam is running"), "{d:?}");
    assert!(u.confirm().is_none());
    // The step itself refuses for "can't tell" exactly as for "running", in plain words, and writes nothing.
    for steam in [Steam::Unknown, Steam::Running] {
        let why = selftile::run_step(&Step::SelfTile { add: true }, &home, &folder, &share, steam).unwrap().unwrap_err();
        assert!(why.contains("Steam") && !why.contains("pgrep") && !why.contains("os error"), "{why}");
        assert!(!file.exists());
    }
    assert!(selftile::run_step(&Step::SelfTile { add: true }, &home, &folder, &share, Steam::Unknown).unwrap().unwrap_err().starts_with("Turbine Hub couldn't check whether Steam is running"));
    // The first-run pick is held back, and a streaming tile's dialog says to close Steam first.
    let all = rows(&parse(&u.page("quick", "")).unwrap());
    let pick = all.iter().find(|x| x.str_at("title") == Some(selftile::TITLE)).unwrap();
    assert_eq!((pick.str_at("kind"), pick.str_at("badge")), (Some("info"), Some("Later, with Steam closed")));
    let d = parse(&u.act("tile", "netflix", "")).unwrap();
    let items = d.get("dialog").unwrap().get("items").map(Json::as_array).unwrap().to_vec();
    assert!(items.iter().any(|i| i.str_at("title") == Some("Close Steam first") && i.str_at("line") == Some(selftile::UNSURE)), "{items:?}");
    u.cancel();
    // With the entry there (added while Steam was closed), removing it is Steam's own menu while Turbine Hub can't
    // tell, and the remove step refuses as well.
    selftile::run_step(&Step::SelfTile { add: true }, &home, &folder, &share, Steam::Closed).unwrap().unwrap();
    let before = std::fs::read(&file).unwrap();
    let d = parse(&u.act("self-tile", "remove", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Remove it in Steam"));
    assert!(d.get("dialog").unwrap().str_at("text").unwrap().starts_with("Turbine Hub couldn't check"));
    assert!(selftile::run_step(&Step::SelfTile { add: false }, &home, &folder, &share, Steam::Unknown).unwrap().is_err());
    assert_eq!(std::fs::read(&file).unwrap(), before);
    // The pictures are not Steam's list: they may be written whatever Steam does.
    selftile::run_step(&Step::SelfArt, &home, &folder, &share, Steam::Unknown).unwrap().unwrap();
    done("unsure");
}

#[test]
fn a_steam_library_turbine_hub_may_not_read_is_shown_with_allow() {
    let mut u = ui("unseen", Preset::Desktop, Mode::Guided);
    // Every library readable: nothing about it anywhere.
    assert!(u.unseen_libraries().is_empty());
    assert!(!checkup(&mut u).iter().any(|i| i.str_at("id") == Some("unseen")));
    assert!(!rows(&parse(&u.page("games", "")).unwrap()).iter().any(|r| r.str_at("act") == Some("allow-library")));
    assert_eq!(parse(&u.act("allow-library", "", "")).unwrap().str_at("toast"), Some("Turbine Hub can see every Steam library already."));

    u.sample_with_unseen_library();
    let lib = sample::UNSEEN_LIBRARY;
    assert_eq!(u.unseen_libraries(), vec![PathBuf::from(lib)]);
    // The checkup: something to look at, with Allow.
    let items = checkup(&mut u);
    let item = items.iter().find(|i| i.str_at("id") == Some("unseen")).expect("a checkup card");
    assert_eq!((item.str_at("title"), item.str_at("button"), item.str_at("opens"), item.get("look").and_then(Json::as_bool)), (Some("A Steam library can't be read"), Some("Allow"), Some("allow-library"), Some(true)));
    assert!(item.str_at("line").unwrap().contains(lib));
    // Games: a section under the picked game's panel, with the same press.
    let games = parse(&u.page("games", "")).unwrap();
    let last = games.get("sections").map(Json::as_array).unwrap().last().unwrap().clone();
    assert_eq!(last.str_at("title"), Some("Games missing?"));
    let row = rows(&games).into_iter().find(|r| r.str_at("act") == Some("allow-library")).unwrap();
    assert_eq!((row.str_at("arg"), row.str_at("button")), (Some(lib), Some("Allow")));
    // A folder Steam doesn't list is never allowed, whatever asks.
    assert!(parse(&u.act("allow-library", "/etc", "")).unwrap().get("dialog").is_none());
    assert!(u.confirm().is_none());
    // The dialog shows the folder and says what follows; its one step is flatpak's own override, for this user.
    let d = parse(&u.act("checkup", "allow-library", "")).unwrap();
    let d = d.get("dialog").unwrap().clone();
    assert_eq!((d.str_at("title"), d.str_at("confirm")), (Some("Let Turbine Hub read this Steam library?"), Some("Allow")));
    let items: Vec<(String, String)> = d.get("items").map(Json::as_array).unwrap().iter().map(|i| (i.str_at("title").unwrap_or("").to_string(), i.str_at("line").unwrap_or("").to_string())).collect();
    assert_eq!(items[0], ("The folder".to_string(), lib.to_string()));
    assert_eq!(items[2].0, "Then start Turbine Hub again");
    assert!(!items[1].1.contains("flatpak"), "Guided mode shows no command: {items:?}");
    let plan = u.confirm().unwrap();
    let line: Vec<String> = ["flatpak-spawn", "--host", "flatpak", "override", "--user", "--filesystem=/mnt/games-drive/SteamLibrary", "io.github.ravenor907.TurbineHub"].iter().map(|x| x.to_string()).collect();
    assert_eq!(plan.steps, vec![Step::Host(line)]);
    assert!(!plan.needs_internet() && plan.helper_jobs() == 0);
    assert!(plan.done.contains(lib) && plan.done.ends_with("Close Turbine Hub, then start it again from Steam to see its games."), "{}", plan.done);
    // Once it worked: no more Allow, and the restart is what's left to do, on Home and in Games.
    u.play_sample(&plan).unwrap();
    assert_eq!(u.allowed, vec![PathBuf::from(lib)]);
    assert!(u.unseen_libraries().is_empty());
    let items = checkup(&mut u);
    assert!(!items.iter().any(|i| i.str_at("id") == Some("unseen")));
    let item = items.iter().find(|i| i.str_at("id") == Some("allowed")).unwrap();
    assert_eq!((item.str_at("button"), item.str_at("opens")), (Some("Close"), Some("restart-app")));
    let row = rows(&parse(&u.page("games", "")).unwrap()).into_iter().find(|r| r.str_at("act") == Some("restart-app")).unwrap();
    assert_eq!((row.str_at("title"), row.str_at("button")), (Some("Close Turbine Hub, then start it again from Steam"), Some("Close")));
    assert_eq!(parse(&u.act("allow-library", lib, "")).unwrap().str_at("toast"), Some("That folder is allowed already. Its games show after Turbine Hub starts again."));
    // In Game Mode the dialog closes Turbine Hub and says to start it from Steam; nothing starts by itself.
    let d = parse(&u.act("checkup", "restart-app", "")).unwrap();
    assert!(d.get("restart").is_none());
    let d = d.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.str_at("confirm"), d.str_at("local")), (Some("Close Turbine Hub?"), Some("Close"), Some("quit")));
    assert!(d.str_at("text").unwrap().contains("start it again from Steam") && d.str_at("text").unwrap().contains(lib));
    done("unseen");

    // Veteran mode sees the exact command; in a Desktop Mode window the words say Restart.
    let mut v = ui("unseen-veteran", Preset::Desktop, Mode::Veteran);
    v.window = true;
    v.sample_with_unseen_library();
    let d = parse(&v.act("allow-library", lib, "")).unwrap();
    let items = d.get("dialog").unwrap().get("items").map(Json::as_array).unwrap().to_vec();
    assert_eq!(items[1].str_at("line"), Some("flatpak override --user --filesystem=/mnt/games-drive/SteamLibrary io.github.ravenor907.TurbineHub"));
    let plan = v.confirm().unwrap();
    assert!(plan.done.ends_with("Restart Turbine Hub to see its games."));
    v.play_sample(&plan).unwrap();
    let items = checkup(&mut v);
    assert_eq!(items.iter().find(|i| i.str_at("id") == Some("allowed")).unwrap().str_at("button"), Some("Restart"));
    done("unseen-veteran");
}

#[test]
fn with_no_games_at_all_the_games_tab_still_offers_allow() {
    let mut u = ui("unseen-empty", HANDHELD, Mode::Guided);
    // Both libraries: Steam's own and the one on the card.
    for lib in [u.ctx.home.join(".local/share/Steam"), u.ctx.root.join(crate::fake::FILES_DRIVE.trim_start_matches('/'))] {
        for e in std::fs::read_dir(lib.join("steamapps")).unwrap().flatten() {
            if e.file_name().to_string_lossy().starts_with("appmanifest_") {
                std::fs::remove_file(e.path()).unwrap();
            }
        }
    }
    let j = parse(&u.page("games", "")).unwrap();
    assert_eq!(j.str_at("empty"), Some("No games found. Install one in Steam or Heroic and it shows up here."));
    assert!(rows(&j).is_empty());
    u.sample_with_unseen_library();
    let j = parse(&u.page("games", "")).unwrap();
    assert_eq!(j.str_at("empty"), Some("No games found yet. Steam lists a library Turbine Hub isn't allowed to read."));
    assert_eq!(rows(&j).iter().filter(|r| r.str_at("act") == Some("allow-library")).count(), 1);
    done("unseen-empty");
}

#[test]
fn after_an_update_the_screens_say_how_to_get_the_new_version() {
    // Game Mode: close it, start it again from Steam.
    let mut u = ui("updated", HANDHELD, Mode::Guided);
    u.update = Some("0.9".into());
    let first = |u: &mut Ui| rows(&parse(&u.page("settings", "updates")).unwrap())[0].clone();
    assert_eq!((first(&mut u).str_at("act"), first(&mut u).str_at("button")), (Some("update-app"), Some("Update")));
    assert!(checkup(&mut u).iter().any(|i| i.str_at("id") == Some("app")));
    parse(&u.act("update-app", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert!(plan.steps.contains(&Step::SelfUpdate) && plan.needs_internet());
    assert_eq!(plan.done, "Turbine Hub 0.9 is installed. Close Turbine Hub, then start it again from Steam.");
    u.play_sample(&plan).unwrap();
    assert_eq!(u.update_installed.as_deref(), Some("0.9"));
    // The same version is not offered again, on Home or in Settings: what's left is to start the new one.
    assert!(!checkup(&mut u).iter().any(|i| i.str_at("id") == Some("app")));
    let row = first(&mut u);
    assert_eq!((row.str_at("act"), row.str_at("title"), row.str_at("button"), row.str_at("badge")), (Some("restart-app"), Some("Close Turbine Hub"), Some("Close"), Some("Installed")));
    assert!(row.str_at("line").unwrap().contains("start it again from Steam"));
    let d = parse(&u.act("restart-app", "", "")).unwrap();
    assert!(d.get("restart").is_none(), "nothing starts by itself in Game Mode");
    assert_eq!(d.get("dialog").unwrap().str_at("local"), Some("quit"));
    assert!(d.get("dialog").unwrap().str_at("text").unwrap().starts_with("Turbine Hub 0.9 is installed. Close it, then start it again from Steam"));
    // A still newer one found later is offered as usual.
    u.update = Some("1.0".into());
    assert!(checkup(&mut u).iter().any(|i| i.str_at("id") == Some("app")));
    done("updated");

    // A Desktop Mode window: Restart, and the app is told to start Turbine Hub again once it closed.
    let mut w = ui("updated-window", Preset::Desktop, Mode::Guided);
    w.window = true;
    w.sample_updated();
    let row = rows(&parse(&w.page("settings", "updates")).unwrap())[0].clone();
    assert_eq!((row.str_at("act"), row.str_at("title"), row.str_at("button")), (Some("restart-app"), Some("Restart Turbine Hub"), Some("Restart")));
    let d = parse(&w.act("restart-app", "", "")).unwrap();
    assert_eq!(d.get("restart").and_then(Json::as_bool), Some(true));
    let d = d.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.str_at("confirm"), d.str_at("cancel"), d.str_at("local")), (Some("Restart Turbine Hub?"), Some("Restart"), Some("Not now"), Some("quit")));
    w.update = Some("1.1".into());
    w.update_installed = None;
    parse(&w.act("update-app", "", "")).unwrap();
    assert_eq!(w.confirm().unwrap().done, "Turbine Hub 1.1 is installed. Restart Turbine Hub to use it.");
    done("updated-window");
}

#[test]
fn a_report_carries_turbine_hubs_own_log_with_nothing_private_in_it() {
    use crate::applog::{self, Log, Private};
    let mut u = ui("applog", HANDHELD, Mode::Veteran);
    // Settings, Help and reports: the report about Turbine Hub itself, and where its log is (home as "~").
    let help = rows(&parse(&u.page("settings", "help")).unwrap());
    let row = help.iter().find(|r| r.str_at("act") == Some("app-report")).unwrap();
    assert_eq!(row.str_at("button"), Some("Write"));
    let info = help.iter().find(|r| r.str_at("title") == Some("Turbine Hub's own log")).unwrap();
    assert!(info.str_at("line").unwrap().contains("app.log and crash.log"));
    // No log yet: the report simply has no such part.
    let text = super::games::report_text(&u, super::games::APP_REPORT, "now");
    assert!(text.contains("Screen: Settings") && !text.contains("Turbine Hub's log") && !text.contains("Crash log"));
    // The app writes its lines through the same cleaning as here.
    let logs = applog::dir(&u.data);
    let private = Private::new(std::path::Path::new("/home/samwise"), None);
    let mut log = Log::open(&logs);
    for i in 0..(applog::REPORT_LINES + 20) {
        log.add(&applog::entry("2026-11-12 10:00:00.000", "act", &private.scrub(&format!("press number {i}"))));
    }
    log.add(&applog::entry("2026-11-12 10:00:01.000", "step", &private.scrub("Host flatpak override --user --filesystem=/home/samwise/Games io.github.ravenor907.TurbineHub: done")));
    log.add(&applog::entry("2026-11-12 10:00:02.000", "update", &private.scrub("curl to 140.82.121.6 with token=abcdef0123 for samwise failed")));
    applog::add_crash(&logs, "2026-11-12 09:00:00.000", &private.scrub("panic in thread main at app/src/run.rs:10: no /home/samwise/.cache"));
    parse(&u.act("app-report", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![Step::Report { game: "app".into() }]);
    u.play_sample(&plan).unwrap();
    let dir = super::games::reports_dir(&u);
    let file = std::fs::read_dir(&dir).unwrap().flatten().next().unwrap().path();
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains(&format!("Turbine Hub's log ({} lines, newest last):", applog::REPORT_LINES)), "{text}");
    // The newest lines are in, the oldest beyond the limit are not.
    assert!(text.contains("press number 219") && !text.contains("press number 5\n"));
    assert!(text.contains("--filesystem=~/Games"));
    assert!(text.contains("Crash log (newest last):") && text.contains("panic in thread main at app/src/run.rs:10: no ~/.cache"));
    for private in ["samwise", "140.82.121.6", "abcdef0123"] {
        assert!(!text.contains(private), "{private} in {text}");
    }
    // The made-up machine's own home folder isn't in the report either.
    assert!(!text.contains(&u.ctx.home.to_string_lossy().into_owned()));
    done("applog");
}

#[test]
fn turbine_hubs_library_art_is_brought_up_to_date_at_every_start() {
    use selftile::State;
    let u = ui("artstart", HANDHELD, Mode::Guided);
    let share = art_set("artstart");
    let (home, folder) = (u.ctx.home.clone(), u.folder());
    // Not in the library: nothing to do, nothing written.
    assert_eq!(selftile::art_at_start(&home, &folder, &share), None);
    selftile::run_step(&Step::SelfTile { add: true }, &home, &folder, &share, Steam::Closed).unwrap().unwrap();
    let (file, appid) = crate::shortcuts::find_self_file(&home).unwrap();
    let grid = crate::steam_art::grid_dir(&file);
    assert_eq!(selftile::state(&home), State::Added { ours: true, art_missing: 0 });
    // Nothing changed since: quiet.
    assert_eq!(selftile::art_at_start(&home, &folder, &share), None);
    // A new version brings a new banner: the next start places it, and says so for the log.
    std::fs::write(selftile::art_dir(&share).join("wide.png"), "png wide, redrawn").unwrap();
    let said = selftile::art_at_start(&home, &folder, &share).unwrap();
    assert!(said.starts_with("Library art brought up to date: 1 written"), "{said}");
    assert_eq!(std::fs::read_to_string(grid.join(format!("{appid}.png"))).unwrap(), "png wide, redrawn");
    // A picture the player put there is theirs for good, and one they cleared in Steam stays cleared.
    std::fs::write(grid.join(format!("{appid}p.png")), "the player's own cover").unwrap();
    std::fs::remove_file(grid.join(format!("{appid}_hero.png"))).unwrap();
    std::fs::write(selftile::art_dir(&share).join("capsule.png"), "png capsule, redrawn").unwrap();
    std::fs::write(selftile::art_dir(&share).join("hero.png"), "png hero, redrawn").unwrap();
    assert_eq!(selftile::art_at_start(&home, &folder, &share), None);
    assert_eq!(std::fs::read_to_string(grid.join(format!("{appid}p.png"))).unwrap(), "the player's own cover");
    assert!(!grid.join(format!("{appid}_hero.png")).exists());
    done("artstart");

    // An entry the player added in Steam, whose pictures Turbine Hub never placed, is left alone until they ask.
    let u = ui("artstart-hand", HANDHELD, Mode::Guided);
    let share = art_set("artstart-hand");
    let (home, folder) = (u.ctx.home.clone(), u.folder());
    let file = home.join(".local/share/Steam/userdata/1001/config/shortcuts.vdf");
    let by_hand = crate::shortcuts::NewShortcut { launch_options: "run io.github.ravenor907.TurbineHub".into(), ..selftile::tile() };
    crate::shortcuts::add_shortcut(&file, &by_hand, "run io.github", Steam::Closed).unwrap();
    assert!(crate::shortcuts::find_self_file(&home).is_some());
    assert_eq!(selftile::art_at_start(&home, &folder, &share), None);
    assert!(!crate::steam_art::grid_dir(&file).exists());
    done("artstart-hand");
}
