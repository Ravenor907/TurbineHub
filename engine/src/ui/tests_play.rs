//! Games, the launch tool, Proton, saves and the Store's search and pages, on the made-up machine.
use super::games::{Switches, matches};
use super::launch::{self, Uses};
use super::plan::Step;
use super::*;
use crate::compat_tool as tool;
use crate::fake::Preset;
use crate::json::parse;
use crate::saves::{Job, WindowsRun};

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-play-{tag}-{}", std::process::id()))
}

fn ui(tag: &str, preset: Preset) -> Ui {
    Ui::sample(&dir(tag), preset, Some(Mode::Guided))
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

fn sections(j: &Json) -> Vec<(String, Vec<Json>)> {
    j.get("sections").map(Json::as_array).unwrap_or(&[]).iter().map(|sec| (sec.str_at("title").unwrap_or("").to_string(), sec.get("rows").map(Json::as_array).unwrap_or(&[]).to_vec())).collect()
}

fn rows(j: &Json) -> Vec<Json> {
    sections(j).into_iter().flat_map(|(_, r)| r).collect()
}

fn titles(list: &[Json]) -> Vec<&str> {
    list.iter().filter_map(|r| r.str_at("title")).collect()
}

fn side(j: &Json) -> Vec<&str> {
    j.get("side").map(Json::as_array).unwrap_or(&[]).iter().filter_map(|x| x.str_at("title")).collect()
}

fn hints(j: &Json) -> Vec<&str> {
    j.get("hints").map(Json::as_array).unwrap_or(&[]).iter().filter_map(|x| x.str_at("label")).collect()
}

const BG3: &str = "steam:1086940";
const CYBERPUNK: &str = "steam:1091500";
const HOLLOW: &str = "steam:367520";
const APEX: &str = "steam:1172470";
const ELDEN: &str = "steam:1245620";

#[test]
fn a_search_matches_words_whatever_their_case_and_marks() {
    assert!(matches("", "Anything"));
    assert!(matches("  ", "Anything"));
    assert!(matches("baldurs", "Baldur's Gate 3"));
    assert!(matches("GATE baldur", "Baldur's Gate 3"));
    assert!(matches("eldenring", "Elden Ring"));
    assert!(matches("gate 3", "Baldur's Gate 3"));
    assert!(!matches("gate 4", "Baldur's Gate 3"));
    assert!(!matches("zelda", "Elden Ring"));
    assert!(matches("disney+", "Disney+"));
}

#[test]
fn the_games_search_narrows_the_list_and_says_when_nothing_matches() {
    let mut u = ui("search", HANDHELD);
    let j = parse(&u.page("games", "")).unwrap();
    assert_eq!(side(&j).len(), 8);
    assert_eq!(j.get("search").unwrap().str_at("hint"), Some("Search games"));
    assert!(hints(&j).contains(&"Search"));
    assert_eq!(parse(&u.act("search", "games", "el")).unwrap().get("refresh").and_then(Json::as_bool), Some(true));
    // The game that was picked isn't among the matches any more: the first match shows.
    let j = parse(&u.page("games", BG3)).unwrap();
    assert_eq!(side(&j), vec!["Elden Ring", "Helldivers 2"]);
    assert_eq!((j.str_at("sideTitle"), j.get("search").unwrap().str_at("value")), (Some("2 of 8 games"), Some("el")));
    assert_eq!(j.get("header").unwrap().str_at("title"), Some("Elden Ring"));
    assert_eq!(parse(&u.page("games", "steam:553850")).unwrap().get("header").unwrap().str_at("title"), Some("Helldivers 2"));
    // Nothing matches: the box stays, with a plain sentence beside it.
    u.act("search", "games", "zelda\n");
    let j = parse(&u.page("games", "")).unwrap();
    assert!(side(&j).is_empty() && rows(&j).is_empty());
    assert_eq!((j.str_at("empty"), j.str_at("sideTitle")), (Some("No game matches \"zelda\"."), Some("0 of 8 games")));
    assert_eq!(j.get("search").unwrap().str_at("value"), Some("zelda"));
    // Emptied, everything is back. Long text is cut, and only Games and the Store have a search.
    u.act("search", "games", "");
    assert_eq!(parse(&u.page("games", "")).unwrap().str_at("sideTitle"), Some("Installed \u{b7} 8 games"));
    u.act("search", "games", &"x".repeat(200));
    assert_eq!(parse(&u.page("games", "")).unwrap().get("search").unwrap().str_at("value").unwrap().len(), 60);
    assert!(parse(&u.act("search", "settings", "x")).unwrap().entries().is_empty());
    done("search");
}

#[test]
fn a_game_that_wont_run_here_shows_only_the_other_ways_to_play() {
    let mut u = ui("wontrun", HANDHELD);
    let j = parse(&u.page("games", APEX)).unwrap();
    let secs = sections(&j);
    assert_eq!(secs.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), vec!["Play it another way"]);
    // Windows is on this machine, so restarting to it comes first; then the two cloud services.
    assert_eq!(titles(&secs[0].1), vec!["Restart to Windows", "GeForce NOW", "Xbox Cloud"]);
    assert!(rows(&j).iter().all(|r| r.str_at("act") != Some("switch") && r.str_at("kind") != Some("toggle")));
    assert!(!hints(&j).contains(&"Play") && j.get("x") == Some(&Json::Null));
    // Asking anyway changes nothing.
    let before = Switches::load(&u.folder());
    assert!(parse(&u.act("switch", &format!("{APEX}|fsr4"), "")).unwrap().get("toast").is_some());
    assert_eq!(Switches::load(&u.folder()), before);
    assert!(parse(&u.act("game", &format!("play|{APEX}"), "")).unwrap().get("dialog").is_none());
    assert!(parse(&u.act("proton-pick", &format!("{APEX}|GE-Proton10-4"), "")).unwrap().get("dialog").is_none());
    assert!(parse(&u.act("tool-how", APEX, "")).unwrap().get("dialog").is_none());
    // Without Windows on the machine only the cloud services are offered.
    let mut tv = ui("wontrun", Preset::TvPc);
    assert_eq!(titles(&rows(&parse(&tv.page("games", APEX)).unwrap())), vec!["GeForce NOW", "Xbox Cloud"]);
    done("wontrun");
}

