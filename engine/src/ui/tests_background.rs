//! The screens' side of the background helper: its switch, and what Home, Battery, Sleep, Lighting and the Dock show
//! from what it wrote down.
use super::machine::{COLORS, Lighting};
use super::plan::{Ask, Step};
use super::*;
use crate::agent::{self, Report};
use crate::fake::Preset;
use crate::json::parse;

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-ui-bg-{tag}-{}", std::process::id()))
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

fn row_titled(j: &Json, title: &str) -> Json {
    rows(j).into_iter().find(|r| r.str_at("title") == Some(title)).unwrap_or_else(|| panic!("no row {title}"))
}

/// As on a machine where the background helper was never turned on.
fn without_agent(u: &mut Ui) {
    u.agent_on = Some(false);
    let _ = std::fs::remove_dir_all(&u.data);
    u.absorb();
}

fn install_helper(u: &Ui) {
    let bin = u.ctx.root.join(crate::helper::BIN.trim_start_matches('/'));
    std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
    std::fs::write(bin, "").unwrap();
}

#[test]
fn the_background_helper_is_a_switch_with_its_exact_steps() {
    let mut u = ui("switch", HANDHELD, Mode::Guided);
    let page = parse(&u.page("settings", "general")).unwrap();
    let r = row_titled(&page, "Background helper");
    assert_eq!((r.str_at("kind"), r.get("on").and_then(Json::as_bool), r.str_at("badge")), (Some("toggle"), Some(true), Some("Running")));
    assert!(rows(&page).iter().all(|r| r.str_at("badge") != Some("Not in this build")));
    // On: the press offers to turn it off, with the commands that do.
    let j = parse(&u.act("agent", "", "")).unwrap();
    assert_eq!(j.get("dialog").unwrap().str_at("title"), Some("Turn the background helper off?"));
    // Cancel is the highlighted button, and Guided mode shows no file path.
    assert_eq!(j.get("dialog").unwrap().get("danger").and_then(Json::as_bool), Some(true));
    assert!(!j.get("dialog").unwrap().get("items").unwrap().as_array()[0].str_at("line").unwrap().contains('/'));
    let plan = u.confirm().unwrap();
    assert_eq!((plan.ask, plan.steps.clone()), (Ask::None, agent::turn_off(&u.ctx.home)));
    // Off: it turns on by writing its unit into the player's own folder, never with a password.
    without_agent(&mut u);
    let page = parse(&u.page("settings", "general")).unwrap();
    let r = row_titled(&page, "Background helper");
    assert_eq!((r.get("on").and_then(Json::as_bool), r.str_at("badge")), (Some(false), Some("")));
    let j = parse(&u.act("agent", "", "")).unwrap();
    let d = j.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.str_at("ask"), d.str_at("confirm")), (Some("Turn the background helper on?"), Some(""), Some("Turn on")));
    // The unit's path only in Veteran mode.
    let line = d.get("items").unwrap().as_array()[0].str_at("line").unwrap().to_string();
    assert_eq!(line.ends_with(".config/systemd/user/turbinehub-agent.service"), u.veteran(), "{line}");
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, agent::turn_on(&u.ctx.home));
    assert!(matches!(&plan.steps[1], Step::HostWrite { value, .. } if value.contains("--agent")));
    assert_eq!(plan.helper_jobs(), 0);
    // Set to start but not writing: said so instead of pretending.
    u.agent_on = Some(true);
    assert_eq!(row_titled(&parse(&u.page("settings", "general")).unwrap(), "Background helper").str_at("badge"), Some("Not running"));
    // Before the host answered, a status written a moment ago is enough to call it on.
    u.agent_on = None;
    assert!(!u.agent_running());
    Report { at: crate::now(), ..Report::default() }.save(&u.data);
    assert!(u.agent_running());
    Report { at: crate::now() - agent::ALIVE_SECS - 5, ..Report::default() }.save(&u.data);
    assert!(!u.agent_running());
    // An update of Turbine Hub starts the background helper again as the new version.
    u.agent_on = Some(true);
    u.update = Some("0.2".into());
    u.act("update-app", "", "");
    assert_eq!(u.confirm().unwrap().steps, vec![Step::SelfUpdate, Step::Host(agent::restart())]);
    // Removing Turbine Hub takes the background helper with it.
    u.agent_on = Some(true);
    u.act("remove-app", "", "");
    let plan = u.confirm().unwrap();
    let off = agent::turn_off(&u.ctx.home);
    let at = plan.steps.iter().position(|x| *x == off[0]).unwrap();
    assert_eq!(plan.steps[at..at + 3], off[..]);
    // The launch tool goes first (it stops everything while a game still uses it), the recorded changes are undone
    // (with the helper set up once more when it isn't there) and the app goes last.
    assert_eq!(plan.steps[..2], [Step::HelperInstall, Step::Tool { install: false }]);
    assert!(plan.steps.iter().any(|x| matches!(x, Step::Host(c) if c.last().unwrap() == "uninstall")));
    assert!(matches!(plan.steps.last(), Some(Step::Host(c)) if c.contains(&"--delete-data".to_string())));
    assert!(plan.danger);
    done("switch");
}

