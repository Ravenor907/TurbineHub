//! Games, the launch tool, Proton, saves and the Store's search and pages, on the made-up machine.
use super::games::{Switches, matches};
use super::launch::{self, Uses};
use super::selftile;
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
    // The two cloud services first; Windows is on this machine, so restarting to it comes last, away from where
    // the highlight lands.
    assert_eq!(titles(&secs[0].1), vec!["GeForce NOW", "Xbox Cloud", "Restart to Windows"]);
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
    // Play comes first, where the highlight lands.
    let play = &sections(&j)[0].1[0];
    assert_eq!((play.str_at("title"), play.str_at("arg"), play.get("primary").and_then(Json::as_bool)), (Some("Play"), Some("play|steam:1086940"), Some(true)));
    let sw = &sections(&j)[1].1;
    assert_eq!(titles(sw), vec!["On for this game", "FSR 4 upgrade", "Frame generation", "Handheld mode", "Proton log"]);
    assert_eq!(sw[1].get("on").and_then(Json::as_bool), Some(true));
    assert_eq!(parse(&u.act("switch", &format!("{BG3}|log"), "")).unwrap().str_at("toast"), Some("Applies the next time the game starts."));
    // Frame generation says what it needs until the plugin's wrapper is there.
    assert!(sw[2].str_at("line").unwrap().starts_with("Needs the Lossless Scaling plugin"));
    std::fs::write(tool::framegen_wrapper(&u.ctx.home), "#!/bin/sh\nexec \"$@\"\n").unwrap();
    assert_eq!(sections(&parse(&u.page("games", BG3)).unwrap())[1].1[2].str_at("line"), Some("Lossless Scaling, set the multiplier in Decky"));
    // Cyberpunk doesn't use the tool yet: the first row leads to the three steps, and a switch is kept for later.
    let j = parse(&u.page("games", CYBERPUNK)).unwrap();
    let first = &sections(&j)[1].1[0];
    assert_eq!((first.str_at("act"), first.str_at("arg"), first.str_at("button")), (Some("tool-how"), Some(CYBERPUNK), Some("How")));
    assert!(first.str_at("line").unwrap().starts_with("In Steam:"));
    assert_eq!(parse(&u.act("switch", &format!("{CYBERPUNK}|handheld"), "")).unwrap().str_at("toast"), Some("Saved. It applies once this game uses Turbine Hub in Steam."));
    assert!(Switches::load(&u.folder()).on(CYBERPUNK, "handheld"));
    // This handheld's graphics can't run FSR 4: the switch isn't offered, and BG3's (on from before) says so.
    assert!(parse(&u.act("switch", &format!("{CYBERPUNK}|fsr4"), "")).unwrap().str_at("toast").unwrap().contains("RX 9000"));
    assert!(!Switches::load(&u.folder()).on(CYBERPUNK, "fsr4"));
    assert!(sw[1].str_at("line").unwrap().contains("RX 9000"));
    let d = parse(&u.act("tool-how", CYBERPUNK, "")).unwrap();
    let steps = d.get("dialog").unwrap().get("items").unwrap().as_array().to_vec();
    assert_eq!(steps.len(), 4);
    assert!(steps[0].str_at("line").unwrap().contains("Cyberpunk 2077") && steps[2].str_at("line").unwrap().contains("\"Turbine Hub\""));
    assert_eq!(d.get("dialog").unwrap().str_at("confirm"), Some(""));
    // Before Steam has started again, the restart comes first, on the row and in the steps.
    u.steam_started = None;
    assert!(sections(&parse(&u.page("games", CYBERPUNK)).unwrap())[1].1[0].str_at("line").unwrap().starts_with("Restart Steam once"));
    let d = parse(&u.act("tool-how", CYBERPUNK, "")).unwrap();
    assert!(d.get("dialog").unwrap().get("items").unwrap().as_array()[0].str_at("line").unwrap().starts_with("Restart Steam once"));
    // Hollow Knight uses Turbine Mod Manager's tool: no switches here, and it says where they are.
    // Turbine Mod Manager isn't installed here: no button that leads nowhere, and no Mods row at all.
    let j = parse(&u.page("games", HOLLOW)).unwrap();
    let sw = &sections(&j)[1].1;
    assert_eq!(titles(sw), vec!["Uses Turbine Mod Manager in Steam", "Turbine Mod Manager isn't installed"]);
    assert!(rows(&j).iter().all(|r| !r.str_at("arg").unwrap_or("").starts_with("mods|")));
    u.installed.push(store::TURBINE.into());
    let j = parse(&u.page("games", HOLLOW)).unwrap();
    assert_eq!(titles(&sections(&j)[1].1), vec!["Uses Turbine Mod Manager in Steam", "Open Turbine Mod Manager"]);
    assert!(parse(&u.page("games", BG3)).unwrap().to_text().contains("Mods: open Turbine Mod Manager"));
    u.installed.retain(|a| a != store::TURBINE);
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
    assert_eq!(sections(&parse(&u.page("games", CYBERPUNK)).unwrap())[1].1[0].str_at("title"), Some("Update the launch tool"));
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
    let first = sections(&parse(&u.page("games", BG3)).unwrap())[1].1[0].clone();
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
    assert_eq!(titles(&rows(&parse(&u.page("proton", ELDEN)).unwrap())), vec!["Plays as it is"]);
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
    let first = rows(&parse(&u.page("store", "")).unwrap())[0].clone();
    assert_eq!((first.str_at("title"), first.str_at("act"), first.str_at("arg")), (Some("Battle.net"), Some("open"), Some("store-item|nsl")));
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
    assert_eq!(titles(&sections(&j)[0].1), vec!["Open Discord"]);
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
    assert_eq!((titles(&sections(&j)[0].1), titles(&sections(&j)[2].1)), (vec!["Update", "Open Heroic Games Launcher"], vec!["Remove"]));
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
fn turbine_hub_adds_itself_to_the_library_only_while_steam_is_closed() {
    use selftile::State;
    let mut u = ui("selftile", HANDHELD);
    let share = art_set("selftile");
    let (home, folder) = (u.ctx.home.clone(), u.folder());
    let file = home.join(".local/share/Steam/userdata/1001/config/shortcuts.vdf");
    let grid = file.with_file_name("grid");
    assert_eq!(selftile::state(&home), State::Missing);
    let general = rows(&parse(&u.page("settings", "")).unwrap());
    let offer = general.iter().find(|r| r.str_at("title") == Some(selftile::TITLE)).unwrap();
    assert_eq!((offer.str_at("act"), offer.str_at("arg"), offer.str_at("button")), (Some("self-tile"), Some("add"), Some("Add")));
    // While Steam runs there is no dialog to confirm, only the steps; and the step itself refuses too.
    u.steam_running = true;
    let d = parse(&u.act("self-tile", "add", "")).unwrap();
    assert_eq!((d.get("dialog").unwrap().str_at("title"), d.get("dialog").unwrap().str_at("confirm")), (Some("Steam has to be closed for this"), Some("")));
    assert!(u.confirm().is_none());
    assert!(selftile::run_step(&Step::SelfTile { add: true }, &home, &folder, &share, true).unwrap().is_err());
    assert!(!file.exists() && !grid.exists());
    // With Steam closed: one dialog, one step. A cover the player picked is there already and stays.
    u.steam_running = false;
    let appid = selftile::tile().appid();
    std::fs::create_dir_all(&grid).unwrap();
    std::fs::write(grid.join(format!("{appid}p.png")), "the player's own").unwrap();
    let d = parse(&u.act("self-tile", "add", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("confirm"), Some("Add"));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![Step::SelfTile { add: true }]);
    selftile::run_step(&plan.steps[0], &home, &folder, &share, false).unwrap().unwrap();
    let list = crate::shortcuts::parse(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].name.as_str(), list[0].exe.as_str(), list[0].launch_options.as_str(), list[0].appid), ("Turbine Hub", "\"/usr/bin/flatpak\"", "run --branch=stable --arch=x86_64 --command=turbinehub io.github.ravenor907.TurbineHub", appid));
    assert_eq!(crate::shortcuts::find_self(&home).map(|(id, _)| id), Some(appid));
    assert_eq!(std::fs::read_to_string(grid.join(format!("{appid}p.png"))).unwrap(), "the player's own");
    for (name, text) in [(format!("{appid}.png"), "png wide"), (format!("{appid}_hero.png"), "png hero"), (format!("{appid}_logo.png"), "png logo"), (format!("{appid}_icon.png"), "png icon")] {
        assert_eq!(std::fs::read_to_string(grid.join(name)).unwrap(), text);
    }
    assert_eq!(selftile::state(&home), State::Added { ours: true, art_missing: 0 });
    // Adding again adds no second entry; Settings now names it and offers Remove.
    selftile::run_step(&Step::SelfTile { add: true }, &home, &folder, &share, false).unwrap().unwrap();
    assert_eq!(crate::shortcuts::parse(&std::fs::read(&file).unwrap()).unwrap().len(), 1);
    assert_eq!(parse(&u.act("self-tile", "add", "")).unwrap().str_at("toast"), Some("Turbine Hub is in your library already."));
    let general = rows(&parse(&u.page("settings", "")).unwrap());
    assert!(general.iter().any(|r| r.str_at("title") == Some(selftile::TITLE) && r.str_at("badge") == Some("In your library")));
    // A picture cleared in Steam shows as missing, and Fix art puts it back.
    std::fs::remove_file(grid.join(format!("{appid}_hero.png"))).unwrap();
    assert_eq!(selftile::state(&home), State::Added { ours: true, art_missing: 1 });
    parse(&u.act("self-tile", "art", "")).unwrap();
    assert_eq!(u.confirm().unwrap().steps, vec![Step::SelfArt]);
    selftile::run_step(&Step::SelfArt, &home, &folder, &share, true).unwrap().unwrap();
    assert_eq!(selftile::state(&home), State::Added { ours: true, art_missing: 0 });
    // Remove: the steps while Steam runs, else the entry and Turbine Hub's own pictures go; the player's stays.
    u.steam_running = true;
    assert_eq!(parse(&u.act("self-tile", "remove", "")).unwrap().get("dialog").unwrap().str_at("title"), Some("Remove it in Steam"));
    u.steam_running = false;
    let d = parse(&u.act("self-tile", "remove", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().get("danger").and_then(Json::as_bool), Some(true));
    let plan = u.confirm().unwrap();
    selftile::run_step(&plan.steps[0], &home, &folder, &share, false).unwrap().unwrap();
    assert_eq!(selftile::state(&home), State::Missing);
    assert!(grid.join(format!("{appid}p.png")).exists() && !grid.join(format!("{appid}.png")).exists());
    assert!(crate::shortcuts::backup_file(&file).exists());
    // An entry the player added in Steam: Turbine Hub adds only its pictures (not the icon), and never removes it.
    let mut by_hand = selftile::tile();
    by_hand.name = "Hub".into();
    by_hand.launch_options = "run io.github.ravenor907.TurbineHub".into();
    crate::shortcuts::add_shortcut(&file, &by_hand, "run io.github", false).unwrap();
    assert_eq!(selftile::state(&home), State::Added { ours: false, art_missing: 4 });
    selftile::run_step(&Step::SelfTile { add: true }, &home, &folder, &share, false).unwrap().unwrap();
    assert_eq!(selftile::state(&home), State::Added { ours: false, art_missing: 0 });
    assert!(!grid.join(format!("{}_icon.png", by_hand.appid())).exists());
    assert_eq!(parse(&u.act("self-tile", "remove", "")).unwrap().get("dialog").unwrap().str_at("title"), Some("Remove it in Steam"));
    assert!(selftile::run_step(&Step::SelfTile { add: false }, &home, &folder, &share, false).unwrap().is_err());
    assert_eq!(crate::shortcuts::parse(&std::fs::read(&file).unwrap()).unwrap().len(), 1);
    // The made-up PC has it in the library without its pictures: Settings offers Fix art and Remove.
    let mut pc = ui("selftile-pc", Preset::Desktop);
    assert_eq!(selftile::state(&pc.ctx.home), State::Added { ours: true, art_missing: 5 });
    let general = rows(&parse(&pc.page("settings", "")).unwrap());
    let mine: Vec<Option<&str>> = general.iter().filter(|r| r.str_at("act") == Some("self-tile")).map(|r| r.str_at("button")).collect();
    assert_eq!(mine, vec![Some("Fix art"), Some("Remove")]);
    done("selftile");
    done("selftile-pc");
}