#[test]
fn switches_say_how_they_reach_each_game() {
    let mut u = ui("reach", HANDHELD);
    // Baldur's Gate 3 uses Turbine Hub in Steam: its page says so and the switch applies at the next start.
    let j = parse(&u.page("games", BG3)).unwrap();
    let chips: Vec<&str> = j.get("header").unwrap().get("chips").unwrap().as_array().iter().filter_map(|c| c.str_at("text")).collect();
    assert_eq!(chips, vec!["Works", "Uses Turbine Hub"]);
    let sw = &sections(&j)[0].1;
    assert_eq!(titles(sw), vec!["On for this game", "FSR 4 upgrade", "Frame generation", "Handheld mode", "Proton log"]);
    assert_eq!(sw[1].get("on").and_then(Json::as_bool), Some(true));
    assert_eq!(parse(&u.act("switch", &format!("{BG3}|log"), "")).unwrap().str_at("toast"), Some("Applies the next time the game starts."));
    // Frame generation says what it needs until the plugin's wrapper is there.
    assert!(sw[2].str_at("line").unwrap().starts_with("Needs the Lossless Scaling plugin"));
    std::fs::write(tool::framegen_wrapper(&u.ctx.home), "#!/bin/sh\nexec \"$@\"\n").unwrap();
    assert_eq!(sections(&parse(&u.page("games", BG3)).unwrap())[0].1[2].str_at("line"), Some("Lossless Scaling, set the multiplier in Decky"));
    // Cyberpunk doesn't use the tool yet: the first row leads to the three steps, and a switch is kept for later.
    let j = parse(&u.page("games", CYBERPUNK)).unwrap();
    let first = &sections(&j)[0].1[0];
    assert_eq!((first.str_at("act"), first.str_at("arg"), first.str_at("button")), (Some("tool-how"), Some(CYBERPUNK), Some("How")));
    assert!(first.str_at("line").unwrap().starts_with("In Steam:"));
    assert_eq!(parse(&u.act("switch", &format!("{CYBERPUNK}|fsr4"), "")).unwrap().str_at("toast"), Some("Saved. It applies once this game uses Turbine Hub in Steam."));
    assert!(Switches::load(&u.folder()).on(CYBERPUNK, "fsr4"));
    let d = parse(&u.act("tool-how", CYBERPUNK, "")).unwrap();
    let steps = d.get("dialog").unwrap().get("items").unwrap().as_array().to_vec();
    assert_eq!(steps.len(), 4);
    assert!(steps[0].str_at("line").unwrap().contains("Cyberpunk 2077") && steps[2].str_at("line").unwrap().contains("\"Turbine Hub\""));
    assert_eq!(d.get("dialog").unwrap().str_at("confirm"), Some(""));
    // Before Steam has started again, the restart comes first, on the row and in the steps.
    u.steam_started = None;
    assert!(sections(&parse(&u.page("games", CYBERPUNK)).unwrap())[0].1[0].str_at("line").unwrap().starts_with("Restart Steam once"));
    let d = parse(&u.act("tool-how", CYBERPUNK, "")).unwrap();
    assert!(d.get("dialog").unwrap().get("items").unwrap().as_array()[0].str_at("line").unwrap().starts_with("Restart Steam once"));
    // Hollow Knight uses Turbine Mod Manager's tool: no switches here, and it says where they are.
    let j = parse(&u.page("games", HOLLOW)).unwrap();
    let sw = &sections(&j)[0].1;
    assert_eq!(titles(sw), vec!["Uses Turbine Mod Manager in Steam", "Open Turbine Mod Manager"]);
    assert!(rows(&j).iter().all(|r| r.str_at("act") != Some("switch")));
    let before = Switches::load(&u.folder());
    assert!(parse(&u.act("switch", &format!("{HOLLOW}|fsr4"), "")).unwrap().str_at("toast").unwrap().contains("Turbine Mod Manager"));
    assert_eq!(Switches::load(&u.folder()), before);
    let tv = launch::tool_view(&u);
    let all = super::games::listed(&u);
    let uses = |id: &str| tv.uses(&all.iter().find(|l| super::games::game_id(&l.game) == id).unwrap().game);
    assert_eq!((uses(BG3), uses(HOLLOW), uses(CYBERPUNK)), (Uses::Hub, Uses::Mods, Uses::None));
    done("reach");
}