#[test]
fn the_save_time_machine_needs_ludusavi_and_says_when_it_waits() {
    let mut u = ui("timemachine", HANDHELD, Mode::Guided);
    let j = parse(&u.act("snapshots", "", "")).unwrap();
    assert_eq!((j.str_at("toast"), j.str_at("open")), (Some("Saves need Ludusavi. Install it from Store, Essentials."), Some("store")));
    assert!(!u.settings().save_snapshots);
    u.installed.push(store::LUDUSAVI.into());
    let j = parse(&u.act("snapshots", "", "")).unwrap();
    assert!(u.settings().save_snapshots && j.get("toast").is_none());
    assert_eq!(row_titled(&parse(&u.page("settings", "")).unwrap(), "Save time machine").str_at("badge"), Some(""));
    // Turning it off never needs anything.
    u.installed.clear();
    u.act("snapshots", "", "");
    assert!(!u.settings().save_snapshots);
    u.installed.push(store::LUDUSAVI.into());
    without_agent(&mut u);
    assert_eq!(parse(&u.act("snapshots", "", "")).unwrap().str_at("toast"), Some("Snapshots start once the background helper is on."));
    assert_eq!(row_titled(&parse(&u.page("settings", "")).unwrap(), "Save time machine").str_at("badge"), Some("Needs the background helper"));
    done("timemachine");
}

#[test]
fn home_shows_time_left_and_what_the_background_helper_found() {
    let mut u = ui("home", HANDHELD, Mode::Guided);
    let j = parse(&u.page("home", "")).unwrap();
    let tile = &j.get("tiles").unwrap().as_array()[0];
    // 46 Wh at about 14 W.
    assert_eq!((tile.str_at("value"), tile.str_at("label")), (Some("78%"), Some("Battery, about 3 h 15 min left")));
    let items = j.get("items").unwrap().as_array();
    let sleep = items.iter().find(|i| i.str_at("id") == Some("sleep")).unwrap();
    assert_eq!((sleep.str_at("title"), sleep.str_at("line")), (Some("Sleep used 6% battery an hour"), Some("Built-in controller woke the machine. Stop it from waking it?")));
    // Charging: no promise about time left.
    u.readings.as_mut().unwrap().charging = true;
    assert_eq!(parse(&u.page("home", "")).unwrap().get("tiles").unwrap().as_array()[0].str_at("label"), Some("Battery, charging"));
    u.readings.as_mut().unwrap().charging = false;
    // Without the background helper there is still an answer from the reading of this moment, and no sleep item.
    without_agent(&mut u);
    let j = parse(&u.page("home", "")).unwrap();
    assert_eq!(j.get("tiles").unwrap().as_array()[0].str_at("label"), Some("Battery, about 3 h 14 min left"));
    assert!(j.get("items").unwrap().as_array().iter().all(|i| i.str_at("id") != Some("sleep")));
    assert_eq!(j.str_at("headline"), Some("One thing could use a look"));
    // A battery that doesn't say how much is in it, and no history: just the charge.
    u.readings.as_mut().unwrap().battery_wh = None;
    assert_eq!(parse(&u.page("home", "")).unwrap().get("tiles").unwrap().as_array()[0].str_at("label"), Some("Battery"));
    done("home");
}