#[test]
fn the_first_run_offers_the_library_entry_and_the_checkup_tips_it() {
    // Ticked from the start, and its step comes before the setup is marked done.
    let mut u = Ui::sample(&dir("selfpick"), HANDHELD, Some(Mode::Veteran));
    let all = rows(&parse(&u.page("quick", "")).unwrap());
    let pick = all.iter().find(|r| r.str_at("title") == Some(selftile::TITLE)).unwrap();
    assert_eq!((pick.str_at("kind"), pick.get("on").and_then(Json::as_bool)), (Some("check"), Some(true)));
    parse(&u.act("quick-apply", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    let at = plan.steps.iter().position(|x| *x == Step::SelfTile { add: true }).unwrap();
    assert_eq!(at, plan.steps.len() - 2);
    // While Steam runs it can't be ticked, and says when it can be done.
    let mut r = Ui::sample(&dir("selfpick"), HANDHELD, Some(Mode::Guided));
    r.steam_running = true;
    let all = rows(&parse(&r.page("quick", "")).unwrap());
    let pick = all.iter().find(|x| x.str_at("title") == Some(selftile::TITLE)).unwrap();
    assert_eq!((pick.str_at("kind"), pick.str_at("badge")), (Some("info"), Some("Later, with Steam closed")));
    parse(&r.act("quick-apply", "", "")).unwrap();
    assert!(r.confirm().unwrap().steps.iter().all(|x| !matches!(x, Step::SelfTile { .. })));
    // Guided's picks offer the quick panel's steps, since Decky Loader is here: a How button, never a badge that
    // looks done.
    assert!(all.iter().all(|x| x.str_at("title") != Some(store::PANEL_TITLE)));
    let guided = parse(&r.page("setup", "picks")).unwrap();
    let panel = rows(&guided).into_iter().find(|x| x.str_at("title") == Some(store::PANEL_TITLE)).unwrap();
    assert_eq!((panel.str_at("act"), panel.str_at("button"), panel.get("badge")), (Some("panel-guide"), Some("How"), None));
    assert!(guided.to_text().contains(selftile::TITLE));
    // The checkup tips it while it is missing, and the tip's button is the same Add.
    let m = r.machine();
    let facts = crate::checkup::Facts { not_in_library: true, ..crate::checkup::Facts::default() };
    let tip = crate::checkup::items(&m, &facts).into_iter().find(|i| i.id == "library").unwrap();
    assert_eq!((tip.level, tip.action), (crate::checkup::Level::Tip, ("Add", "self-tile")));
    assert!(crate::checkup::items(&m, &crate::checkup::Facts::default()).iter().all(|i| i.id != "library"));
    assert_eq!(parse(&r.act("checkup", "self-tile", "")).unwrap().get("dialog").unwrap().str_at("title"), Some("Steam has to be closed for this"));
    done("selfpick");
}

#[test]
fn the_quick_panel_is_found_by_its_package_name_and_installed_through_decky() {
    let mut u = ui("panel", HANDHELD);
    let page = parse(&u.page("store", "decky")).unwrap();
    let row = rows(&page).into_iter().find(|r| r.str_at("title") == Some(store::PANEL_TITLE)).unwrap();
    assert_eq!((row.str_at("act"), row.str_at("button")), (Some("panel-guide"), Some("How")));
    let d = parse(&u.act("panel-guide", "", "")).unwrap();
    let items = d.get("dialog").unwrap().get("items").unwrap().as_array().to_vec();
    assert_eq!(items.len(), 6);
    assert_eq!(d.get("dialog").unwrap().get("guide").and_then(Json::as_bool), Some(true));
    assert!(items.iter().any(|i| i.str_at("line") == Some("https://github.com/Ravenor907/TurbineHub/releases/latest/download/TurbineHub-Decky.zip")));
    assert!(items[2].str_at("line").unwrap().contains("Install Plugin from URL"));
    assert_eq!(d.get("dialog").unwrap().str_at("confirm"), Some(""));
    // Whatever its folder is called, its package.json names it. Another plugin's doesn't count.
    assert!(!store::panel_installed(&u));
    let plugins = u.ctx.home.join("homebrew/plugins");
    std::fs::write(plugins.join("decky-css/package.json"), "{\"name\": \"css-loader\"}").unwrap();
    assert!(!store::panel_installed(&u));
    std::fs::create_dir_all(plugins.join("Some Folder")).unwrap();
    std::fs::write(plugins.join("Some Folder/package.json"), "{\n  \"name\": \"turbinehub-decky\",\n  \"version\": \"0.1.0\"\n}").unwrap();
    assert!(store::panel_installed(&u));
    let row = rows(&parse(&u.page("store", "decky")).unwrap()).into_iter().find(|r| r.str_at("title") == Some(store::PANEL_TITLE)).unwrap();
    assert_eq!((row.str_at("kind"), row.str_at("badge")), (Some("info"), Some("Installed")));
    assert_eq!(parse(&u.act("panel-guide", "", "")).unwrap().str_at("toast"), Some("The quick panel is installed already."));
    // Without Decky Loader the steps start with it.
    let mut tv = ui("panel", Preset::TvPc);
    std::fs::remove_dir_all(tv.ctx.home.join("homebrew")).unwrap();
    let d = parse(&tv.act("panel-guide", "", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().get("items").unwrap().as_array()[0].str_at("title"), Some("First"));
    // The name is the one the panel's own package carries.
    let package = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../decky/package.json")).unwrap();
    assert_eq!(parse(&package).unwrap().str_at("name"), Some(store::PANEL_PACKAGE));
    done("panel");
}

#[test]
fn games_that_run_come_first_and_a_game_opens_on_play() {
    let mut u = ui("order", HANDHELD);
    let j = parse(&u.page("games", "")).unwrap();
    // Apex won't run here: it goes to the end, so the list's first highlight is a game that plays.
    let list = side(&j);
    assert_eq!(list.last(), Some(&"Apex Legends"));
    assert_eq!(list[..2], ["Balatro", "Baldur's Gate 3"]);
    assert_eq!(j.get("index").and_then(Json::as_f64), Some(0.0));
    let first = &rows(&j)[0];
    assert_eq!((first.str_at("title"), first.str_at("act"), first.str_at("arg")), (Some("Play"), Some("game"), Some("play|steam:2379780")));
    // Pressing it asks Steam to start the game: never anything that restarts the machine.
    let d = parse(&u.act("game", "play|steam:1086940", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Play Baldur's Gate 3?"));
    assert!(u.confirm().unwrap().steps.iter().all(|s| matches!(s, Step::Host(c) if c.iter().any(|a| a.starts_with("steam://rungameid/")))));
    // Without Windows the page of a game that won't run never mentions restarting to it.
    let mut tv = ui("order-tv", Preset::TvPc);
    assert!(!tv.page("games", APEX).contains("Restart to Windows"));
    done("order");
    done("order-tv");
}

#[test]
fn a_report_says_where_it_is() {
    let mut u = ui("report", HANDHELD);
    parse(&u.act("game", &format!("report|{BG3}"), "")).unwrap();
    let plan = u.confirm().unwrap();
    let dir = super::games::reports_dir(&u);
    assert!(plan.done.contains(&dir.display().to_string()));
    u.play_sample(&plan).unwrap();
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    done("report");
}

#[test]
fn pages_show_what_a_confirmed_dialog_did() {
    let mut u = ui("after", HANDHELD);
    // A Proton pick: the page shows it in use, with Undo.
    parse(&u.act("proton-pick", &format!("{BG3}|GE-Proton10-4"), "")).unwrap();
    let plan = u.confirm().unwrap();
    u.play_sample(&plan).unwrap();
    let j = parse(&u.page("proton", BG3)).unwrap();
    assert_eq!(rows(&j)[0].str_at("title"), Some("Starts with GE-Proton10-4"));
    assert!(rows(&j).iter().any(|r| r.str_at("title") == Some("GE-Proton10-4") && r.str_at("badge") == Some("In use")));
    assert!(rows(&j).iter().any(|r| r.str_at("act") == Some("proton-undo")));
    // Back up now adds a snapshot; a restore leaves the Undo row.
    u.installed.push(store::LUDUSAVI.into());
    parse(&u.act("saves", &format!("backup|{BG3}"), "")).unwrap();
    let plan = u.confirm().unwrap();
    u.play_sample(&plan).unwrap();
    let secs = sections(&parse(&u.page("saves", BG3)).unwrap());
    assert_eq!(secs[1].0, "Snapshots \u{b7} 4");
    let oldest = secs[1].1.last().unwrap().str_at("arg").unwrap().to_string();
    parse(&u.act("saves", &oldest, "")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!(plan.done, "Save restored. Undo is on this page.");
    u.play_sample(&plan).unwrap();
    assert!(titles(&sections(&parse(&u.page("saves", BG3)).unwrap())[0].1).contains(&"Undo the last restore"));
    // An app updated: no "Update ready" any more, on its page, in the Store and on Home.
    assert!(u.page("store-item", store::HEROIC).contains("Update ready"));
    parse(&u.act("store-update", store::HEROIC, "")).unwrap();
    let plan = u.confirm().unwrap();
    u.play_sample(&plan).unwrap();
    assert!(!u.page("store-item", store::HEROIC).contains("Update ready"));
    assert!(u.updates.is_empty() && u.facts.updates.is_empty());
    // An install is remembered as Turbine Hub's own, and Review changes offers to remove it.
    parse(&u.act("store", "com.spotify.Client", "")).unwrap();
    let plan = u.confirm().unwrap();
    u.play_sample(&plan).unwrap();
    assert!(store::is_installed(&u, "com.spotify.Client"));
    assert!(rows(&parse(&u.page("changes", "")).unwrap()).iter().any(|r| r.str_at("act") == Some("store-remove") && r.str_at("arg") == Some("com.spotify.Client")));
    done("after");
}

#[test]
fn review_changes_lists_what_turbine_hub_did_in_the_players_folders() {
    let mut u = ui("review", HANDHELD);
    tool::set_proton_pick(&u.folder(), "1086940", Some(&u.ctx.home.join(".local/share/Steam/compatibilitytools.d/GE-Proton10-4"))).unwrap();
    let j = parse(&u.page("changes", "")).unwrap();
    let secs = sections(&j);
    assert_eq!(secs.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), vec!["System changes, made with your password", "In your own folders, no password"]);
    let own = &secs[1].1;
    let acts: Vec<(&str, &str)> = own.iter().map(|r| (r.str_at("act").unwrap(), r.str_at("arg").unwrap())).collect();
    assert!(acts.contains(&("tool", "remove")));
    assert!(acts.contains(&("proton-pick", "steam:1086940|")));
    assert!(acts.contains(&("open", "games|steam:1086940")));
    assert!(acts.contains(&("agent", "")));
    // Undo of a Proton pick goes back to Automatic, through the usual dialog.
    let d = parse(&u.act("proton-pick", "steam:1086940|", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Use Automatic for Baldur's Gate 3?"));
    done("review");
}

#[test]
fn installed_apps_open_from_their_page() {
    let mut u = ui("open", HANDHELD);
    let j = parse(&u.page("store-item", "com.discordapp.Discord")).unwrap();
    let first = &rows(&j)[0];
    assert_eq!((first.str_at("act"), first.str_at("button")), (Some("store-open"), Some("Open")));
    let a = parse(&u.act("store-open", "com.discordapp.Discord", "")).unwrap();
    let line: Vec<&str> = a.get("run").unwrap().as_array().iter().filter_map(Json::as_str).collect();
    assert_eq!(line, vec!["flatpak-spawn", "--host", "flatpak", "run", "com.discordapp.Discord"]);
    assert!(parse(&u.page("store-item", "com.spotify.Client")).unwrap().to_text().find("store-open").is_none());
    assert!(parse(&u.act("store-open", "com.spotify.Client", "")).unwrap().get("run").is_none());
    done("open");
}

#[test]
fn screens_are_read_from_desktop_modes_settings_and_game_mode_is_told_where_to_go() {
    let text = "\u{1b}[01;32mOutput: \u{1b}[0;0m1 eDP-1\n\tenabled\n\tModes:  0:1280x800@90*!  1:1280x800@60\n\tVrr: incapable\n\tHDR: incapable\nOutput: 2 DP-4\n\tenabled\n\tModes:  0:2560x1440@239.97!  1:2560x1440@59.95*\n\tVrr: Automatic\n\tHDR: disabled\n";
    let s = crate::checkup::screens_from_kscreen(text);
    assert_eq!(s.len(), 2);
    assert_eq!((s[0].name.as_str(), s[0].hz, s[0].vrr.as_str(), s[0].hdr), ("eDP-1", Some(90), "", None));
    assert_eq!((s[1].name.as_str(), s[1].hz, s[1].vrr.as_str(), s[1].hdr), ("DP-4", Some(60), "automatic", Some(false)));
    // The PC's monitor page marks what is set now.
    let mut u = ui("screens", Preset::Desktop);
    let j = parse(&u.page("module:monitors", "")).unwrap();
    let pick = |t: &str| rows(&j).into_iter().find(|r| r.str_at("title") == Some(t)).unwrap().get("index").and_then(Json::as_f64);
    assert_eq!((pick("Variable refresh (VRR)"), pick("HDR")), (Some(1.0), Some(0.0)));
    // Home's Switch leads to Steam's own settings in Game Mode, and to Monitors in Desktop Mode.
    let item = crate::checkup::items(&u.machine(), &u.facts).into_iter().find(|i| i.id == "refresh").unwrap();
    assert_eq!(item.action, ("Switch", "refresh-rate"));
    assert!(parse(&u.act("checkup", "refresh-rate", "")).unwrap().get("dialog").is_some());
    u.window = true;
    assert_eq!(parse(&u.act("checkup", "refresh-rate", "")).unwrap().str_at("open"), Some("module:monitors"));
    done("screens");
}