#[test]
fn the_launch_tool_is_set_up_kept_and_removed_through_the_dialog() {
    let mut u = ui("tool", HANDHELD);
    let steam = u.ctx.home.join(".local/share/Steam");
    // The Games page keeps what the script reads: the locked games and Steam's Proton for all games.
    parse(&u.page("games", "")).unwrap();
    assert_eq!(std::fs::read_to_string(tool::launch_dir(&u.folder()).join("locked.list")).unwrap(), "1245620\n252950\n553850\n");
    assert!(std::fs::read_to_string(tool::default_proton_file(&u.folder())).unwrap().starts_with(&format!("{}\nsteam\n", steam.join("steamapps/common/Proton 11.0").display())));
    // Set up and seen by Steam: the page says so and lists who uses it.
    let j = parse(&u.page("tool", "")).unwrap();
    let secs = sections(&j);
    assert_eq!((secs[0].1[0].str_at("title"), secs[0].1[0].str_at("badge"), secs[0].1.len()), (Some("Set up"), Some("Ready"), 1));
    assert_eq!(secs[1].1.len(), 3);
    assert_eq!(titles(&secs[2].1), vec!["Baldur's Gate 3", "Hollow Knight"]);
    assert!(secs[2].1[1].str_at("line").unwrap().contains("Turbine Mod Manager"));
    assert_eq!(parse(&u.act("tool", "install", "")).unwrap().str_at("toast"), Some("The launch tool is set up already."));
    // Removing is refused while a game names the tool: Steam couldn't start that game any more.
    let d = parse(&u.act("tool", "remove", "")).unwrap();
    let d = d.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.str_at("confirm")), (Some("Switch these games back first"), Some("")));
    assert_eq!(d.get("items").unwrap().as_array()[0].str_at("title"), Some("Baldur's Gate 3"));
    assert!(u.confirm().is_none());
    assert!(launch::run_step(&Step::Tool { install: false }, &u.ctx.home, &u.folder()).unwrap().unwrap_err().starts_with("1 game still use"));
    assert_eq!(tool::state(&steam, &u.folder()), tool::State::Current);
    // A file that differs is an update; the dialog names the folder and writes only through its step.
    std::fs::write(tool::tool_dir(&steam).join(tool::SCRIPT), "old").unwrap();
    let j = parse(&u.page("tool", "")).unwrap();
    assert_eq!((rows(&j)[0].str_at("title"), rows(&j)[0].str_at("arg")), (Some("An update is ready"), Some("update")));
    assert_eq!(sections(&parse(&u.page("games", CYBERPUNK)).unwrap())[0].1[0].str_at("title"), Some("Update the launch tool"));
    let d = parse(&u.act("tool", "update", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("confirm"), Some("Update"));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![Step::Tool { install: true }]);
    assert_eq!(std::fs::read_to_string(tool::tool_dir(&steam).join(tool::SCRIPT)).unwrap(), "old");
    launch::run_step(&plan.steps[0], &u.ctx.home, &u.folder()).unwrap().unwrap();
    assert_eq!(tool::state(&steam, &u.folder()), tool::State::Current);
    // Once no game names it, Remove asks first and takes only Turbine Hub's own folder.
    std::fs::write(tool::config_file(&steam), "\"InstallConfigStore\"\n{\n}\n").unwrap();
    let d = parse(&u.act("tool", "remove", "")).unwrap();
    assert_eq!((d.get("dialog").unwrap().str_at("confirm"), d.get("dialog").unwrap().get("danger").and_then(Json::as_bool)), (Some("Remove"), Some(true)));
    let plan = u.confirm().unwrap();
    launch::run_step(&plan.steps[0], &u.ctx.home, &u.folder()).unwrap().unwrap();
    assert!(!tool::tool_dir(&steam).exists() && steam.join("compatibilitytools.d/GE-Proton10-4/proton").exists());
    // Never set up: every Proton game's page leads to the tool's page, which offers the one press.
    let first = sections(&parse(&u.page("games", BG3)).unwrap())[0].1[0].clone();
    assert_eq!((first.str_at("act"), first.str_at("arg"), first.str_at("button")), (Some("open"), Some("tool"), Some("Set up")));
    assert_eq!(parse(&u.act("tool-how", BG3, "")).unwrap().str_at("open"), Some("tool"));
    let j = parse(&u.page("tool", "")).unwrap();
    assert_eq!((rows(&j)[0].str_at("act"), rows(&j)[0].str_at("arg")), (Some("tool"), Some("install")));
    // A game that still names the tool in Steam can't start without it, and the page says so.
    std::fs::write(tool::config_file(&steam), "\"InstallConfigStore\"\n{\n\"Software\"\n{\n\"Valve\"\n{\n\"Steam\"\n{\n\"CompatToolMapping\"\n{\n\"1086940\"\n{\n\"name\" \"turbine-hub\"\n}\n}\n}\n}\n}\n}\n").unwrap();
    let named = rows(&parse(&u.page("tool", "")).unwrap()).into_iter().find(|r| r.str_at("title") == Some("Baldur's Gate 3")).unwrap();
    assert_eq!(named.str_at("badge"), Some("Needs the tool"));
    let chips = parse(&u.page("games", BG3)).unwrap().get("header").unwrap().get("chips").unwrap().as_array().len();
    assert_eq!(chips, 1);
    std::fs::write(tool::config_file(&steam), "\"InstallConfigStore\"\n{\n}\n").unwrap();
    assert!(sections(&j).iter().all(|(t, _)| t != "Remove"));
    let d = parse(&u.act("tool", "install", "")).unwrap();
    assert!(d.get("dialog").unwrap().get("items").unwrap().as_array().iter().any(|i| i.str_at("title") == Some("Restart Steam once afterwards")));
    let plan = u.confirm().unwrap();
    launch::run_step(&plan.steps[0], &u.ctx.home, &u.folder()).unwrap().unwrap();
    // Steam started before the tool was written: it hasn't seen it yet, and the page says to restart once.
    u.steam_started = Some(crate::now() - 3600);
    let j = parse(&u.page("tool", "")).unwrap();
    assert_eq!(titles(&sections(&j)[0].1), vec!["Set up", "Restart Steam once"]);
    assert_eq!(parse(&u.act("tool", "remove", "")).unwrap().get("dialog").unwrap().str_at("confirm"), Some("Remove"));
    done("tool");
}