#[test]
fn updates_the_background_helper_found_show_until_the_app_asks_itself() {
    let mut u = ui("updates", Preset::Desktop, Mode::Guided);
    let now = crate::now();
    Report { at: now, version: "0.1".into(), game: None, update: Some("0.2".into()), updates: vec![store::HEROIC.into(), "com.discordapp.Discord".into(), "org.unknown.App".into()], plugins: vec!["CSS Loader".into()], checked: now - 60 }.save(&u.data);
    agent::save_drift(&u.data, now, "3.9.6", &["Kernel setting vm.swappiness set to 100 isn't in effect".to_string()]);
    assert!(u.absorb());
    assert!(!u.absorb());
    assert_eq!(u.facts.updates, vec!["Discord", "Heroic Games Launcher"]);
    assert_eq!((u.update.as_deref(), u.updates.len(), u.facts.drift.len()), (Some("0.2"), 3, 1));
    u.home_all = true;
    let j = parse(&u.page("home", "")).unwrap();
    let items = j.get("items").unwrap().as_array();
    let by_id = |id: &str| items.iter().find(|i| i.str_at("id") == Some(id)).unwrap_or_else(|| panic!("no {id}"));
    assert_eq!((by_id("plugins").str_at("title"), by_id("plugins").str_at("line")), (Some("1 plugin update ready"), Some("CSS Loader")));
    assert_eq!((by_id("app").str_at("title"), by_id("app").get("look").and_then(Json::as_bool)), (Some("Turbine Hub 0.2 is ready"), Some(false)));
    assert!(by_id("drift").get("look").and_then(Json::as_bool) == Some(true));
    // Each of them leads somewhere: a guide for plugins (Decky updates them), the update dialog for the app.
    let guide = parse(&u.act("checkup", by_id("plugins").str_at("opens").unwrap(), "")).unwrap();
    assert_eq!(guide.get("dialog").unwrap().str_at("title"), Some("Updating Decky plugins"));
    let update = parse(&u.act("checkup", by_id("app").str_at("opens").unwrap(), "")).unwrap();
    assert_eq!(update.get("dialog").unwrap().str_at("title"), Some("Update to Turbine Hub 0.2?"));
    u.cancel();
    // The newest version of everything: nothing is claimed.
    Report { at: now, update: Some(String::new()), checked: now, ..Report::default() }.save(&u.data);
    u.absorb();
    assert_eq!((u.update.clone(), u.updates.len(), u.facts.plugin_updates.len()), (None, 0, 0));
    // Once the app asked the host itself, the background helper's older lists don't replace the answer.
    u.updates_own = true;
    u.drift_own = true;
    u.updates = vec![store::HEROIC.into()];
    u.facts.drift.clear();
    assert!(!u.absorb());
    assert_eq!((u.updates.len(), u.facts.drift.len()), (1, 0));
    done("updates");
}

#[test]
fn battery_and_sleep_pages_list_the_history() {
    let mut u = ui("history", HANDHELD, Mode::Guided);
    let page = parse(&u.page("module:battery", "")).unwrap();
    let left = row_titled(&page, "Time left");
    assert_eq!(left.str_at("value"), Some("About 3 h 15 min"));
    let games = page.get("sections").unwrap().as_array()[1].clone();
    assert_eq!(games.str_at("title"), Some("Battery use by game"));
    let list = games.get("rows").unwrap().as_array();
    assert_eq!(list.iter().map(|r| (r.str_at("title").unwrap(), r.str_at("value").unwrap())).collect::<Vec<_>>(), vec![("Cyberpunk 2077", "44% an hour"), ("Baldur's Gate 3", "34% an hour"), ("Hollow Knight", "18% an hour"), ("Balatro", "12% an hour")]);
    assert_eq!(list[1].str_at("line"), Some("2 h 54 min on battery, 2 sessions"));
    let page = parse(&u.page("module:sleep", "")).unwrap();
    let sleeps = page.get("sections").unwrap().as_array()[1].clone();
    assert_eq!(sleeps.str_at("title"), Some("Last sleeps"));
    let list = sleeps.get("rows").unwrap().as_array();
    assert_eq!(list.len(), 3);
    assert_eq!((list[0].str_at("title"), list[0].str_at("line"), list[0].str_at("value")), (Some("Asleep 7 h"), Some("42% used, 6.0% an hour \u{b7} woken by Built-in controller"), Some("9 h ago")));
    assert_eq!(list[1].str_at("line"), Some("Plugged in"));
    // No history yet: the pages say why instead of showing nothing.
    without_agent(&mut u);
    let page = parse(&u.page("module:battery", "")).unwrap();
    assert!(page.get("sections").unwrap().as_array()[1].str_at("empty").unwrap().starts_with("The background helper keeps this history."));
    assert!(parse(&u.page("module:sleep", "")).unwrap().get("sections").unwrap().as_array()[1].str_at("empty").unwrap().starts_with("The background helper keeps this history."));
    u.agent_on = Some(true);
    assert!(parse(&u.page("module:sleep", "")).unwrap().get("sections").unwrap().as_array()[1].str_at("empty").unwrap().starts_with("Nothing yet."));
    // A desktop's Sleep page has no battery matters on it.
    let mut d = ui("history", Preset::Desktop, Mode::Guided);
    assert_eq!(parse(&d.page("module:sleep", "")).unwrap().get("sections").unwrap().as_array().len(), 1);
    done("history");
}

