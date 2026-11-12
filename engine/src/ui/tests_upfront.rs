//! Everything up front: My machine as one page of sections with its controls on it, Home's quick switches in both
//! modes, the power modes, the battery's history as charts, every check on request and Start's fix for all.
use super::lighting::Lighting;
use super::plan::{Ask, Step};
use super::*;
use crate::fake::Preset;
use crate::helper::{Change, Setting};
use crate::json::parse;

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-upfront-{tag}-{}", std::process::id()))
}

fn ui(tag: &str, preset: Preset, mode: Mode) -> Ui {
    Ui::sample(&dir(tag), preset, Some(mode))
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

fn list(j: &Json, key: &str) -> Vec<Json> {
    j.get(key).map(Json::as_array).unwrap_or(&[]).to_vec()
}

fn ids(j: &Json, key: &str) -> Vec<String> {
    list(j, key).iter().map(|x| x.str_at("id").unwrap_or("").to_string()).collect()
}

fn rows(j: &Json) -> Vec<Json> {
    list(j, "sections").iter().flat_map(|sec| list(sec, "rows")).collect()
}

fn by_id(items: &[Json], id: &str) -> Json {
    items.iter().find(|x| x.str_at("id") == Some(id)).cloned().unwrap_or_else(|| panic!("no {id} among {:?}", items.iter().map(|x| x.str_at("id").unwrap_or("").to_string()).collect::<Vec<_>>()))
}

fn page(u: &mut Ui, name: &str, arg: &str) -> Json {
    parse(&u.page(name, arg)).unwrap()
}

fn act(u: &mut Ui, action: &str, arg: &str, value: &str) -> Json {
    parse(&u.act(action, arg, value)).unwrap()
}

#[test]
fn my_machine_is_one_page_with_a_section_for_each_part() {
    let mut u = ui("sections", HANDHELD, Mode::Guided);
    let j = page(&mut u, "machine", "");
    assert_eq!(j.str_at("section"), Some("overview"));
    assert_eq!(ids(&j, "chips"), ["overview", "power", "display", "lighting", "battery", "sleep", "tuneup", "dock", "windows"]);
    let titles: Vec<String> = list(&j, "chips").iter().map(|c| c.str_at("title").unwrap().to_string()).collect();
    assert_eq!(titles, ["Overview", "Power", "Screen", "Lighting", "Battery", "Sleep and wake", "Tune-up", "Dock", "Windows"]);
    assert_eq!(list(&j, "chips").iter().filter(|c| c.get("on").and_then(Json::as_bool) == Some(true)).count(), 1);
    // The highlight starts on the first card, and Overview's cards are the sections, in order.
    assert_eq!(j.str_at("focus"), Some("card:power"));
    assert_eq!(ids(&j, "cards"), ["power", "display", "lighting", "battery", "sleep", "tuneup", "dock", "windows"]);
    // A section is the same page with its controls on it, whichever way it was asked for.
    let battery = page(&mut u, "machine", "battery");
    assert_eq!(battery.str_at("section"), Some("battery"));
    assert_eq!((battery.str_at("title"), battery.str_at("focus")), (Some("Battery"), Some("charge")));
    assert!(battery.get("cards").is_none() && !rows(&battery).is_empty());
    let linked = page(&mut u, "module:battery", "");
    assert_eq!(linked.get("sections"), battery.get("sections"));
    assert_eq!(linked.get("chips"), battery.get("chips"));
    // (A link keeps working after the section was changed on it.)
    assert_eq!(page(&mut u, "module:battery", "tuneup").str_at("section"), Some("tuneup"));
    // A section this machine doesn't have is Overview.
    assert_eq!(page(&mut u, "machine", "gpu").str_at("section"), Some("overview"));
    assert_eq!(page(&mut u, "module:nothing", "").str_at("section"), Some("overview"));
    // On every machine: each section starts the highlight on one of its own rows, has rows, and offers its
    // long explanation in Options.
    for (preset, mode) in [(HANDHELD, Mode::Guided), (Preset::Desktop, Mode::Veteran), (Preset::TvPc, Mode::Guided)] {
        let mut u = ui("sections", preset, mode);
        let chips = ids(&page(&mut u, "machine", ""), "chips");
        assert!(chips.len() >= 7, "{chips:?}");
        for id in chips.iter().skip(1) {
            let j = page(&mut u, "machine", id);
            let all = rows(&j);
            assert!(!all.is_empty(), "{id} has nothing on it");
            let focus = j.str_at("focus").unwrap();
            assert!(all.iter().any(|r| r.str_at("id") == Some(focus)), "{id}: the highlight starts on {focus:?}, which isn't a row");
            let about = list(&j, "options").into_iter().find(|o| o.str_at("act") == Some("about")).unwrap_or_else(|| panic!("{id} has no About"));
            assert_eq!(about.str_at("arg"), Some(id.as_str()));
            let guide = act(&mut u, "about", id, "");
            let d = guide.get("dialog").unwrap();
            assert!(d.get("guide").and_then(Json::as_bool) == Some(true) && !list(d, "items").is_empty() && !d.str_at("text").unwrap().is_empty(), "{id}");
            assert_eq!(Some(d.str_at("title").unwrap()), list(&j, "chips").iter().find(|c| c.str_at("id") == Some(id.as_str())).and_then(|c| c.str_at("title")));
        }
    }
    // A TV PC without Windows has no Windows section; a desktop has its own parts.
    let mut tv = ui("sections", Preset::TvPc, Mode::Guided);
    assert_eq!(ids(&page(&mut tv, "machine", ""), "chips"), ["overview", "gpu", "cpu", "monitors", "drives", "sound", "sleep", "tuneup"]);
    assert_eq!(by_id(&list(&page(&mut tv, "machine", ""), "chips"), "monitors").str_at("title"), Some("TV"));
    // Help for a section is about that section.
    let help = page(&mut u, "help", "machine@tuneup");
    assert!(help.str_at("intro").unwrap().starts_with("Memory and kernel settings"));
    assert_eq!(help.str_at("title"), Some("Tune-up"));
    assert!(page(&mut u, "help", "machine@").str_at("intro").unwrap().contains("LT and RT"));
    done("sections");
}

#[test]
fn overview_cards_carry_the_main_switch_of_their_part() {
    let mut u = ui("cards", HANDHELD, Mode::Guided);
    let cards = list(&page(&mut u, "machine", ""), "cards");
    let flag = |c: &Json, k: &str| c.get(k).and_then(Json::as_bool);
    // Game Boost, the lights and the charge limit are switches on their cards.
    let power = by_id(&cards, "power");
    assert_eq!((power.str_at("status"), flag(&power, "toggle"), flag(&power, "on"), power.str_at("act")), (Some("Game Boost"), Some(true), Some(false), Some("boost")));
    let lights = by_id(&cards, "lighting");
    assert_eq!((lights.str_at("status"), flag(&lights, "toggle"), flag(&lights, "on"), lights.str_at("act"), lights.str_at("arg")), (Some("Lights"), Some(true), Some(true), Some("light"), Some("power")));
    let battery = by_id(&cards, "battery");
    assert_eq!((battery.str_at("status"), flag(&battery, "on"), battery.str_at("act"), battery.str_at("value")), (Some("Stop at 80%"), Some(true), Some("charge-quick"), Some("Stops at 80%")));
    // The tune-up and Windows have their button; the others open their section.
    let tune = by_id(&cards, "tuneup");
    assert_eq!((tune.str_at("button"), tune.str_at("act"), tune.str_at("arg")), (Some("Turn on all 5"), Some("tuneup"), Some("all")));
    assert_eq!(by_id(&cards, "windows").str_at("button"), Some("Restart"));
    let screen = by_id(&cards, "display");
    assert_eq!((screen.str_at("act"), screen.str_at("arg"), flag(&screen, "toggle")), (Some("open"), Some("module:display"), None));
    // Every card says in its Options what its section holds, and a card that is a switch says how to get there.
    for c in &cards {
        assert!(!list(c, "offers").is_empty() || c.str_at("id") == Some("display"), "{:?}", c.str_at("id"));
    }
    assert_eq!(list(&power, "offers")[0].str_at("title"), Some("Go to Power"));
    assert!(list(&tune, "offers").iter().any(|o| o.str_at("act") == Some("tuneup") && o.str_at("arg") != Some("all")));
    // One press: the lights go off and keep their brightness for when they come back.
    assert!(act(&mut u, "light", "power", "").get("refresh").is_some());
    let l = Lighting::load(&u.folder());
    assert_eq!((l.brightness, l.lit), (0, 60));
    let off = by_id(&list(&page(&mut u, "machine", ""), "cards"), "lighting");
    assert_eq!((flag(&off, "on"), off.str_at("value")), (Some(false), Some("Off")));
    u.act("light", "power", "");
    assert_eq!(Lighting::load(&u.folder()).brightness, 60);
    // The brightness slider keeps what the switch comes back to, and a file from before the switch reads fine.
    u.act("light", "brightness", "30");
    u.act("light", "power", "");
    u.act("light", "power", "");
    assert_eq!(Lighting::load(&u.folder()).brightness, 30);
    assert_eq!(Lighting::parse(r#"{"brightness":0}"#).lit, 60);
    // One press: the charge limit goes to 100% and back to 80%, each through its own dialog.
    let d = act(&mut u, "charge-quick", "", "");
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Charge to 100%?"));
    let plan = u.confirm().unwrap();
    assert_eq!((plan.steps.first(), plan.helper_jobs(), plan.ask), (Some(&Step::HelperInstall), 1, Ask::Current));
    // A machine without a battery has no such press.
    let mut d = ui("cards", Preset::Desktop, Mode::Guided);
    assert!(act(&mut d, "charge-quick", "", "").entries().is_empty());
    let cpu = by_id(&list(&page(&mut d, "machine", ""), "cards"), "cpu");
    assert_eq!((cpu.str_at("title"), cpu.str_at("status"), cpu.str_at("act")), (Some("Processor"), Some("Game Boost"), Some("boost")));
    assert_eq!(by_id(&list(&page(&mut d, "machine", ""), "cards"), "sound").str_at("value"), Some("Headset"));
    done("cards");
}

#[test]
fn switches_that_change_the_system_are_marked_until_the_helper_is_set_up() {
    let mark = Some("Needs your password");
    let mut u = ui("marks", HANDHELD, Mode::Guided);
    let badge = |j: &Json, id: &str| by_id(&rows(j), id).str_at("badge").map(String::from);
    let power = page(&mut u, "machine", "power");
    for id in ["tdp", "boost", "power-mode"] {
        assert_eq!(badge(&power, id).as_deref(), mark, "{id}");
    }
    assert_eq!(badge(&page(&mut u, "machine", "battery"), "charge").as_deref(), mark);
    assert_eq!(badge(&page(&mut u, "machine", "windows"), "windows-boot").as_deref(), mark);
    assert!(rows(&page(&mut u, "machine", "sleep")).iter().filter(|r| r.str_at("act") == Some("wake")).all(|r| r.str_at("badge") == mark));
    // A tune-up switch that has something of its own to say keeps saying it.
    let tune = rows(&page(&mut u, "machine", "tuneup"));
    assert!(tune.iter().any(|r| r.str_at("badge") == mark) && tune.iter().all(|r| r.str_at("kind") != Some("toggle") || !r.str_at("badge").unwrap_or("").is_empty()));
    let cards = list(&page(&mut u, "machine", ""), "cards");
    for id in ["power", "battery", "tuneup", "windows"] {
        assert_eq!(by_id(&cards, id).str_at("badge"), mark, "{id}");
    }
    // The lights and the dock change nothing of the system: no mark.
    assert_eq!(by_id(&cards, "lighting").str_at("badge"), Some(""));
    assert!(rows(&page(&mut u, "machine", "dock")).iter().all(|r| r.str_at("badge").is_none()));
    let home = list(&page(&mut u, "home", ""), "switches");
    assert_eq!(home.iter().filter(|r| r.str_at("badge") == mark).map(|r| r.str_at("id").unwrap()).collect::<Vec<_>>(), ["power-mode", "boost", "charge-quick"]);
    // With the helper set up nothing asks, so nothing is marked.
    u.sample_with_helper();
    for (name, arg) in [("machine", ""), ("machine", "power"), ("machine", "battery"), ("machine", "sleep"), ("machine", "tuneup"), ("machine", "windows"), ("home", "")] {
        assert!(!u.page(name, arg).contains("Needs your password"), "{name} {arg}");
    }
    done("marks");
}

#[test]
fn power_modes_go_through_the_helper_and_show_the_one_in_effect() {
    let mut u = ui("modes", HANDHELD, Mode::Guided);
    let mode = by_id(&list(&page(&mut u, "home", ""), "switches"), "power-mode");
    assert_eq!((mode.str_at("kind"), mode.str_at("title"), mode.get("index").and_then(Json::as_f64)), (Some("choice"), Some("Power mode"), Some(-1.0)));
    assert_eq!(list(&mode, "options").iter().map(|o| o.as_str().unwrap().to_string()).collect::<Vec<_>>(), ["Quiet", "Balanced", "Boost"]);
    // The same choice is in the Power section, and Veteran sees each mode's limit.
    assert_eq!(by_id(&rows(&page(&mut u, "machine", "power")), "power-mode").str_at("title"), Some("Mode"));
    let mut v = ui("modes-v", HANDHELD, Mode::Veteran);
    let words = list(&by_id(&list(&page(&mut v, "home", ""), "switches"), "power-mode"), "options");
    assert_eq!(words.iter().map(|o| o.as_str().unwrap().to_string()).collect::<Vec<_>>(), ["Quiet \u{b7} 14 W", "Balanced \u{b7} 21 W", "Boost \u{b7} 35 W"]);
    done("modes-v");
    // Picking one is a dialog with the exact changes, the helper's setup first, and five helper jobs.
    let d = act(&mut u, "power-mode", "", "0");
    let dialog = d.get("dialog").unwrap();
    assert_eq!(dialog.str_at("title"), Some("Switch to Quiet?"));
    let items: Vec<String> = list(dialog, "items").iter().map(|i| i.str_at("title").unwrap().to_string()).collect();
    assert!(items.iter().any(|t| t == "Processor speed preference: balanced \u{2192} power"), "{items:?}");
    assert!(items.iter().any(|t| t == "Power limit: 17 W \u{2192} 14 W"), "{items:?}");
    let plan = u.confirm().unwrap();
    assert_eq!((plan.steps.first(), plan.helper_jobs(), plan.ask, plan.done.as_str()), (Some(&Step::HelperInstall), 5, Ask::Current, "Quiet mode on"));
    // A mode that isn't one, and a machine that has none, change nothing.
    assert!(act(&mut u, "power-mode", "", "7").entries().is_empty());
    // Once the kernel holds a mode's values the choice shows it, and picking it again says so.
    u.sample_power_mode("quiet");
    let mode = by_id(&list(&page(&mut u, "home", ""), "switches"), "power-mode");
    assert_eq!((mode.get("index").and_then(Json::as_f64), mode.str_at("line")), (Some(0.0), Some("Cool and quiet, the battery lasts longest.")));
    assert_eq!(act(&mut u, "power-mode", "", "0").str_at("toast"), Some("That's already set."));
    assert_eq!(act(&mut u, "power-mode", "", "2").get("dialog").unwrap().str_at("title"), Some("Switch to Boost?"));
    assert_eq!(u.confirm().unwrap().helper_jobs(), 5);
    // A desktop has the modes without a limit: it starts on Balanced.
    let mut d = ui("modes", Preset::Desktop, Mode::Guided);
    let mode = by_id(&rows(&page(&mut d, "machine", "cpu")), "power-mode");
    assert_eq!(mode.get("index").and_then(Json::as_f64), Some(1.0));
    act(&mut d, "power-mode", "", "2");
    assert_eq!(d.confirm().unwrap().helper_jobs(), 2);
    done("modes");
}

#[test]
fn home_has_the_quick_switches_in_both_modes() {
    let mut u = ui("quick", HANDHELD, Mode::Guided);
    let j = page(&mut u, "home", "");
    assert_eq!(ids(&j, "switches"), ["power-mode", "boost", "light:power", "charge-quick"]);
    // Guided keeps the rest one press away, on the same page.
    assert!(list(&j, "extra").is_empty());
    assert_eq!(j.str_at("moreSwitches"), Some("More switches"));
    assert!(list(&j, "options").iter().any(|o| o.str_at("act") == Some("home-more")));
    assert!(act(&mut u, "home-more", "", "").get("refresh").is_some());
    let j = page(&mut u, "home", "");
    assert_eq!(j.str_at("moreSwitches"), Some("Fewer switches"));
    assert_eq!(ids(&j, "extra"), ["open:module:display", "open:module:sleep", "open:module:tuneup", "open:module:dock", "open:module:windows"]);
    // A way to a section says in its Options what the section holds.
    assert!(list(&by_id(&list(&j, "extra"), "open:module:tuneup"), "offers").iter().any(|o| o.str_at("act") == Some("tuneup")));
    // Each is a row whose press is the one its own page makes.
    let sw = list(&j, "switches");
    let lights = by_id(&sw, "light:power");
    assert_eq!((lights.str_at("kind"), lights.get("on").and_then(Json::as_bool), lights.str_at("line")), (Some("toggle"), Some(true), Some("Static, 60%")));
    let charge = by_id(&sw, "charge-quick");
    assert_eq!((charge.str_at("title"), charge.get("on").and_then(Json::as_bool)), (Some("Stop charging at 80%"), Some(true)));
    // Veteran shows them all at once.
    let mut v = ui("quick-v", HANDHELD, Mode::Veteran);
    let all = ids(&page(&mut v, "home", ""), "switches");
    assert_eq!(all, ["power-mode", "boost", "light:power", "charge-quick", "open:module:display", "open:module:sleep", "open:module:tuneup", "open:module:dock", "open:module:windows"]);
    done("quick-v");
    // A desktop has no lights and no battery, and FSR 4 for every game where its card runs it.
    let mut d = ui("quick", Preset::Desktop, Mode::Veteran);
    let all = ids(&page(&mut d, "home", ""), "switches");
    assert_eq!(&all[..3], ["power-mode", "boost", "fsr4all"]);
    assert!(!all.iter().any(|x| x == "light:power" || x == "charge-quick"));
    // (Guided has it up front too, there; a handheld whose graphics can't run it never sees it.)
    let mut g = ui("quick-g", Preset::Desktop, Mode::Guided);
    assert_eq!(ids(&page(&mut g, "home", ""), "switches"), ["power-mode", "boost", "fsr4all"]);
    done("quick-g");
    // Lights left to another app have no switch here.
    let mut l = Lighting::load(&u.folder());
    l.huesync = true;
    l.save(&u.folder());
    assert!(!ids(&page(&mut u, "home", ""), "switches").iter().any(|x| x == "light:power"));
    // The battery tile carries its charge over the last twelve hours.
    let tile = by_id(&list(&j, "tiles"), "battery");
    let spark = list(&tile, "spark");
    assert_eq!(spark.len(), 24);
    assert!(spark.iter().filter(|p| p.as_f64().is_some()).count() >= 12 && spark.contains(&Json::Null));
    assert!(by_id(&list(&j, "tiles"), "cpu").get("spark").is_none());
    done("quick");
}

#[test]
fn the_battery_section_charts_the_last_day() {
    let mut u = ui("charts", HANDHELD, Mode::Guided);
    let j = page(&mut u, "machine", "battery");
    let day = list(&j, "sections").into_iter().find(|sec| sec.str_at("title") == Some("The last day")).unwrap();
    let charts = list(&day, "rows");
    assert_eq!(charts.iter().map(|r| (r.str_at("kind").unwrap(), r.str_at("title").unwrap())).collect::<Vec<_>>(), [("chart", "Charge, last 24 hours"), ("chart", "Processor temperature, last 24 hours")]);
    let charge = &charts[0];
    let points = list(charge, "points");
    assert_eq!(points.len(), 48);
    // The seven hours asleep are a gap, never a line.
    assert!(points.iter().filter(|p| **p == Json::Null).count() >= 12);
    assert!(points.iter().filter_map(Json::as_f64).all(|v| (0.0..=100.0).contains(&v)));
    assert_eq!((charge.get("min").and_then(Json::as_f64), charge.get("max").and_then(Json::as_f64), charge.str_at("low"), charge.str_at("high"), charge.str_at("from"), charge.str_at("to")), (Some(0.0), Some(100.0), Some("0%"), Some("100%"), Some("24 hours ago"), Some("Now")));
    // The newest value and the range are said in words too.
    assert_eq!(charge.str_at("value"), Some("78%"));
    assert_eq!(charge.str_at("line"), Some("Between 50% and 100%. A gap is a sleep."));
    let temp = &charts[1];
    assert_eq!((temp.str_at("value"), temp.str_at("low"), temp.str_at("high")), (Some("58\u{b0}C"), Some("30\u{b0}C"), Some("80\u{b0}C")));
    // Without readings there is nothing to draw, and nothing is drawn.
    let _ = std::fs::remove_dir_all(u.data.join("history"));
    let j = page(&mut u, "machine", "battery");
    assert!(list(&j, "sections").iter().all(|sec| sec.str_at("title") != Some("The last day")));
    assert!(by_id(&list(&page(&mut u, "home", ""), "tiles"), "battery").get("spark").is_none());
    done("charts");
}

#[test]
fn every_check_shows_on_request_and_start_fixes_what_it_can() {
    let mut u = ui("checks", HANDHELD, Mode::Guided);
    let j = page(&mut u, "home", "");
    assert!(list(&j, "passed").is_empty());
    assert!(j.str_at("more").unwrap().ends_with("checks passed."), "{:?}", j.str_at("more"));
    // One thing can be fixed with a yes: Start says its own words.
    let primary = j.get("primary").unwrap();
    assert_eq!((primary.str_at("label"), primary.str_at("act"), primary.str_at("k")), (Some("Update all"), Some("checkup-fix"), Some("start")));
    assert_eq!(act(&mut u, "checkup-fix", "", "").get("dialog").unwrap().str_at("title"), Some("Update everything?"));
    u.cancel();
    // Show all checks: what was looked at and found in order, and never something that is in the list above.
    u.act("home-all", "", "");
    let j = page(&mut u, "home", "");
    let passed: Vec<String> = list(&j, "passed").iter().map(|p| p.str_at("title").unwrap().to_string()).collect();
    assert!(passed.iter().any(|t| t == "Turbine Hub is up to date") && passed.iter().any(|t| t == "The battery stops charging at 80%"), "{passed:?}");
    let found = ids(&j, "items");
    assert!(ids(&j, "passed").iter().all(|id| !found.contains(id)));
    assert!(!ids(&j, "passed").iter().any(|id| id == "updates" || id == "sleep"));
    assert!(list(&j, "options").iter().any(|o| o.str_at("title") == Some("Show fewer checks")));
    // Decky Loader's files are there but it doesn't run: a checkup item with Repair, and Start fixes both.
    u.sample_decky_stopped();
    let j = page(&mut u, "home", "");
    let decky = by_id(&list(&j, "items"), "decky");
    assert_eq!((decky.str_at("title"), decky.str_at("button"), decky.get("look").and_then(Json::as_bool)), (Some("Decky Loader is missing after a SteamOS update"), Some("Repair"), Some(true)));
    assert!(!ids(&j, "passed").iter().any(|id| id == "decky" || id == "plugins"));
    assert_eq!(act(&mut u, "checkup", "decky-repair", "").get("dialog").unwrap().str_at("title"), Some("Repair Decky Loader?"));
    u.cancel();
    assert_eq!(j.get("primary").unwrap().str_at("label"), Some("Fix all 2"));
    let d = act(&mut u, "checkup-fix", "", "");
    let dialog = d.get("dialog").unwrap();
    assert_eq!((dialog.str_at("title"), dialog.str_at("ask")), (Some("Fix 2 things?"), Some("current")));
    let titles: Vec<String> = list(dialog, "items").iter().map(|i| i.str_at("title").unwrap().to_string()).collect();
    assert_eq!(titles, ["Set up Turbine Hub's helper", "Decky Loader is missing after a SteamOS update", "1 update ready"]);
    let plan = u.confirm().unwrap();
    // The helper is set up once, then Decky Loader's install and the apps' update.
    assert_eq!(plan.steps.iter().filter(|x| **x == Step::HelperInstall).count(), 1);
    assert_eq!(plan.steps[0], Step::HelperInstall);
    let lines: Vec<String> = plan.steps.iter().filter_map(|x| if let Step::Host(c) = x { Some(c.join(" ")) } else { None }).collect();
    assert!(lines.iter().any(|c| c.contains("decky-install")) && lines.iter().any(|c| c.contains("flatpak") && c.contains("update")), "{lines:?}");
    assert!(plan.needs_internet());
    // With the helper there the same press asks for nothing.
    u.sample_with_helper();
    assert_eq!(act(&mut u, "checkup-fix", "", "").get("dialog").unwrap().str_at("ask"), Some(""));
    u.cancel();
    // Plugins the player turned off here are not a thing to repair.
    let mut l = u.ledger();
    l.record(Change::Set(Setting::Service { unit: "plugin_loader.service".into(), enabled: false }), Vec::new(), crate::now(), "3.9.5");
    std::fs::write(u.ctx.root.join(crate::helper::LEDGER.trim_start_matches('/')), l.to_text()).unwrap();
    assert!(!ids(&page(&mut u, "home", ""), "items").iter().any(|id| id == "decky"));
    // Running, it is one of the checks that passed.
    let mut r = ui("checks-r", HANDHELD, Mode::Veteran);
    r.host_lights.asked = true;
    r.host_lights.decky_running = true;
    r.act("home-all", "", "");
    let j = page(&mut r, "home", "");
    assert!(list(&j, "passed").iter().any(|p| p.str_at("title") == Some("Decky Loader is running")));
    // Nothing to fix by itself: no main button, and the press says so.
    r.updates.clear();
    r.facts.updates.clear();
    assert!(page(&mut r, "home", "").get("primary").is_none_or(|p| p.str_at("act").is_none()));
    assert!(act(&mut r, "checkup-fix", "", "").str_at("toast").unwrap().starts_with("Nothing here can be fixed in one go"));
    done("checks-r");
    done("checks");
}