#[test]
fn another_proton_is_a_pick_of_turbine_hubs_own_with_undo() {
    let mut u = ui("proton", HANDHELD);
    let steam = u.ctx.home.join(".local/share/Steam");
    let j = parse(&u.page("proton", BG3)).unwrap();
    let secs = sections(&j);
    assert_eq!(secs[0].1[0].str_at("title"), Some("Starts with Proton 11.0"));
    assert!(secs[0].1[0].str_at("line").unwrap().starts_with("Automatic"));
    assert_eq!(secs[0].1.len(), 1);
    // Automatic first (in use), then every Proton installed, best first.
    assert_eq!(titles(&secs[1].1), vec!["Automatic", "Proton 11.0", "Proton - Experimental", "GE-Proton10-4"]);
    assert_eq!((secs[1].1[0].str_at("badge"), secs[1].1[0].str_at("kind"), secs[1].1[0].str_at("line")), (Some("In use"), Some("info"), Some("Steam's choice for all games: Proton 11.0")));
    assert!(secs[1].1[1..].iter().all(|r| r.str_at("act") == Some("proton-pick") && r.str_at("button") == Some("Use")));
    assert_eq!(parse(&u.act("proton-pick", &format!("{BG3}|"), "")).unwrap().str_at("toast"), Some("That one is in use already."));
    assert!(parse(&u.act("proton-pick", &format!("{BG3}|Proton 99"), "")).unwrap().entries().is_empty());
    assert!(parse(&u.act("proton-undo", BG3, "")).unwrap().get("dialog").is_none());
    // Picking one asks first; the step writes Turbine Hub's own file and nothing of Steam's.
    let config_before = std::fs::read_to_string(tool::config_file(&steam)).unwrap();
    let d = parse(&u.act("proton-pick", &format!("{BG3}|GE-Proton10-4"), "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Use GE-Proton10-4 for Baldur's Gate 3?"));
    assert_eq!(d.get("dialog").unwrap().get("items").unwrap().as_array().len(), 1);
    let plan = u.confirm().unwrap();
    let ge = steam.join("compatibilitytools.d/GE-Proton10-4");
    assert_eq!(plan.steps, vec![Step::ProtonPick { app_id: "1086940".into(), dir: Some(ge.clone()) }]);
    assert_eq!(tool::proton_pick(&u.folder(), "1086940"), None);
    launch::run_step(&plan.steps[0], &u.ctx.home, &u.folder()).unwrap().unwrap();
    assert_eq!(tool::proton_pick(&u.folder(), "1086940"), Some(ge.clone()));
    assert_eq!(std::fs::read_to_string(tool::config_file(&steam)).unwrap(), config_before);
    let j = parse(&u.page("proton", BG3)).unwrap();
    let secs = sections(&j);
    assert_eq!(titles(&secs[0].1), vec!["Starts with GE-Proton10-4", "Undo the last change"]);
    assert_eq!(secs[0].1[1].str_at("line"), Some("Back to Automatic"));
    assert_eq!((secs[1].1[3].str_at("badge"), secs[1].1[3].str_at("kind"), secs[1].1[0].str_at("act")), (Some("In use"), Some("info"), Some("proton-pick")));
    assert!(sections(&parse(&u.page("games", BG3)).unwrap()).iter().any(|(_, r)| r.iter().any(|x| x.str_at("line") == Some("Your pick: GE-Proton10-4"))));
    // Undo puts Automatic back, and Undo again brings the pick back.
    parse(&u.act("proton-undo", BG3, "")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![Step::ProtonUndo { app_id: "1086940".into() }]);
    launch::run_step(&plan.steps[0], &u.ctx.home, &u.folder()).unwrap().unwrap();
    assert_eq!(tool::proton_pick(&u.folder(), "1086940"), None);
    assert_eq!(tool::proton_before(&u.folder(), "1086940"), Some(Some(ge.clone())));
    launch::run_step(&plan.steps[0], &u.ctx.home, &u.folder()).unwrap().unwrap();
    assert_eq!(tool::proton_pick(&u.folder(), "1086940"), Some(ge));
    // A game that doesn't use the tool yet is told the pick waits; a locked one is never touched.
    let d = parse(&u.act("proton-pick", &format!("{CYBERPUNK}|Proton - Experimental"), "")).unwrap();
    assert!(d.get("dialog").unwrap().get("items").unwrap().as_array().iter().any(|i| i.str_at("title").unwrap().starts_with("Applies once the game uses Turbine Hub")));
    u.cancel();
    assert_eq!(sections(&parse(&u.page("proton", CYBERPUNK)).unwrap())[0].1[1].str_at("act"), Some("tool-how"));
    assert!(parse(&u.act("proton-pick", &format!("{ELDEN}|GE-Proton10-4"), "")).unwrap().str_at("toast").unwrap().contains("anti-cheat"));
    assert_eq!(titles(&rows(&parse(&u.page("proton", ELDEN)).unwrap())), vec!["Locked"]);
    assert_eq!(parse(&u.page("proton", "steam:1")).unwrap().str_at("title"), Some("Not found"));
    // A Proton whose runtime is missing is listed, but can't be picked.
    std::fs::remove_file(steam.join("steamapps/appmanifest_4183110.acf")).unwrap();
    let j = parse(&u.page("proton", CYBERPUNK)).unwrap();
    let list = &sections(&j)[1].1;
    assert_eq!(list.iter().filter(|r| r.str_at("badge") == Some("Can't start")).count(), 2);
    assert_eq!(list[0].str_at("line"), Some("The newest one installed: GE-Proton10-4"));
    assert!(parse(&u.act("proton-pick", &format!("{CYBERPUNK}|Proton 11.0"), "")).unwrap().entries().is_empty());
    done("proton");
}