#[test]
fn lighting_and_dock_follow_the_background_helper() {
    let mut u = ui("lights", HANDHELD, Mode::Guided);
    // Running, but the lights' files aren't open to it yet: the page offers the one-time setup.
    let page = parse(&u.page("module:lighting", "")).unwrap();
    assert_eq!(rows(&page)[0].str_at("act"), Some("light-test"));
    assert_eq!(row_titled(&page, "Color").get("index").and_then(Json::as_f64), Some(0.0));
    // Colors per game: while a game runs, the color picked is that game's and the page says whose it is.
    u.act("light", "pergame", "");
    Report { at: crate::now(), game: Some(("steam:1086940".into(), "Baldur's Gate 3".into(), crate::now() - 600)), ..Report::default() }.save(&u.data);
    u.act("light", "color", "4");
    let l = Lighting::load(&u.folder());
    assert_eq!((l.color.as_str(), l.game_colors.get("steam:1086940").map(String::as_str)), (COLORS[0].1, Some(COLORS[4].1)));
    assert_eq!(Lighting::parse(&l.to_json()), l);
    let page = parse(&u.page("module:lighting", "")).unwrap();
    assert_eq!(row_titled(&page, "Color for Baldur's Gate 3").get("index").and_then(Json::as_f64), Some(4.0));
    // No game running: the color is the one for everything else.
    Report { at: crate::now(), ..Report::default() }.save(&u.data);
    u.act("light", "color", "2");
    assert_eq!(Lighting::load(&u.folder()).color, COLORS[2].1);
    assert!(Lighting::parse(r##"{"gameColors":{"steam:1":"#123456","steam:2":"#FFFFFF","steam:3":7}}"##).game_colors.keys().eq(["steam:2"]));
    // The Dock card says what docking changes.
    let card = |u: &mut Ui| parse(&u.page("machine", "")).unwrap().get("cards").unwrap().as_array().iter().find(|c| c.str_at("id") == Some("dock")).unwrap().str_at("value").unwrap().to_string();
    assert_eq!(card(&mut u), "Nothing changes");
    u.act("light", "dockoff", "");
    u.act("light", "docksound", "");
    assert_eq!(card(&mut u), "Lights off, sound to the TV");
    // With the background helper off, both pages say so first, with the way to Settings.
    without_agent(&mut u);
    for p in ["module:lighting", "module:dock"] {
        let first = rows(&parse(&u.page(p, "")).unwrap())[0].clone();
        assert_eq!((first.str_at("title"), first.str_at("act"), first.str_at("arg")), (Some("The background helper is off"), Some("open"), Some("settings")), "{p}");
    }
    done("lights");
}

#[test]
fn game_boost_says_what_it_still_needs() {
    let mut u = ui("boost", HANDHELD, Mode::Veteran);
    // No privileged helper yet: turning Game Boost on offers to set it up, with the one password.
    let j = parse(&u.act("boost", "", "")).unwrap();
    assert_eq!(j.get("dialog").unwrap().str_at("ask"), Some("current"));
    // Cancelled: it stays off.
    u.cancel();
    assert!(!u.settings().game_boost);
    parse(&u.act("boost", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![Step::HelperInstall, Step::Choice { key: "boost", on: true }]);
    // Only once the dialog's steps worked does it turn on (the app does the last step).
    assert!(!u.settings().game_boost);
    let mut st = u.settings();
    super::plan::set_choice(&mut st, "boost", true);
    u.save(&st);
    assert!(u.settings().game_boost);
    assert_eq!(row_titled(&parse(&u.page("module:power", "")).unwrap(), "Game Boost").str_at("badge"), Some("Not active yet"));
    // Off again needs nothing.
    assert!(parse(&u.act("boost", "", "")).unwrap().get("dialog").is_none());
    install_helper(&u);
    let j = parse(&u.act("boost", "", "")).unwrap();
    assert!(j.get("dialog").is_none() && j.get("toast").is_none());
    assert_eq!(row_titled(&parse(&u.page("module:power", "")).unwrap(), "Game Boost").str_at("badge"), Some(""));
    u.act("boost", "", "");
    without_agent(&mut u);
    let j = parse(&u.act("boost", "", "")).unwrap();
    assert_eq!((j.str_at("toast"), j.str_at("open")), (Some("Game Boost starts once the background helper is on. Settings, General."), Some("settings")));
    done("boost");
}
