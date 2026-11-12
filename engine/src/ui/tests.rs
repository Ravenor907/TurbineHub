use super::games::{Switches, reset_windows_files};
use super::machine::{Lighting, light_writes};
use super::plan::{Ask, Step};
use super::*;
use crate::fake::Preset;
use crate::json::parse;
use std::path::Path;

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-ui-{tag}-{}", std::process::id()))
}

fn ui(tag: &str, preset: Preset, mode: Mode) -> Ui {
    Ui::sample(&dir(tag), preset, Some(mode))
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

const PAGES: &[&str] = &["home", "machine", "games", "library", "store", "store-item", "tool", "proton", "saves", "settings", "bring", "windrive", "winapps", "wheres", "help", "changes", "pick", "setup", "quick", "module:power", "module:cpu", "module:display", "module:lighting", "module:battery", "module:sleep", "module:tuneup", "module:dock", "module:gpu", "module:monitors", "module:drives", "module:sound", "module:windows"];

/// Every string anywhere in a page.
fn strings(j: &Json, out: &mut Vec<String>) {
    match j {
        Json::Str(t) => out.push(t.clone()),
        Json::Arr(a) => a.iter().for_each(|x| strings(x, out)),
        Json::Obj(o) => o.iter().filter(|(k, _)| !["id", "act", "arg", "icon", "kind"].contains(&k.as_str())).for_each(|(_, v)| strings(v, out)),
        _ => {}
    }
}

/// Every row of a page's sections.
fn rows(j: &Json) -> Vec<Json> {
    j.get("sections").map(Json::as_array).unwrap_or(&[]).iter().flat_map(|sec| sec.get("rows").map(Json::as_array).unwrap_or(&[]).to_vec()).collect()
}

#[test]
fn every_page_is_valid_and_plainly_worded() {
    for (preset, mode) in [(HANDHELD, Mode::Guided), (HANDHELD, Mode::Veteran), (Preset::Desktop, Mode::Guided), (Preset::TvPc, Mode::Veteran)] {
        let mut u = ui("pages", preset, mode);
        assert!(parse(&u.state()).is_ok());
        for sections in ["", "apps", "decky", "launchers", "tv", "emulation", "installed", "look", "updates", "setup", "help", "about", "machine", "password", "games", "bring", "picks", "steam:1086940", "steam:367520", "steam:1172470", "steam:1245620", "com.discordapp.Discord", "com.spotify.Client", "tile:netflix", "nsl"] {
            for p in PAGES {
                let text = u.page(p, sections);
                let j = parse(&text).unwrap_or_else(|e| panic!("{p} {sections}: {e}"));
                let mut all = Vec::new();
                strings(&j, &mut all);
                for t in &all {
                    assert!(!t.contains(['\u{2013}', '\u{2014}']), "{p}: dash in {t:?}");
                    // The glossary on Where's my...? names the technical words to explain them.
                    if mode == Mode::Guided && *p != "wheres" && !t.contains('/') && !t.contains("com.") {
                        assert!(crate::glossary::jargon(t).is_empty(), "{p}: jargon in {t:?}");
                    }
                }
            }
        }
    }
    done("pages");
}

#[test]
fn every_row_does_something() {
    for (preset, mode) in [(HANDHELD, Mode::Veteran), (Preset::Desktop, Mode::Guided)] {
        for p in PAGES {
            for arg in ["", "decky", "tv", "launchers", "updates", "setup", "about", "look", "steam:1086940", "steam:1091500", "steam:1172470", "com.discordapp.Discord", "com.spotify.Client", "tile:netflix"] {
                let mut u = ui("rows", preset, mode);
                let page = parse(&u.page(p, arg)).unwrap();
                for r in rows(&page) {
                    let (act, a, kind) = (r.str_at("act").unwrap_or(""), r.str_at("arg").unwrap_or(""), r.str_at("kind").unwrap_or(""));
                    if ["none", "search", "update-check"].contains(&act) || kind == "info" {
                        continue;
                    }
                    let value = if kind == "slider" { r.get("value").and_then(Json::as_f64).unwrap_or(0.0).to_string() } else { "1".into() };
                    let answer = parse(&u.act(act, a, &value)).unwrap();
                    assert!(!answer.entries().is_empty(), "{p} {arg}: {act} {a} did nothing");
                    u.cancel();
                }
            }
        }
    }
    done("rows");
}

#[test]
fn names_are_short_and_fit_the_machine() {
    assert_eq!(cpu_short("13th Gen Intel(R) Core(TM) i9-13900K"), "Core i9-13900K");
    assert_eq!(cpu_short("AMD Ryzen AI Z2 Extreme"), "Ryzen AI Z2 Extreme");
    assert_eq!(cpu_short("AMD Ryzen 7 7840U w/ Radeon 780M Graphics"), "Ryzen 7 7840U");
    let u = ui("names", Preset::Desktop, Mode::Guided);
    assert_eq!(top_name(&u.machine()), "Desktop PC \u{b7} Core i9-13900K \u{b7} RX 9070 XT");
    let u = ui("names", Preset::TvPc, Mode::Guided);
    assert_eq!(top_name(&u.machine()), "Living room PC \u{b7} RX 9070 XT");
    assert_eq!(parse(&u.state()).unwrap().str_at("kind"), Some("tv"));
    let u = ui("names", HANDHELD, Mode::Guided);
    assert_eq!(top_name(&u.machine()), u.machine().name());
    done("names");
}

#[test]
fn freshness_words_never_hide_what_is_cached() {
    let day = 86_400;
    assert_eq!(checked_words(10 * day, Some(10 * day - 60), false), "Checked today");
    assert_eq!(checked_words(10 * day, Some(9 * day), false), "Checked yesterday");
    assert_eq!(checked_words(10 * day, Some(7 * day), true), "Checked 3 days ago, offline now");
    assert_eq!(checked_words(10 * day, None, true), "Not checked yet, offline now");
    assert_eq!(ago(1000, 990), "now");
    assert_eq!(ago(10_000, 10_000 - 7200), "2 h");
    // Offline, the library check still lists every game with its last known label.
    let mut u = ui("fresh", HANDHELD, Mode::Guided);
    let before = rows(&parse(&u.page("library", "")).unwrap()).len();
    u.sample_offline();
    let page = parse(&u.page("library", "")).unwrap();
    assert_eq!(rows(&page).len(), before);
    assert_eq!(page.str_at("fresh"), Some("Checked 3 days ago, offline now"));
    // The Store keeps its list and says it is offline; a machine that never fetched anything says so per game
    // instead of calling games unrated.
    let store = parse(&u.page("store", "apps")).unwrap();
    assert!(rows(&store).len() >= 5 && store.get("header").unwrap().get("chips").unwrap().as_array()[0].str_at("text").unwrap().starts_with("Offline now"));
    std::fs::remove_dir_all(&u.cache).unwrap();
    let page = parse(&u.page("library", "")).unwrap();
    assert_eq!(page.str_at("fresh"), Some("Not checked yet, offline now"));
    assert!(rows(&page).iter().skip(1).all(|r| r.str_at("line").unwrap().contains("Waiting for the ratings")));
    done("fresh");
}

#[test]
fn home_shows_problems_first_and_setup_until_done() {
    let mut u = ui("home", HANDHELD, Mode::Guided);
    let j = parse(&u.page("home", "")).unwrap();
    assert_eq!(j.str_at("view"), Some("guided"));
    assert_eq!(j.str_at("headline"), Some("Two things could use a look"));
    let items = j.get("items").unwrap().as_array();
    assert!(items.len() <= 3 && items[0].get("look").and_then(Json::as_bool) == Some(true));
    let setup = j.get("setup").unwrap();
    assert_eq!((setup.get("show").and_then(Json::as_bool), setup.get("done").and_then(Json::as_f64)), (Some(true), Some(3.0)));
    assert_eq!(j.get("tiles").unwrap().as_array()[0].str_at("value"), Some("78%"));
    // Show all lists the tips too.
    u.act("home-all", "", "");
    assert!(parse(&u.page("home", "")).unwrap().get("items").unwrap().as_array().len() >= items.len());
    let mut v = ui("home", HANDHELD, Mode::Veteran);
    let j = parse(&v.page("home", "")).unwrap();
    assert_eq!(j.str_at("view"), Some("veteran"));
    let sw = j.get("switches").unwrap().as_array();
    assert_eq!(sw.len(), 8);
    assert_eq!(sw[0].str_at("id"), Some("boost"));
    assert!(!j.get("activity").unwrap().as_array().is_empty());
    // A desktop window gets the mouse tip, and a desktop has no battery tile.
    let mut d = ui("home", Preset::Desktop, Mode::Guided);
    d.window = true;
    let j = parse(&d.page("home", "")).unwrap();
    assert_eq!(j.get("tip").unwrap().str_at("title"), Some("Mouse and keyboard"));
    assert!(j.get("tiles").unwrap().as_array().iter().all(|t| t.str_at("id") != Some("battery")));
    done("home");
}

#[test]
fn locked_games_get_no_switches_and_no_changes() {
    let mut u = ui("locked", HANDHELD, Mode::Guided);
    let j = parse(&u.page("games", "steam:1245620")).unwrap();
    assert_eq!(j.get("header").unwrap().str_at("title"), Some("Elden Ring"));
    let all = rows(&j);
    assert!(all.iter().all(|r| !["switch"].contains(&r.str_at("act").unwrap_or(""))));
    assert!(all.iter().all(|r| !r.str_at("arg").unwrap_or("").starts_with("reset|") && !r.str_at("arg").unwrap_or("").starts_with("mods|")));
    let side = j.get("side").unwrap().as_array();
    let elden = side.iter().find(|x| x.str_at("id") == Some("steam:1245620")).unwrap();
    // It plays: the list says Works, with no lock that would read as "can't play".
    assert_eq!((elden.str_at("chip"), elden.get("lock").and_then(Json::as_bool)), (Some("Works"), Some(false)));
    assert!(!j.to_text().contains("Locked"));
    assert_eq!(all[0].str_at("title"), Some("Play"));
    // Asking anyway is refused and nothing is written.
    let before = Switches::load(&u.folder());
    let answer = parse(&u.act("switch", "steam:1245620|fsr4", "")).unwrap();
    assert!(answer.str_at("toast").unwrap().contains("anti-cheat"));
    assert_eq!(Switches::load(&u.folder()), before);
    assert!(!before.games.contains_key("steam:1245620"));
    assert!(parse(&u.act("game", "reset|steam:1245620", "")).unwrap().get("dialog").is_none());
    // An unlocked game takes the switch.
    u.act("switch", "steam:1086940|handheld", "");
    assert!(Switches::load(&u.folder()).on("steam:1086940", "handheld"));
    let j = parse(&u.page("games", "steam:1086940")).unwrap();
    assert!(rows(&j).iter().any(|r| r.str_at("arg") == Some("steam:1086940|handheld") && r.get("on").and_then(Json::as_bool) == Some(true)));
    done("locked");
}

#[test]
fn switches_file_round_trips_and_drops_unknown_words() {
    let mut sw = Switches::default();
    assert_eq!(sw.toggle("steam:1", "fsr4", false), Ok(true));
    assert_eq!(sw.toggle("steam:1", "framegen", false), Ok(true));
    assert!(sw.toggle("steam:1", "rm", false).is_err());
    assert!(sw.toggle("steam:2", "fsr4", true).is_err());
    assert_eq!(Switches::parse(&sw.to_json()), sw);
    assert_eq!(sw.toggle("steam:1", "fsr4", false), Ok(false));
    assert_eq!(sw.toggle("steam:1", "framegen", false), Ok(false));
    assert!(sw.games.is_empty());
    assert_eq!(Switches::parse(r#"{"fsr4All":true,"games":{"steam:1":{"switches":["fsr4","evil"]},"steam:2":{"switches":[]}}}"#).games.len(), 1);
    assert_eq!(Switches::parse("junk"), Switches::default());
}

#[test]
fn library_check_counts_orders_filters_and_fixes() {
    // The handheld's Radeon 890M can't run FSR 4: a fix that is only that switch isn't offered, nor the switch.
    let mut h = ui("library-h", HANDHELD, Mode::Veteran);
    let j = parse(&h.page("library", "")).unwrap();
    let tiles: Vec<&str> = j.get("tiles").unwrap().as_array().iter().map(|t| t.str_at("value").unwrap()).collect();
    assert_eq!(tiles, vec!["6", "0", "1", "1"]);
    assert!(j.get("primary").unwrap() == &Json::Null);
    assert!(rows(&parse(&h.page("games", "steam:1091500")).unwrap()).iter().all(|r| r.str_at("arg") != Some("steam:1091500|fsr4")));
    assert!(parse(&h.act("switch", "steam:1091500|fsr4", "")).unwrap().str_at("toast").unwrap().contains("RX 9000"));
    assert!(parse(&h.page("home", "")).unwrap().to_text().find("fsr4all").is_none());
    done("library-h");
    // The desktop's RX 9070 XT runs it.
    let mut u = ui("library", Preset::Desktop, Mode::Guided);
    let j = parse(&u.page("library", "")).unwrap();
    let tiles: Vec<&str> = j.get("tiles").unwrap().as_array().iter().map(|t| t.str_at("value").unwrap()).collect();
    assert_eq!(tiles, vec!["5", "1", "1", "1"]);
    let all = rows(&j);
    let names: Vec<&str> = all.iter().skip(1).map(|r| r.str_at("title").unwrap()).collect();
    assert_eq!(names.first(), Some(&"Baldur's Gate 3"));
    assert_eq!(&names[names.len() - 2..], &["Cyberpunk 2077", "Apex Legends"]);
    assert_eq!(j.get("primary").unwrap().str_at("label"), Some("Fix all 1"));
    u.act("filter", "library", "2");
    let j = parse(&u.page("library", "")).unwrap();
    assert_eq!(rows(&j).iter().skip(1).map(|r| r.str_at("title").unwrap()).collect::<Vec<_>>(), vec!["Apex Legends"]);
    // The one-tap fix turns the switch on.
    let answer = parse(&u.act("fix-all", "", "")).unwrap();
    assert!(answer.str_at("toast").unwrap().starts_with("Fix on"));
    assert!(Switches::load(&u.folder()).on("steam:1091500", "fsr4"));
    done("library");
}

#[test]
fn system_changes_ask_first_and_install_the_helper_once() {
    let mut u = ui("plan", HANDHELD, Mode::Guided);
    let j = parse(&u.act("tuneup", "all", "")).unwrap();
    let d = j.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.str_at("ask"), d.str_at("confirm")), (Some("Turn on 5 tune-up items?"), Some("current"), Some("Turn on")));
    // Only what changes is listed, the restart one says so, and Guided mode shows no keys.
    let items = d.get("items").unwrap().as_array();
    assert_eq!(items.len(), 6);
    assert!(items.iter().any(|i| i.str_at("line").unwrap().ends_with("Takes effect at the next restart.")));
    assert!(items.iter().all(|i| !i.str_at("title").unwrap().contains("vm.") && !i.str_at("line").unwrap().contains("vm.")));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps[0], Step::HelperInstall);
    assert!(plan.helper_jobs() >= 5);
    assert!(plan.steps[1..].iter().all(|x| matches!(x, Step::Host(c) if c[..3] == ["flatpak-spawn", "--host", "pkexec"] && c[4] == "apply")));
    assert!(u.confirm().is_none());
    // With the helper there, no password is asked.
    std::fs::create_dir_all(u.ctx.root.join("var/lib/turbinehub/bin")).unwrap();
    std::fs::write(u.ctx.root.join(crate::helper::BIN.trim_start_matches('/')), "").unwrap();
    let j = parse(&u.act("charge", "", "4")).unwrap();
    assert_eq!(j.get("dialog").unwrap().str_at("ask"), Some(""));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.ask, Ask::None);
    assert!(matches!(&plan.steps[0], Step::Host(c) if c.last().unwrap().contains("charge_control_end_threshold") && c.last().unwrap().contains("\"100\"")));
    // Turning off something the record holds becomes undo jobs, newest first.
    parse(&u.act("tuneup", "swap", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps.iter().map(|x| match x { Step::Host(c) => c[4..].join(" "), _ => String::new() }).collect::<Vec<_>>(), vec!["undo {\"id\":2}", "undo {\"id\":1}"]);
    // Without a password on the machine the same dialog sets one first, in either mode, then uses it for the helper.
    for mode in [Mode::Guided, Mode::Veteran] {
        let mut n = ui("plan", HANDHELD, mode);
        n.facts.has_password = Some(false);
        let j = parse(&n.act("tuneup", "all", "")).unwrap();
        assert_eq!(j.get("dialog").unwrap().str_at("ask"), Some("new"));
        assert!(j.get("open").is_none());
        let plan = n.confirm().unwrap();
        assert_eq!(plan.steps[..3], [Step::SetPassword, Step::Done { setup: Some("password"), brought: None }, Step::HelperInstall]);
    }
    done("plan");
}