#[test]
fn snapshots_are_listed_and_a_restore_keeps_the_current_saves_first() {
    let mut u = ui("saves", HANDHELD);
    // Without a game the page lists every game with its count.
    let j = parse(&u.page("saves", "")).unwrap();
    let list = rows(&j);
    assert_eq!(list.len(), 8);
    let bg3 = list.iter().find(|r| r.str_at("title") == Some("Baldur's Gate 3")).unwrap();
    assert_eq!((bg3.str_at("line"), bg3.str_at("arg")), (Some("3 snapshots"), Some("saves|steam:1086940")));
    assert!(sections(&parse(&u.page("games", BG3)).unwrap()).iter().any(|(t, r)| t == "Saves" && r[0].str_at("line").unwrap().starts_with("3 snapshots")));
    // Ludusavi isn't installed: the page says what does the copying, and a press leads to it.
    let j = parse(&u.page("saves", BG3)).unwrap();
    let secs = sections(&j);
    assert_eq!(titles(&secs[0].1), vec!["Ludusavi does the copying", "Back up now"]);
    assert_eq!(secs[1].0, "Snapshots \u{b7} 3");
    assert_eq!(titles(&secs[1].1), vec!["30 min ago", "2 days ago", "9 days ago"]);
    assert_eq!(secs[1].1[0].str_at("line"), Some("When the game started \u{b7} 3 MB"));
    assert_eq!(secs[1].1[2].str_at("line"), Some("Taken by hand \u{b7} 3 MB"));
    let newest = secs[1].1[0].str_at("arg").unwrap().to_string();
    let a = parse(&u.act("saves", &newest, "")).unwrap();
    assert_eq!((a.str_at("open"), a.get("dialog")), (Some("store-item|com.github.mtkennerly.ludusavi"), None));
    // With Ludusavi, restoring asks first and says the current saves are kept.
    u.installed.push(store::LUDUSAVI.into());
    assert_eq!(titles(&sections(&parse(&u.page("saves", BG3)).unwrap())[0].1), vec!["Back up now"]);
    let d = parse(&u.act("saves", &newest, "")).unwrap();
    let d = d.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.get("danger").and_then(Json::as_bool)), (Some("Restore this snapshot?"), Some(true)));
    assert!(d.get("items").unwrap().as_array().iter().any(|i| i.str_at("title") == Some("The current saves are kept first")));
    let plan = u.confirm().unwrap();
    let id = newest.rsplit('|').next().unwrap().to_string();
    assert!(matches!(&plan.steps[..], [Step::Saves(Job::Restore { game, at })] if game.id == BG3 && game.steam_id.as_deref() == Some("1086940") && at.to_string() == id));
    parse(&u.act("saves", &format!("backup|{BG3}"), "")).unwrap();
    assert!(matches!(&u.confirm().unwrap().steps[..], [Step::Saves(Job::Backup { game })] if game.name == "Baldur's Gate 3"));
    // Undo shows only once a restore kept something, and a snapshot that is gone says so.
    assert_eq!(parse(&u.act("saves", &format!("undo|{BG3}"), "")).unwrap().str_at("toast"), Some("There is nothing to undo any more."));
    assert_eq!(parse(&u.act("saves", &format!("restore|{BG3}|5"), "")).unwrap().str_at("toast"), Some("That snapshot isn't there any more."));
    // The same snapshots the background helper writes: one kept before a restore, and the note that names it.
    let at = crate::now() - 60;
    let kept = crate::snapshots::game_dir(&u.data, BG3).join(at.to_string());
    std::fs::create_dir_all(&kept).unwrap();
    std::fs::write(kept.join("snapshot.json"), format!("{{\"game\":\"{BG3}\",\"name\":\"Baldur's Gate 3\",\"at\":{at},\"kind\":\"before-restore\",\"bytes\":5,\"files\":1}}")).unwrap();
    std::fs::write(crate::snapshots::game_dir(&u.data, BG3).join("undo"), at.to_string()).unwrap();
    let j = parse(&u.page("saves", BG3)).unwrap();
    assert_eq!(titles(&sections(&j)[0].1), vec!["Back up now", "Undo the last restore"]);
    parse(&u.act("saves", &format!("undo|{BG3}"), "")).unwrap();
    assert!(matches!(&u.confirm().unwrap().steps[..], [Step::Saves(Job::Undo { .. })]));
    // A game with none says how snapshots come to be.
    let j = parse(&u.page("saves", "steam:2379780")).unwrap();
    assert!(sections(&j)[1].1.is_empty());
    assert!(j.get("sections").unwrap().as_array()[1].str_at("empty").unwrap().starts_with("No snapshots yet"));
    done("saves");
}

#[test]
fn the_store_search_looks_everywhere_and_names_come_first() {
    let mut u = ui("find", HANDHELD);
    u.act("search", "store", "stream");
    let j = parse(&u.page("store", "apps")).unwrap();
    let found = rows(&j);
    let names = titles(&found);
    for want in ["Moonlight", "OBS Studio", "Xbox Cloud", "Netflix", "Twitch"] {
        assert!(names.contains(&want), "{want} in {names:?}");
    }
    assert!(!names.contains(&"Discord"));
    assert_eq!(j.get("header").unwrap().str_at("title"), Some("Search"));
    // The sections step aside while the matches come from all of them.
    assert!(side(&j).is_empty());
    assert_eq!(j.get("header").unwrap().str_at("line"), Some(format!("{} matches for \"stream\"", found.len()).as_str()));
    // A name that matches comes before words that match; every row opens its own page or its guide.
    u.act("search", "store", "decky");
    let j = parse(&u.page("store", "")).unwrap();
    assert_eq!(titles(&rows(&j))[0], "Decky Loader");
    u.act("search", "store", "battle");
    assert_eq!(rows(&parse(&u.page("store", "")).unwrap())[0].str_at("act"), Some("launcher-guide"));
    u.act("search", "store", "qqq");
    let j = parse(&u.page("store", "tv")).unwrap();
    assert!(rows(&j).is_empty());
    assert_eq!(j.get("sections").unwrap().as_array()[0].str_at("empty"), Some("Nothing in the Store matches \"qqq\"."));
    // The games search is its own.
    assert_eq!(side(&parse(&u.page("games", "")).unwrap()).len(), 8);
    u.act("search", "store", "");
    let j = parse(&u.page("store", "tv")).unwrap();
    assert_eq!((j.get("header").unwrap().str_at("title"), side(&j).len()), (Some("TV and streaming"), 7));
    done("find");
}