#[test]
fn power_and_wake_controls_stay_inside_the_helper_lists() {
    let mut u = ui("power", HANDHELD, Mode::Veteran);
    let page = parse(&u.page("module:power", "")).unwrap();
    let slider = &rows(&page)[0];
    assert_eq!((slider.str_at("title"), slider.get("min").and_then(Json::as_f64), slider.get("max").and_then(Json::as_f64), slider.get("value").and_then(Json::as_f64)), (Some("TDP limit"), Some(7.0), Some(35.0), Some(17.0)));
    parse(&u.act("tdp", "", "99")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!(plan.helper_jobs(), 3);
    let mut d = ui("power", Preset::Desktop, Mode::Guided);
    let page = parse(&d.page("module:sleep", "")).unwrap();
    assert_eq!(rows(&page).len(), 2);
    let j = parse(&d.act("wake", "3554:fa09", "")).unwrap();
    assert!(j.get("dialog").unwrap().str_at("title").unwrap().starts_with("Stop "));
    assert!(parse(&d.act("wake", "dead:beef", "")).unwrap().entries().is_empty());
    let j = parse(&d.act("windows-boot", "", "")).unwrap();
    assert_eq!(j.get("dialog").unwrap().get("danger").and_then(Json::as_bool), Some(true));
    let plan = d.confirm().unwrap();
    assert!(matches!(plan.steps.last(), Some(Step::Host(c)) if c.ends_with(&["systemctl".to_string(), "reboot".to_string()])));
    done("power");
}

#[test]
fn lighting_is_clamped_saved_and_written_per_ring() {
    let l = Lighting::parse(r##"{"effect":"Disco","speed":900,"brightness":-4,"color":"red","rainbow":true}"##);
    assert_eq!((l.effect.as_str(), l.speed, l.brightness, l.color.as_str(), l.rainbow), ("Static", 100, 0, "#6FAE8C", true));
    // A saved color stays what it is, brass included.
    assert_eq!(Lighting::parse(r##"{"color":"#C99E52"}"##).color, "#C99E52");
    assert_eq!((super::machine::COLORS[0], super::machine::COLORS[1]), (("Jade", "#6FAE8C"), ("Brass", "#C99E52")));
    assert_eq!(Lighting::parse(&l.to_json()), l);
    let mut u = ui("light", HANDHELD, Mode::Guided);
    u.act("light", "effect", "8");
    u.act("light", "brightness", "40");
    u.act("light", "color", "2");
    let l = Lighting::load(&u.folder());
    assert_eq!((l.effect.as_str(), l.brightness, l.rgb()), ("Spiral", 40, (0xE0, 0x56, 0x4A)));
    let writes = light_writes(&u.ctx.root, &u.machine(), &l);
    let led = u.machine().rgb_leds[0].clone();
    // Four parts of red, green and blue each; the brightness is in the colors, so the file is at full.
    assert_eq!(writes.len(), 2);
    assert_eq!((writes[0].0.clone(), writes[0].1.split(' ').filter(|v| v.parse::<u8>().is_ok()).count()), (format!("/sys/class/leds/{led}/multi_intensity"), 12));
    assert_eq!(writes[1], (format!("/sys/class/leds/{led}/brightness"), "255".to_string()));
    // The first test asks for the access rule (and the helper); the writes come after it.
    parse(&u.act("light-test", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps[0], Step::HelperInstall);
    assert!(matches!(plan.steps.last(), Some(Step::HostWrite { path, .. }) if path.ends_with("/brightness")));
    done("light");
}

#[test]
fn windows_drive_picks_copy_only_what_can_run_and_never_write_to_it() {
    let mut u = ui("drive", Preset::Desktop, Mode::Guided);
    let j = parse(&u.page("windrive", "")).unwrap();
    let all = rows(&j);
    // Baldur's Gate 3 is on this machine already; the half-downloaded Apex and the manifest pointing outside the
    // library aren't listed at all. The other two are ticked from the start.
    assert_eq!(all.iter().filter(|r| r.str_at("badge") == Some("Here already")).count(), 1);
    assert!(all.iter().all(|r| r.str_at("title") != Some("Sneaky") && r.str_at("title") != Some("Apex Legends")));
    assert_eq!(j.get("primary").unwrap().str_at("label"), Some("Copy 2 games"));
    // Unticking one leaves the other; with none ticked nothing is asked.
    u.act("tick", "windrive|632470", "");
    u.act("tick", "windrive|1145360", "");
    assert!(parse(&u.page("windrive", "")).unwrap().get("primary").is_none_or(|p| *p == Json::Null));
    assert_eq!(parse(&u.act("copy", "", "")).unwrap().str_at("toast"), Some("Tick a game first."));
    u.act("tick", "windrive|1145360", "");
    assert_eq!(parse(&u.page("windrive", "")).unwrap().get("primary").unwrap().str_at("label"), Some("Copy 1 game"));
    assert!(parse(&u.act("copy", "", "")).unwrap().get("dialog").is_some());
    let plan = u.confirm().unwrap();
    let mount = u.windows_drive().unwrap();
    let Step::CopyGame { name, pairs } = &plan.steps[0] else { panic!("not a copy") };
    assert_eq!(name, "Hades II");
    for (from, to) in pairs {
        assert!(from.starts_with(&mount) && to.starts_with(u.ctx.home.join(".local/share/Steam/steamapps")) && !to.starts_with(&mount));
    }
    assert_eq!(plan.steps.last(), Some(&Step::Done { setup: None, brought: Some("windows") }));
    // Not enough room is said before anything is asked.
    u.free_bytes = Some(1);
    assert!(parse(&u.act("copy", "", "")).unwrap().str_at("toast").unwrap().starts_with("Not enough room"));
    // The copy itself leaves the source as it was.
    let (from, to) = (dir("drive").join("copy-from"), dir("drive").join("copy-to"));
    std::fs::create_dir_all(from.join("sub")).unwrap();
    std::fs::write(from.join("sub/a.bin"), b"12345").unwrap();
    assert_eq!(bring::copy_tree(&from, &to).unwrap(), 5);
    assert_eq!(std::fs::read(to.join("sub/a.bin")).unwrap(), b"12345");
    assert!(from.join("sub/a.bin").exists());
    done("drive");
}

#[test]
fn windows_apps_tick_what_the_drive_has_and_install_each_once() {
    let mut u = ui("apps", HANDHELD, Mode::Guided);
    let j = parse(&u.page("winapps", "")).unwrap();
    let all = rows(&j);
    let row_of = |name: &str| all.iter().find(|r| r.str_at("title") == Some(name)).unwrap();
    // Discord is installed, Afterburner and Spotify were found on the drive, DS4Windows is built in.
    assert_eq!(row_of("Discord").str_at("badge"), Some("Installed"));
    assert_eq!(row_of("MSI Afterburner").get("on").and_then(Json::as_bool), Some(true));
    assert_eq!(row_of("Spotify").get("on").and_then(Json::as_bool), Some(true));
    assert_eq!(row_of("DS4Windows").str_at("badge"), Some("Built in"));
    assert_eq!(row_of("Battle.net").str_at("button"), Some("How"));
    assert_eq!(j.get("primary").unwrap().str_at("label"), Some("Install 2 apps"));
    u.act("tick", "winapps|epic", "");
    u.act("tick", "winapps|gog", "");
    u.act("tick", "winapps|xbox", "");
    parse(&u.act("install-apps", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    let installs: Vec<&String> = plan.steps.iter().filter_map(|x| match x { Step::Host(c) if c.iter().any(|w| w == "install") => c.last(), _ => None }).collect();
    // Heroic is here already (Epic and GOG need nothing), Chrome comes once for the Xbox Cloud tile.
    assert_eq!(installs, vec!["io.github.radiolamp.mangojuice", "com.google.Chrome", "com.spotify.Client"]);
    assert!(plan.steps.iter().any(|x| matches!(x, Step::Tile { name, .. } if name == "Xbox Cloud")));
    done("apps");
}

#[test]
fn first_run_walks_the_steps_and_quick_setup_counts_its_changes() {
    let mut u = Ui::sample(&dir("setup"), HANDHELD, None);
    let mut st = u.settings();
    st.setup_done.clear();
    u.save(&st);
    assert_eq!(parse(&u.state()).unwrap().str_at("mode"), Some(""));
    assert_eq!(parse(&u.act("pick", "guided", "")).unwrap().str_at("root"), Some("setup"));
    let mut seen = Vec::new();
    loop {
        let page = parse(&u.page("setup", "")).unwrap();
        let step = page.str_at("step").unwrap().to_string();
        seen.push(step.clone());
        let answer = parse(&u.act("setup-next", &step, "")).unwrap();
        if answer.str_at("root") == Some("home") {
            break;
        }
        assert!(seen.len() < 9);
    }
    assert_eq!(seen, vec!["machine", "password", "games", "bring", "picks"]);
    assert!(u.settings().setup_finished());
    // Veteran: the defaults, minus what is on already, as one plan with the helper's part first.
    let mut v = Ui::sample(&dir("setup"), HANDHELD, None);
    assert_eq!(parse(&v.act("pick", "veteran", "")).unwrap().str_at("root"), Some("quick"));
    let page = parse(&v.page("quick", "")).unwrap();
    let all = rows(&page);
    let ticked = all.iter().filter(|r| r.get("on").and_then(Json::as_bool) == Some(true)).count();
    assert_eq!(page.get("primary").unwrap().str_at("label"), Some(format!("Apply {ticked} changes").as_str()));
    assert!(all.iter().any(|r| r.str_at("title") == Some("Swap tuning for zram") && r.str_at("badge") == Some("On already")));
    assert!(all.iter().any(|r| r.str_at("title") == Some("Heroic Games Launcher") && r.str_at("badge") == Some("Installed")));
    parse(&v.act("quick-apply", "", "")).unwrap();
    let plan = v.confirm().unwrap();
    assert_eq!((plan.steps[0].clone(), plan.ask, plan.items.len()), (Step::HelperInstall, Ask::Current, ticked + 1));
    let first_install = plan.steps.iter().position(|x| matches!(x, Step::Host(c) if c.iter().any(|w| w == "flathub"))).unwrap();
    let last_setting = plan.steps.iter().rposition(|x| matches!(x, Step::Host(c) if c.get(4).map(String::as_str) == Some("apply"))).unwrap();
    assert!(last_setting < first_install);
    assert_eq!(plan.steps.last(), Some(&Step::SetupFinished { boost: true }));
    // Tick all, twice, ticks everything and then nothing.
    v.act("tick-all", "quick", "");
    v.act("tick-all", "quick", "");
    assert_eq!(parse(&v.page("quick", "")).unwrap().get("primary").unwrap().str_at("label"), Some("Finish"));
    done("setup");
}

#[test]
fn settings_change_the_file_and_stay_in_range() {
    let mut u = ui("settings", HANDHELD, Mode::Guided);
    // A new interface size is applied, then asked to be kept, with the way back to the size before.
    let general = parse(&u.page("settings", "general")).unwrap();
    let scale = rows(&general).into_iter().find(|r| r.str_at("act") == Some("scale")).unwrap();
    assert!(scale.get("live").is_none(), "the size applies on A, not while it moves");
    let r = parse(&u.act("scale", "", "60")).unwrap();
    assert_eq!(r.get("state").and_then(Json::as_bool), Some(true));
    let keep = r.get("keep").unwrap();
    assert_eq!((keep.str_at("act"), keep.str_at("arg"), keep.str_at("value")), (Some("scale"), Some("back"), Some("100")));
    assert!(keep.str_at("text").unwrap().contains("{n}"));
    assert_eq!(u.settings().ui_scale, 60);
    // Going back asks nothing, and neither does the size already in use.
    assert!(parse(&u.act("scale", "back", "100")).unwrap().get("keep").is_none());
    assert_eq!(u.settings().ui_scale, 100);
    assert!(parse(&u.act("scale", "", "100")).unwrap().get("keep").is_none());
    assert!(parse(&u.act("scale", "", "400")).unwrap().get("keep").is_some());
    // On a TV the size starts larger until the player picks one.
    let tv = ui("tvscale", Preset::TvPc, Mode::Veteran);
    assert_eq!(parse(&tv.state()).unwrap().get("scale").and_then(Json::as_f64), Some(125.0));
    assert_eq!(parse(&u.state()).unwrap().get("scale").and_then(Json::as_f64), Some(150.0));
    done("tvscale");
    u.act("theme", "", "2");
    u.act("accent", "", "3");
    u.act("mode", "", "1");
    u.act("animations", "", "");
    let st = u.settings();
    assert_eq!((st.ui_scale, st.theme.as_str(), st.accent.as_str(), st.mode, st.animations), (150, "light", "jade", Some(Mode::Veteran), false));
    assert!(parse(&u.act("theme", "", "9")).unwrap().entries().is_empty());
    // Review changes lists the record newest first, and each row undoes its own entry.
    let page = parse(&u.page("changes", "")).unwrap();
    let secs = page.get("sections").unwrap().as_array();
    let all: Vec<Json> = secs[0].get("rows").unwrap().as_array().to_vec();
    assert_eq!(all.len(), 4);
    assert_eq!((all[0].str_at("arg"), all[0].str_at("title")), (Some("4"), Some("Battery charge limit: 80%")));
    // Each row says what Undo puts back, from the record.
    assert!(all[0].str_at("line").unwrap().starts_with("Undo puts back "), "{:?}", all[0].str_at("line"));
    for r in &all {
        let t = r.str_at("title").unwrap();
        assert!(!t.contains('_') && !t.contains("vm.") && !t.contains("current_value"), "{t}");
    }
    parse(&u.act("undo", "4", "")).unwrap();
    assert!(matches!(&u.confirm().unwrap().steps[1], Step::Host(c) if c.last().unwrap() == "{\"id\":4}"));
    done("settings");
}

#[test]
fn a_setup_file_asks_only_for_what_is_allowed_and_missing() {
    let u = ui("file", HANDHELD, Mode::Guided);
    let saved = settings::setup_json(&u);
    let (settings, apps) = settings::setup_wants(&u, &saved).unwrap();
    assert!(settings.is_empty() && apps.is_empty());
    let text = r#"{"turbinehub":"0.1","changes":[{"kind":"sysctl","key":"vm.swappiness","value":100},{"kind":"sysctl","key":"kernel.split_lock_mitigate","value":0},{"kind":"sysctl","key":"kernel.core_pattern","value":1},{"kind":"service","unit":"evil.service","enabled":true}],"apps":["com.spotify.Client","com.discordapp.Discord","com.evil.App"]}"#;
    let (settings, apps) = settings::setup_wants(&u, text).unwrap();
    assert_eq!(settings, vec![crate::helper::Setting::Sysctl { key: "kernel.split_lock_mitigate".into(), value: 0 }]);
    assert_eq!(apps.iter().map(|a| a.id).collect::<Vec<_>>(), vec!["com.spotify.Client"]);
    assert!(settings::setup_wants(&u, "{}").is_err());
    done("file");
}

#[test]
fn store_shows_each_apps_state_and_tiles_bring_their_browser() {
    let mut u = ui("store", HANDHELD, Mode::Guided);
    let page = parse(&u.page("store", "launchers")).unwrap();
    let heroic = rows(&page).into_iter().find(|r| r.str_at("title") == Some("Heroic Games Launcher")).unwrap();
    // A row shows the app's state and opens the app's own page.
    assert_eq!((heroic.str_at("badge"), heroic.str_at("act"), heroic.str_at("arg")), (Some("Update ready"), Some("open"), Some("store-item|com.heroicgameslauncher.hgl")));
    assert!(page.get("hints").unwrap().as_array().iter().any(|h| h.str_at("label") == Some("Update all")));
    let j = parse(&u.act("store", "com.spotify.Client", "")).unwrap();
    assert_eq!(j.get("dialog").unwrap().str_at("confirm"), Some("Install"));
    assert_eq!(u.confirm().unwrap().steps, vec![Step::Host(["flatpak-spawn", "--host", "flatpak", "install", "-y", "--noninteractive", "flathub", "com.spotify.Client"].iter().map(|x| x.to_string()).collect())]);
    parse(&u.act("tile", "netflix", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert!(matches!(&plan.steps[0], Step::Host(c) if c.last().unwrap() == "com.google.Chrome"));
    assert_eq!(plan.steps.last(), Some(&Step::Tile { name: "Netflix".into(), url: "https://www.netflix.com".into() }));
    assert!(store::tile_shortcut("Netflix", "https://www.netflix.com").launch_options.contains(&store::tile_marker("https://www.netflix.com")));
    // Without Decky Loader's release tag (never online) nothing is asked.
    u.decky_tag = None;
    assert!(parse(&u.act("decky", "", "")).unwrap().get("dialog").is_none());
    done("store");
}

#[test]
fn windows_files_reset_keeps_a_backup() {
    let d = dir("reset");
    let pfx = d.join("compatdata/1/pfx");
    std::fs::create_dir_all(&pfx).unwrap();
    std::fs::write(pfx.join("user.reg"), "x").unwrap();
    assert!(reset_windows_files(Path::new("/nonexistent/pfx"), 5).is_err());
    assert!(reset_windows_files(&d.join("compatdata/1"), 5).is_err());
    let name = reset_windows_files(&pfx, 5).unwrap();
    assert!(!pfx.exists() && d.join("compatdata/1").join(name).join("user.reg").exists());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn windows_files_reset_asks_carefully_and_undoes() {
    let mut u = ui("reset-undo", HANDHELD, Mode::Guided);
    let game = super::games::listed(&u).into_iter().find(|l| l.game.prefix.is_some() && !l.locked() && l.verdict.label != crate::library::Label::WontRun).unwrap();
    let id = format!("steam:{}", game.game.store_id);
    let pfx = game.game.prefix.clone().unwrap();
    std::fs::write(pfx.join("saves.dat"), "old").unwrap();
    let j = parse(&u.act("game", &format!("reset|{id}"), "")).unwrap();
    let d = j.get("dialog").unwrap();
    assert_eq!(d.get("danger").and_then(Json::as_bool), Some(true));
    assert!(d.str_at("text").unwrap().contains("Undo"));
    let plan = u.confirm().unwrap();
    let step = plan.steps.last().unwrap().clone();
    assert!(matches!(&step, Step::ResetPrefix { app_id, .. } if *app_id == game.game.store_id));
    super::games::reset_step(&step, &u.folder(), 7).unwrap().unwrap();
    assert!(!pfx.exists());
    // The game's page now offers Undo, even though its Windows files are gone until its next start.
    let page = parse(&u.page("games", &id)).unwrap();
    assert!(rows(&page).iter().any(|r| r.str_at("arg") == Some(format!("reset-undo|{id}").as_str())));
    // The game made fresh files since: Undo keeps them aside and puts the old ones back, saves and all.
    std::fs::create_dir_all(&pfx).unwrap();
    std::fs::write(pfx.join("saves.dat"), "fresh").unwrap();
    parse(&u.act("game", &format!("reset-undo|{id}"), "")).unwrap();
    let plan = u.confirm().unwrap();
    super::games::reset_step(plan.steps.last().unwrap(), &u.folder(), 9).unwrap().unwrap();
    assert_eq!(std::fs::read_to_string(pfx.join("saves.dat")).unwrap(), "old");
    assert_eq!(std::fs::read_to_string(pfx.with_file_name("pfx.turbinehub-fresh-9").join("saves.dat")).unwrap(), "fresh");
    assert!(rows(&parse(&u.page("games", &id)).unwrap()).iter().all(|r| r.str_at("arg") != Some(format!("reset-undo|{id}").as_str())));
    done("reset-undo");
}

#[test]
fn system_dialogs_show_plain_names_and_before_and_after() {
    let mut g = ui("words", HANDHELD, Mode::Guided);
    install_helper_in(&g);
    // The power limit is one line; the boost limits that come down with it say so.
    let d = parse(&g.act("tdp", "", "20")).unwrap();
    let items: Vec<(String, String)> = d.get("dialog").unwrap().get("items").unwrap().as_array().iter().map(|i| (i.str_at("title").unwrap().to_string(), i.str_at("line").unwrap().to_string())).collect();
    assert_eq!(items[0].0, "Power limit: 17 W \u{2192} 20 W");
    assert_eq!(items[1].0, "Boost limits lowered too: 25 W and 30 W \u{2192} 20 W");
    assert!(items.iter().all(|(t, l)| !t.contains("current_value") && !l.contains("current_value")));
    g.cancel();
    // A value that is set already asks nothing.
    assert_eq!(parse(&g.act("charge", "", "2")).unwrap().str_at("toast"), Some("That's already set."));
    let d = parse(&g.act("charge", "", "4")).unwrap();
    assert_eq!(d.get("dialog").unwrap().get("items").unwrap().as_array()[0].str_at("title"), Some("Battery charge limit: 80% \u{2192} 100%"));
    g.cancel();
    // A built-in controller is never plugged in again: only the next start counts.
    let d = parse(&g.act("wake", "045e:028e", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("text"), Some("It takes effect the next time the machine starts."));
    g.cancel();
    // Veteran mode adds the exact key and the raw values.
    let mut v = ui("words-v", HANDHELD, Mode::Veteran);
    install_helper_in(&v);
    let d = parse(&v.act("tdp", "", "20")).unwrap();
    assert!(d.get("dialog").unwrap().get("items").unwrap().as_array()[0].str_at("line").unwrap().ends_with("ppt_pl1_spl/current_value: 17 \u{2192} 20"));
    v.cancel();
    // Graphics card limits in watts.
    let mut p = ui("words-pc", Preset::Desktop, Mode::Guided);
    install_helper_in(&p);
    let d = parse(&p.act("gpu-power", "", "250")).unwrap();
    assert_eq!(d.get("dialog").unwrap().get("items").unwrap().as_array()[0].str_at("title"), Some("Graphics card power limit: 304 W \u{2192} 250 W"));
    p.cancel();
    // One-press settings ask once, and change only when confirmed.
    let d = parse(&g.act("testbuilds", "", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Get test builds?"));
    assert_eq!(g.confirm().unwrap().steps, vec![Step::Choice { key: "testbuilds", on: true }]);
    // Nothing is ever sent: there is no switch for it.
    assert!(g.act("reports", "", "").is_empty() || !g.act("reports", "", "").contains("dialog"));
    let st = g.settings();
    assert!(!st.reports && !st.test_builds);
    // The living room PC is called that everywhere.
    let tv = ui("words-tv", Preset::TvPc, Mode::Guided);
    assert_eq!(tv.machine().name(), "Living room PC");
    done("words");
    done("words-v");
    done("words-pc");
    done("words-tv");
}

#[test]
fn tick_all_never_turns_on_ssh() {
    let mut u = ui("ssh", HANDHELD, Mode::Veteran);
    u.page("quick", "");
    u.act("tick-all", "quick", "");
    let picked = u.picked.get("quick").cloned().unwrap_or_default();
    assert!(!picked.is_empty() && !picked.contains("ssh"));
    // Ticked by hand, it stays ticked through Tick all.
    u.picked.get_mut("quick").unwrap().remove("library");
    u.act("tick", "quick|ssh", "");
    u.act("tick-all", "quick", "");
    assert!(u.picked.get("quick").unwrap().contains("ssh"));
    done("ssh");
}

fn install_helper_in(u: &Ui) {
    std::fs::create_dir_all(u.ctx.root.join("var/lib/turbinehub/bin")).unwrap();
    std::fs::write(u.ctx.root.join(crate::helper::BIN.trim_start_matches('/')), "").unwrap();
}

#[test]
fn help_search_matches_windows_words_and_the_glossary() {
    let mut u = ui("help", HANDHELD, Mode::Guided);
    let j = parse(&u.page("wheres", "task")).unwrap();
    let all = rows(&j);
    assert_eq!(all.iter().filter(|r| r.str_at("kind") == Some("info")).map(|r| r.str_at("title").unwrap()).collect::<Vec<_>>(), vec!["Task Manager"]);
    let j = parse(&u.page("wheres", "zzzz")).unwrap();
    assert_eq!(rows(&j).len(), 1);
    assert!(parse(&u.page("wheres", "GAME BAR")).unwrap().to_text().contains("Quick panel"));
    // Saves have their own entry, and every word a newcomer meets on Picks and the tune-up is explained.
    assert!(rows(&parse(&u.page("wheres", "save")).unwrap()).iter().any(|r| r.str_at("title").unwrap_or("").starts_with("Game saves")));
    for word in ["zram", "swap", "huge pages", "split lock", "ntsync", "kernel", "fsr 4", "frame generation", "proton", "compatibility tool", "flatpak", "flathub", "flatseal", "ludusavi", "protonplus", "decky", "vrr", "hdr", "tdp", "bazzite", "cachyos"] {
        let found = rows(&parse(&u.page("wheres", word)).unwrap());
        assert!(found.iter().any(|r| r.str_at("title").unwrap_or("").to_lowercase().contains(word)), "{word}");
    }
    done("help");
}

#[test]
fn help_is_about_the_page_it_was_asked_on() {
    let mut u = ui("pagehelp", HANDHELD, Mode::Guided);
    let games = parse(&u.page("help", "games@steam:1086940")).unwrap();
    let saves = parse(&u.page("help", "saves@steam:1086940")).unwrap();
    assert_eq!(games.str_at("title"), Some("Games"));
    assert_eq!(saves.str_at("title"), Some("Saves of Baldur's Gate 3"));
    assert_ne!(games.str_at("intro"), saves.str_at("intro"));
    // The page's own buttons are explained, and Where's my...? is one press away.
    let text = games.to_text();
    assert!(text.contains("Play.") && text.contains("Library check.") && text.contains("Search."));
    assert!(rows(&games).iter().any(|r| r.str_at("act") == Some("open") && r.str_at("arg") == Some("wheres")));
    // At a desk the keys are named.
    u.window = true;
    assert!(rows(&parse(&u.page("help", "games@")).unwrap()).iter().any(|r| r.str_at("title") == Some("Enter")));
    done("pagehelp");
}