#[test]
fn each_store_item_has_a_page_with_what_is_known_and_what_can_be_done() {
    let mut u = ui("item", HANDHELD);
    let find = |j: &Json, title: &str| rows(j).into_iter().find(|r| r.str_at("title") == Some(title));
    // Installed, no update: version and size on the drive, and Remove.
    let j = parse(&u.page("store-item", "com.discordapp.Discord")).unwrap();
    assert_eq!((j.str_at("crumb"), j.str_at("title")), (Some("Store"), Some("Discord")));
    assert!(j.str_at("intro").unwrap().starts_with("Voice, video and text chat"));
    assert!(sections(&j)[0].1.is_empty());
    assert_eq!((sections(&j)[2].0.as_str(), titles(&sections(&j)[2].1)), ("Remove", vec!["Remove"]));
    assert_eq!(find(&j, "Installed").unwrap().str_at("line"), Some("Version 0.0.98"));
    assert_eq!(find(&j, "Size").unwrap().str_at("line"), Some("318.4 MB on the drive"));
    assert_eq!(find(&j, "Source").unwrap().str_at("line"), Some("Flathub \u{b7} com.discordapp.Discord"));
    assert_eq!(j.str_at("note"), Some("Sizes and versions: checked today"));
    parse(&u.act("store-remove", "com.discordapp.Discord", "")).unwrap();
    assert!(matches!(&u.confirm().unwrap().steps[..], [Step::Host(c)] if c.contains(&"uninstall".to_string())));
    // Not installed: what a fresh install downloads and takes, and Install.
    let j = parse(&u.page("store-item", "com.spotify.Client")).unwrap();
    assert_eq!(titles(&sections(&j)[0].1), vec!["Install"]);
    assert_eq!(find(&j, "Not installed").unwrap().str_at("line"), Some("Newest version 1.2.60"));
    assert_eq!(find(&j, "Size").unwrap().str_at("line"), Some("182.7 MB to download, 511.9 MB on the drive"));
    assert_eq!(parse(&u.act("store-remove", "com.spotify.Client", "")).unwrap().str_at("toast"), Some("Spotify isn't installed."));
    // An update: both Update and Remove, each asking for its own thing.
    let j = parse(&u.page("store-item", store::HEROIC)).unwrap();
    assert_eq!((titles(&sections(&j)[0].1), titles(&sections(&j)[2].1)), (vec!["Update"], vec!["Remove"]));
    assert_eq!(sections(&j)[0].1[0].str_at("line"), Some("Version 2.18.0 is ready"));
    assert_eq!(parse(&u.act("store-update", store::HEROIC, "")).unwrap().get("dialog").unwrap().str_at("confirm"), Some("Update"));
    u.cancel();
    let d = parse(&u.act("store-remove", store::HEROIC, "")).unwrap();
    assert_eq!((d.get("dialog").unwrap().str_at("confirm"), d.get("dialog").unwrap().get("danger").and_then(Json::as_bool)), (Some("Remove"), Some(true)));
    u.cancel();
    // Nothing known about an app: no size row, and the page still stands. Offline keeps the last facts, aged.
    let j = parse(&u.page("store-item", "org.videolan.VLC")).unwrap();
    assert!(find(&j, "Size").is_none() && find(&j, "Not installed").is_some());
    u.sample_offline();
    assert_eq!(parse(&u.page("store-item", "com.spotify.Client")).unwrap().str_at("note"), Some("Sizes and versions: checked 3 days ago, offline now"));
    // A tile and Decky Loader have pages too.
    let j = parse(&u.page("store-item", "tile:netflix")).unwrap();
    assert_eq!((j.str_at("title"), sections(&j)[0].1[0].str_at("act"), find(&j, "Source").unwrap().str_at("line")), (Some("Netflix"), Some("tile"), Some("https://www.netflix.com")));
    assert_eq!(parse(&u.act("tile-remove", "netflix", "")).unwrap().str_at("toast"), Some("It isn't in your library."));
    let j = parse(&u.page("store-item", "decky")).unwrap();
    assert_eq!((j.str_at("title"), sections(&j)[0].1[0].str_at("button"), find(&j, "Newest").unwrap().str_at("line")), (Some("Decky Loader"), Some("Repair"), Some("Version v3.1.10, as found last. Offline now.")));
    assert_eq!(parse(&u.page("store-item", "com.evil.App")).unwrap().str_at("title"), Some("Not found"));
    done("item");
}

#[test]
fn what_flatpak_says_is_kept_and_an_empty_answer_changes_nothing() {
    let cache = dir("facts");
    let _ = std::fs::remove_dir_all(&cache);
    assert_eq!(store::app_facts(&cache), (None, Default::default()));
    store::remember_installed(&cache, "com.discordapp.Discord\t0.0.98\t318.4\u{a0}MB\ncom.unknown.App\t1\t2 MB\nbroken line\n");
    store::remember_remote(&cache, "com.discordapp.Discord\t0.0.99\t101.5\u{a0}MB\t320.0\u{a0}MB\ncom.spotify.Client\t1.2.60\t182.7 MB\n", 500);
    let (at, apps) = store::app_facts(&cache);
    assert_eq!((at, apps.len()), (Some(500), 2));
    assert_eq!(apps["com.discordapp.Discord"], store::AppFacts { version: "0.0.98".into(), size: "318.4 MB".into(), newest: "0.0.99".into(), download: "101.5 MB".into(), full: "320.0 MB".into() });
    assert_eq!(apps["com.spotify.Client"].full, "");
    // No internet: the answer is empty, and what was found last stays with its age.
    store::remember_remote(&cache, "", 900);
    assert_eq!(store::app_facts(&cache).0, Some(500));
    // An app that was removed loses its installed version, not what Flathub has.
    store::remember_installed(&cache, "");
    let (_, apps) = store::app_facts(&cache);
    assert_eq!((apps["com.discordapp.Discord"].version.as_str(), apps["com.discordapp.Discord"].newest.as_str()), ("", "0.0.99"));
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn windows_saves_go_through_ludusavi_into_games_that_can_take_them() {
    let mut u = ui("winsaves", Preset::Desktop);
    let mount = u.windows_drive().unwrap();
    let j = parse(&u.page("windrive", "")).unwrap();
    let (_, saves) = sections(&j).into_iter().find(|(t, _)| t == "Saves").unwrap();
    assert_eq!((titles(&saves), saves[0].str_at("act")), (vec!["Bring saves over"], Some("win-saves")));
    assert!(saves[0].str_at("line").unwrap().starts_with("Ludusavi is installed first"));
    // Ludusavi comes first when it is missing; then one job with the drive and the games that may take saves.
    let d = parse(&u.act("win-saves", "", "")).unwrap();
    assert!(d.get("dialog").unwrap().str_at("text").unwrap().contains("only read"));
    let plan = u.confirm().unwrap();
    assert!(matches!(&plan.steps[0], Step::Host(c) if c.last().unwrap() == store::LUDUSAVI && c.contains(&"install".to_string())));
    let Some(Step::Saves(Job::Windows { mount: m, targets })) = plan.steps.get(1) else { panic!("no saves job") };
    assert_eq!(*m, mount);
    let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    // Never a game whose anti-cheat runs online.
    assert!(names.contains(&"Baldur's Gate 3") && names.contains(&"Balatro") && !names.contains(&"Elden Ring") && !names.contains(&"Rocket League"));
    let bg3 = targets.iter().find(|t| t.name == "Baldur's Gate 3").unwrap();
    assert_eq!(bg3.windows_dir, Some(mount.join("Program Files (x86)/Steam/steamapps/common/Baldurs Gate 3")));
    assert!(bg3.prefix.as_ref().unwrap().ends_with("compatdata/1086940/pfx") && !bg3.install_dir.starts_with(&mount));
    assert!(targets.iter().all(|t| t.prefix.as_ref().is_none_or(|p| !p.starts_with(&mount))));
    // What the last run did shows on the page, with Undo while there is something to put back.
    assert_eq!(parse(&u.act("win-saves-undo", "", "")).unwrap().str_at("toast"), Some("There is nothing to undo."));
    WindowsRun { at: crate::now() - 120, brought: vec!["Baldur's Gate 3".into()], waiting: vec!["Hades II".into(), "Disco Elysium".into()], titles: vec!["Baldur's Gate 3".into()], kept: Some(u.folder().join("saves/windows/kept-1")), undone: false }.save(&u.folder());
    u.installed.push(store::LUDUSAVI.into());
    let j = parse(&u.page("windrive", "")).unwrap();
    let (_, saves) = sections(&j).into_iter().find(|(t, _)| t == "Saves").unwrap();
    assert_eq!(titles(&saves), vec!["Bring saves over", "Last time: saves of Baldur's Gate 3 brought over", "Undo"]);
    assert!(saves[1].str_at("line").unwrap().starts_with("2 min ago. Kept for later: Hades II, Disco Elysium."));
    let d = parse(&u.act("win-saves-undo", "", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().get("danger").and_then(Json::as_bool), Some(true));
    assert_eq!(u.confirm().unwrap().steps, vec![Step::Saves(Job::WindowsUndo)]);
    // With Ludusavi here the plan is the one job.
    parse(&u.act("win-saves", "", "")).unwrap();
    assert_eq!(u.confirm().unwrap().steps.len(), 1);
    done("winsaves");
}
