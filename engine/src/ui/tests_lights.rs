//! The lights on machines that differ from the plain sample: Ally Hub on the same machine, HueSync installed, lights
//! closed again after the setup, an unrelated LED that stays closed, the lighting chip, the guided test and the
//! lighting check.
use super::lighting::{self, AllyHub, HostLights, Lighting, Tried};
use super::plan::{Ask, Step};
use super::*;
use crate::json::parse;
use std::os::unix::fs::PermissionsExt;

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-ui-lights-{tag}-{}", std::process::id()))
}

fn ui(tag: &str, mode: Mode) -> Ui {
    Ui::sample(&dir(tag), HANDHELD, Some(mode))
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

fn rows(j: &Json) -> Vec<Json> {
    j.get("sections").map(Json::as_array).unwrap_or(&[]).iter().flat_map(|sec| sec.get("rows").map(Json::as_array).unwrap_or(&[]).to_vec()).collect()
}

fn titles(u: &mut Ui) -> Vec<String> {
    rows(&parse(&u.page("module:lighting", "")).unwrap()).iter().filter_map(|r| r.str_at("title").map(String::from)).collect()
}

fn install_helper(u: &Ui, bytes: &str) {
    let bin = u.ctx.root.join(crate::helper::BIN.trim_start_matches('/'));
    std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
    std::fs::write(bin, bytes).unwrap();
}

fn mode(u: &Ui, path: &str, m: u32) {
    std::fs::set_permissions(u.ctx.root.join(path), std::fs::Permissions::from_mode(m)).unwrap();
}

/// The sample handheld as an Ally X: packed rings capped at 255 behind the ASUS lighting chip, set up already.
fn ally_x(tag: &str, mode_: Mode) -> Ui {
    let mut u = ui(tag, mode_);
    let _ = std::fs::remove_dir_all(u.ctx.root.join("sys/class/leds/ally:rgb:joystick_rings"));
    crate::lights_hid::tests::ally_x(&u.ctx.root);
    u.sample_lights("closed");
    for n in ["hidraw0", "hidraw1", "hidraw2", "hidraw3"] {
        mode(&u, &format!("dev/{n}"), 0o600);
    }
    u
}

fn checkup(u: &Ui, id: &str) -> Option<crate::checkup::Item> {
    crate::checkup::items(&u.machine(), &u.facts_now()).into_iter().find(|i| i.id == id)
}

#[test]
fn ally_hubs_saved_choice_is_read_through_the_host_and_is_where_turbine_hub_starts() {
    // Only reading: two files and three questions to systemd, nothing that writes.
    let q = lighting::host_questions(std::path::Path::new("/home/deck"));
    assert_eq!(q[0], ["cat", "/home/deck/.config/allyhub/config.json"]);
    assert_eq!(q[1], ["systemctl", "--user", "is-active", "allyhub-agent.service"]);
    assert_eq!(q[2], ["systemctl", "is-active", "plugin_loader.service"]);
    assert_eq!(q[3], ["cat", "/etc/udev/rules.d/70-turbinehub-access.rules"]);
    assert_eq!(q[4], ["systemctl", "is-enabled", "turbinehub-reapply.service"]);
    assert!(q.iter().all(|line| line[0] == "cat" || (line[0] == "systemctl" && line.iter().any(|w| w.starts_with("is-")))));
    // Ally Hub's config as it saves it: the chip with the route its own test found.
    let config = r##"{"rgb":{"rgb":[225,29,72]},"lighting":{"effect":null,"fps":20,"controller":"allyhub","encoding":"hid","hid_method":"m8","gamma":2.2}}"##;
    let h = HostLights::from_host(Some(config), "active\n", "inactive\n", Some("SUBSYSTEM==\"leds\"\n"), "enabled\n");
    assert_eq!(h.allyhub, Some(AllyHub { controller: "allyhub".into(), hid_method: "m8".into(), encoding: "hid".into() }));
    assert!(h.asked && h.allyhub_agent && !h.decky_running && h.reapply == "enabled" && h.rule.as_deref() == Some("SUBSYSTEM==\"leds\""));
    assert_eq!(h.allyhub_route(), "chip:m8");
    assert_eq!(HostLights::parse(&h.to_json()), h);
    let with = |light: &str| HostLights::from_host(Some(&format!("{{\"lighting\":{light}}}")), "", "", None, "");
    // The chip with no route saved: Ally Hub's own default, the full handshake.
    assert_eq!(with(r#"{"controller":"allyhub","encoding":"hid"}"#).allyhub_route(), "chip:m3");
    assert_eq!(with(r#"{"encoding":"hid","hid_method":"m99"}"#).allyhub_route(), "chip:m3");
    // The files with the encoding its test found; nothing saved, or junk: nothing to start from.
    assert_eq!(with(r#"{"encoding":"hex"}"#).allyhub_route(), "files:hex");
    assert_eq!(with(r#"{"encoding":"packed_bgr"}"#).allyhub_route(), "files:packed_bgr");
    assert_eq!(with(r#"{"controller":"allyhub"}"#).allyhub_route(), "");
    assert_eq!(with(r#"{"encoding":"rm -rf /"}"#).allyhub_route(), "");
    // Without a controller in its settings Ally Hub leaves the rings to HueSync.
    assert_eq!(with("{}").allyhub.unwrap().controller, "huesync");
    // No Ally Hub: nothing found, and the unit's word for "not there" is kept.
    let none = HostLights::from_host(None, "inactive\n", "active\n", None, "");
    assert!(none.asked && none.allyhub.is_none() && !none.allyhub_agent && none.decky_running && none.rule.is_none() && none.allyhub_route().is_empty());
    assert_eq!(HostLights::parse(&none.to_json()), none);
    assert_eq!(HostLights::parse("junk"), HostLights::default());
    // The player's own test wins over Ally Hub's choice; before one, Ally Hub's is the start.
    let l = Lighting::default();
    assert_eq!(lighting::route_in_use(&l, &h), "chip:m8");
    assert_eq!(lighting::route_in_use(&Lighting { route: "chip:m6".into(), ..l.clone() }, &h), "chip:m6");
    assert_eq!(lighting::route_in_use(&l, &none), "");
    // The way is kept in the lighting file, and junk in it is dropped.
    let saved = Lighting { route: "files:hex".into(), allyhub: true, ..Lighting::default() };
    assert_eq!(Lighting::parse(&saved.to_json()), saved);
    assert_eq!(Lighting::parse(r#"{"route":"chip:m99"}"#).route, "");
    // What the host said is kept for the background helper, on the made-up machine's own folder.
    let mut u = ally_x("start", Mode::Veteran);
    u.set_host_lights(h.clone());
    assert_eq!(HostLights::load(&u.folder()), h);
    mode(&u, "dev/hidraw3", 0o602);
    let page = parse(&u.page("module:lighting", "")).unwrap();
    let way = rows(&page).into_iter().find(|r| r.str_at("title") == Some("Way in use")).unwrap();
    assert_eq!(way.str_at("value"), Some("chip:m8"));
    assert!(way.str_at("line").unwrap().contains("Ally Hub's saved choice"), "{way:?}");
    done("start");
}

#[test]
fn another_app_setting_the_lights_is_said_plainly_with_both_choices() {
    let mut u = ui("rival", Mode::Guided);
    u.sample_lights("allyhub");
    // First on the page: who is setting the lights, and the two choices.
    let page = parse(&u.page("module:lighting", "")).unwrap();
    let r = rows(&page);
    assert_eq!((r[0].str_at("title"), r[0].str_at("kind")), (Some("Ally Hub is setting the lights now"), Some("info")));
    assert_eq!((r[1].str_at("title"), r[1].str_at("act"), r[1].str_at("arg")), (Some("Let Turbine Hub drive them"), Some("light-rival"), Some("allyhub")));
    assert_eq!((r[2].str_at("title"), r[2].str_at("act"), r[2].str_at("arg")), (Some("Leave them to Ally Hub"), Some("light"), Some("allyhub")));
    // The checkup says so too, and leads to the page.
    let item = checkup(&u, "lights-rival").unwrap();
    assert_eq!((item.title.as_str(), item.action), ("Ally Hub is setting the lights too", ("Choose", "lighting")));
    assert_eq!(parse(&u.act("checkup", "lighting", "")).unwrap().str_at("open"), Some("module:lighting"));
    // "Let Turbine Hub drive them": where Ally Hub's own lighting is turned off. Nothing of Ally Hub's is touched.
    let d = parse(&u.act("light-rival", "allyhub", "")).unwrap();
    let d = d.get("dialog").unwrap();
    assert_eq!(d.str_at("title"), Some("Turn Ally Hub's lighting off"));
    assert!(d.get("items").unwrap().as_array().iter().any(|i| i.str_at("line").unwrap().contains("Use HueSync instead")));
    assert!(u.confirm().is_none());
    assert!(!Lighting::load(&u.folder()).allyhub);
    // "Leave them to Ally Hub": Turbine Hub keeps its hands off, and stops asking about it and about the setup.
    u.act("light", "allyhub", "");
    let l = Lighting::load(&u.folder());
    assert_eq!((l.allyhub, l.huesync, l.left_to(), l.label().as_str()), (true, false, Some("Ally Hub"), "Ally Hub"));
    assert!(checkup(&u, "lights-rival").is_none() && checkup(&u, "lights").is_none());
    assert_eq!(titles(&mut u), ["Let Ally Hub run the lights"]);
    assert!(parse(&u.act("light-test", "", "")).unwrap().str_at("toast").unwrap().contains("left to Ally Hub"));
    // Taking them back shows the page again.
    u.act("light", "allyhub", "");
    assert!(titles(&mut u).len() > 5);
    // Ally Hub installed but leaving the rings to HueSync, or its agent stopped: nobody else is setting them.
    for quiet in [HostLights { allyhub_agent: false, ..u.host_lights.clone() }, HostLights { allyhub: Some(AllyHub { controller: "huesync".into(), ..AllyHub::default() }), ..u.host_lights.clone() }] {
        u.host_lights = quiet;
        assert!(checkup(&u, "lights-rival").is_none());
        assert!(!titles(&mut u).iter().any(|t| t.contains("is setting the lights")));
        // It is on the machine, so the lights can still be left to it.
        assert!(titles(&mut u).contains(&"Let Ally Hub run the lights".to_string()));
    }
    done("rival");
    // HueSync installed with Decky Loader running: the same, by its name and with its own place to turn it off.
    let mut u = ui("rival-hue", Mode::Guided);
    assert!(lighting::rivals(&u.host_lights, &u.ctx.home).is_empty());
    u.sample_lights("huesync");
    assert_eq!(titles(&mut u)[..3], ["HueSync is setting the lights now", "Let Turbine Hub drive them", "Leave them to HueSync"]);
    assert_eq!(checkup(&u, "lights-rival").unwrap().title, "HueSync is setting the lights too");
    let d = parse(&u.act("light-rival", "huesync", "")).unwrap();
    assert!(d.get("dialog").unwrap().get("items").unwrap().as_array()[0].str_at("line").unwrap().contains("quick menu"));
    // Installed but Decky Loader isn't running: it can't be setting anything.
    u.host_lights.decky_running = false;
    assert!(checkup(&u, "lights-rival").is_none());
    u.host_lights.decky_running = true;
    u.act("light", "huesync", "");
    assert_eq!(Lighting::load(&u.folder()).left_to(), Some("HueSync"));
    assert!(checkup(&u, "lights-rival").is_none());
    done("rival-hue");
}

#[test]
fn lights_closed_again_after_the_setup_are_opened_checked_and_never_asked_for_forever() {
    let mut u = ui("closed", Mode::Veteran);
    u.sample_lights("closed");
    install_helper(&u, "helper");
    let files = ["/sys/class/leds/ally:rgb:joystick_rings/multi_intensity", "/sys/class/leds/ally:rgb:joystick_rings/brightness"];
    // The setup is in the helper's record, and the files are closed: said as that, not as "needs its setup".
    let page = parse(&u.page("module:lighting", "")).unwrap();
    let first = &rows(&page)[0];
    assert_eq!((first.str_at("title"), first.str_at("act"), first.str_at("button")), (Some("The lights are closed again"), Some("light-test"), Some("Open them")));
    assert!(first.str_at("line").unwrap().contains(&format!("{} (mode 644)", files[0])), "{first:?}");
    let item = checkup(&u, "lights").unwrap();
    assert_eq!((item.title.as_str(), item.action), ("The lights are closed again", ("Open them", "light-test")));
    assert!(item.line.contains("multi_intensity (mode 644)"), "{}", item.line);
    // The fix: the helper opens them (no password: it is installed and current), then Turbine Hub looks again at
    // exactly those two files.
    let d = parse(&u.act("checkup", "light-test", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Open the lights again?"));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.ask, Ask::None);
    assert!(matches!(&plan.steps[0], Step::Host(line) if line.iter().any(|a| a.contains("\"access\""))));
    assert_eq!(plan.steps[1], Step::LightsCheck { need: files.iter().map(|f| f.to_string()).collect(), exact: true });
    assert_eq!((plan.steps.len(), plan.done.as_str()), (2, "Lights opened again"));
    // Still closed afterwards: the check fails with what and why, in place of a false "Lights set up".
    let Step::LightsCheck { need, exact } = &plan.steps[1] else { panic!() };
    let why = lighting::check_open(&u.ctx.root, &u.folder(), need, *exact).unwrap_err();
    assert!(why.starts_with("The lights are still closed: /sys/class/leds/ally:rgb:joystick_rings/multi_intensity (mode 644)") && why.contains("closes them again"), "{why}");
    assert_eq!(lighting::check_open(&u.ctx.root, &u.folder(), need, false).unwrap_err().split('.').next(), Some("The lights are still closed"));
    // From here on the same fix isn't offered again: the checkup and the page say why and offer the lighting check.
    let item = checkup(&u, "lights").unwrap();
    assert_eq!((item.title.as_str(), item.action), ("The lights stay closed", ("Lighting check", "light-report")));
    assert!(item.line.contains("closes them again") && item.line.contains("mode 644"), "{}", item.line);
    let first = rows(&parse(&u.page("module:lighting", "")).unwrap())[0].clone();
    assert_eq!((first.str_at("title"), first.str_at("act")), (Some("The lights stay closed"), Some("light-report")));
    assert!(parse(&u.act("checkup", "light-report", "")).unwrap().get("dialog").is_some());
    u.cancel();
    // One of the two opens: something changed, so opening them is offered again.
    mode(&u, "sys/class/leds/ally:rgb:joystick_rings/multi_intensity", 0o666);
    assert_eq!(checkup(&u, "lights").unwrap().title, "The lights are closed again");
    // Both open: nothing to say, and the note about them staying closed is gone.
    mode(&u, "sys/class/leds/ally:rgb:joystick_rings/brightness", 0o666);
    assert!(checkup(&u, "lights").is_none());
    assert!(lighting::check_open(&u.ctx.root, &u.folder(), need, true).is_ok());
    assert!(!u.folder().join("lighting-stuck").exists());
    assert!(!titles(&mut u).iter().any(|t| t.contains("closed")));
    // Gone altogether (the controller asleep): said as that.
    std::fs::remove_file(u.ctx.root.join("sys/class/leds/ally:rgb:joystick_rings/brightness")).unwrap();
    assert!(lighting::check_open(&u.ctx.root, &u.folder(), need, true).unwrap_err().contains("isn't there right now"));
    done("closed");
}

#[test]
fn a_helper_older_than_the_app_is_updated_before_it_is_asked_to_open_the_lights() {
    let mut u = ui("old-helper", Mode::Guided);
    u.sample_lights("closed");
    install_helper(&u, "the helper of 0.2");
    let new = dir("old-helper").join("new-helper");
    std::fs::write(&new, "the helper of this version").unwrap();
    assert!(!u.helper_outdated());
    u.helper_bundled = Some(new.clone());
    assert!(u.helper_outdated());
    // The app's update never replaced it: opening the lights updates it first, with the password, once.
    parse(&u.act("light-test", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!((plan.steps[0].clone(), plan.ask), (Step::HelperInstall, Ask::Current));
    assert_eq!(plan.items[0].0, "Update Turbine Hub's helper", "{:?}", plan.items);
    assert!(plan.text.starts_with("This changes a system setting, and Turbine Hub's helper on this machine is from an older version. Updating it takes your SteamOS password once more"), "{}", plan.text);
    assert!(matches!(plan.steps.last(), Some(Step::LightsCheck { exact: false, .. })));
    assert!(super::games::report_text(&u, lighting::REPORT, "now").contains("helper: installed (older than this Turbine Hub)"));
    // The same helper as the app's: no password, nothing to update.
    std::fs::write(&new, "the helper of 0.2").unwrap();
    assert!(!u.helper_outdated());
    parse(&u.act("light-test", "", "")).unwrap();
    assert_eq!(u.confirm().unwrap().ask, Ask::None);
    // No helper at all is not "older".
    std::fs::remove_file(u.ctx.root.join(crate::helper::BIN.trim_start_matches('/'))).unwrap();
    assert!(!u.helper_outdated());
    done("old-helper");
}

#[test]
fn only_what_the_way_in_use_needs_counts_never_another_led() {
    // The rings open, a keyboard backlight with a color file of its own that stays closed, and a name that only
    // looks like a light: nothing asks for a setup.
    let mut u = ui("other-led", Mode::Veteran);
    u.sample_lights("open");
    let kbd = u.ctx.root.join("sys/class/leds/asus::kbd_backlight");
    std::fs::create_dir_all(&kbd).unwrap();
    for (f, v) in [("multi_intensity", "0 0 0\n"), ("brightness", "0\n")] {
        std::fs::write(kbd.join(f), v).unwrap();
        mode(&u, &format!("sys/class/leds/asus::kbd_backlight/{f}"), 0o644);
    }
    std::fs::create_dir_all(u.ctx.root.join("sys/class/leds/phantom:rgb:logo")).unwrap();
    assert!(u.machine().rgb_leds.len() >= 3);
    assert!(checkup(&u, "lights").is_none() && !u.facts_now().lights_locked);
    let page = parse(&u.page("module:lighting", "")).unwrap();
    assert!(!rows(&page).iter().any(|r| r.str_at("tone") == Some("warn")), "{page:?}");
    // Veteran mode shows the keyboard as closed, and never the name without a color file.
    let k = rows(&page).into_iter().find(|r| r.str_at("title") == Some("asus::kbd_backlight")).unwrap();
    assert!(k.str_at("line").unwrap().contains("Closed: /sys/class/leds/asus::kbd_backlight/multi_intensity (mode 644)"));
    assert!(!rows(&page).iter().any(|r| r.str_at("title") == Some("phantom:rgb:logo")));
    // The light test and the setup dialog never name it either.
    parse(&u.act("light-test", "", "")).unwrap();
    assert_eq!(u.light_test.as_ref().unwrap().routes, ["files"]);
    done("other-led");

    // The chip path: the chip's commands interface is all that counts. The rings' own files stay closed (as the
    // kernel leaves them) and that asks for nothing.
    let mut u = ally_x("chip-only", Mode::Veteran);
    assert_eq!(lighting::locked(&u, &u.machine()).unwrap().closed, ["/dev/hidraw3 (mode 600)"]);
    let item = checkup(&u, "lights").unwrap();
    assert!(item.title == "The lights are closed again" && item.line.contains("/dev/hidraw3 (mode 600)") && !item.line.contains("multi_intensity"), "{}", item.line);
    parse(&u.act("light-test", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert!(plan.steps.contains(&Step::LightsCheck { need: vec!["/dev/hidraw3".to_string()], exact: true }));
    // No color goes to files the chip path doesn't use.
    assert!(!plan.steps.iter().any(|s| matches!(s, Step::HostWrite { .. })));
    // The write bit on that one node is enough: no read bit, no other interface, no LED file.
    mode(&u, "dev/hidraw3", 0o602);
    assert!(checkup(&u, "lights").is_none());
    assert!(!titles(&mut u).iter().any(|t| t.contains("closed") || t.contains("setup")));
    // A way that uses the files instead needs the files, and says so.
    Lighting { route: "files:hex".into(), ..Lighting::load(&u.folder()) }.save(&u.folder());
    assert_eq!(lighting::locked(&u, &u.machine()).unwrap().closed.len(), 2);
    done("chip-only");
}

#[test]
fn the_guided_test_tries_each_way_asks_and_keeps_the_first_yes() {
    let mut u = ally_x("guided", Mode::Guided);
    mode(&u, "dev/hidraw3", 0o602);
    let asked = |u: &Ui| lighting::test_asked(&u.folder()).unwrap();
    // Test lights: the first way goes to the background helper, and the page asks.
    let a = parse(&u.act("light-test", "", "")).unwrap();
    assert!(a.str_at("toast").unwrap().contains("turn red"));
    assert_eq!(u.light_test.as_ref().unwrap().routes, ["chip:m6", "chip:m7", "chip:m8", "chip:m2", "chip:m3"]);
    let (first, route) = asked(&u);
    assert_eq!(route, "chip:m6");
    let page = parse(&u.page("module:lighting", "")).unwrap();
    let r = rows(&page);
    assert_eq!((r[0].str_at("title"), r[0].str_at("value")), (Some("Did both rings turn red?"), Some("Way 1 of 5")));
    assert_eq!([r[1].str_at("title"), r[2].str_at("title"), r[3].str_at("title"), r[4].str_at("title")], [Some("Yes"), Some("No"), Some("Only partly"), Some("Stop the test")]);
    assert!(r[1..5].iter().all(|x| x.str_at("act") == Some("light-answer")));
    // Guided mode keeps the route's name to itself.
    assert!(!r[0].str_at("line").unwrap().contains("m6"));
    // Pressing Test again while it runs changes nothing.
    u.act("light-test", "", "");
    assert_eq!(asked(&u), (first, "chip:m6".to_string()));
    // The background helper answers for that way; the page then says to look.
    lighting::save_step(&u.data, &Tried { at: first, route: "chip:m6".into(), sent: true, lines: vec!["8 reports sent to /dev/hidraw3 on route m6.".into()] });
    assert!(rows(&parse(&u.page("module:lighting", "")).unwrap())[0].str_at("line").unwrap().starts_with("Look at the lights now"));
    // No: the next way, with a newer request.
    u.act("light-answer", "no", "");
    let (second, route) = asked(&u);
    assert!(second > first && route == "chip:m7");
    assert_eq!(rows(&parse(&u.page("module:lighting", "")).unwrap())[0].str_at("value"), Some("Way 2 of 5"));
    // A way that couldn't be sent says so.
    lighting::save_step(&u.data, &Tried { at: second, route: "chip:m7".into(), sent: false, lines: vec!["/dev/hidraw3 didn't take the report (Broken pipe).".into()] });
    assert!(rows(&parse(&u.page("module:lighting", "")).unwrap())[0].str_at("line").unwrap().contains("couldn't be sent"));
    // Only partly: the next way too.
    u.act("light-answer", "partly", "");
    let (third, route) = asked(&u);
    assert!(third > second && route == "chip:m8");
    assert_eq!(Lighting::load(&u.folder()).route, "");
    // Yes: that way is saved in Turbine Hub's lighting settings, and the look comes back.
    let a = parse(&u.act("light-answer", "yes", "")).unwrap();
    assert!(a.str_at("toast").unwrap().starts_with("Saved."));
    assert_eq!(Lighting::load(&u.folder()).route, "chip:m8");
    let (end, word) = asked(&u);
    assert!(end > third && word == lighting::TEST_END && u.light_test.is_none());
    let (_, log) = lighting::last_test(&u.data).unwrap();
    assert!(log.iter().any(|l| l.contains("Way 1 of 5") && l.contains("you saw: not red")) && log.iter().any(|l| l.contains("NOT sent") && l.contains("only partly red")), "{log:?}");
    assert!(log.last().unwrap().starts_with("Works: way 3 of 5 (lighting chip, route m8"), "{log:?}");
    // The page is back to normal, with the result under Test lights; the lights now go the saved way.
    let page = parse(&u.page("module:lighting", "")).unwrap();
    assert!(!rows(&page).iter().any(|r| r.str_at("act") == Some("light-answer")));
    assert!(rows(&page).iter().find(|r| r.str_at("title") == Some("Test lights")).unwrap().str_at("line").unwrap().starts_with("Works: way 3 of 5"));
    assert_eq!(crate::lights::driven(&u.ctx.root, &u.machine().rgb_leds, &lighting::route_in_use(&Lighting::load(&u.folder()), &u.host_lights))[0].method.key, "m8");
    // An answer with no test running does nothing.
    u.act("light-answer", "yes", "");
    assert_eq!(asked(&u).0, end);

    // No to every way: nothing is saved, and the lighting check is where it goes next.
    Lighting { route: String::new(), ..Lighting::load(&u.folder()) }.save(&u.folder());
    u.act("light-test", "", "");
    for _ in 0..4 {
        u.act("light-answer", "no", "");
    }
    assert_eq!(asked(&u).1, "chip:m3");
    let a = parse(&u.act("light-answer", "no", "")).unwrap();
    assert!(a.str_at("toast").unwrap().contains("None of the ways worked"));
    assert!(u.light_test.is_none() && asked(&u).1 == lighting::TEST_END && Lighting::load(&u.folder()).route.is_empty());
    assert!(lighting::last_test(&u.data).unwrap().1.last().unwrap().starts_with("None of the 5 ways"));
    // Stopping, and leaving the lights to another app, end it too.
    u.act("light-test", "", "");
    u.act("light-answer", "stop", "");
    assert!(u.light_test.is_none() && asked(&u).1 == lighting::TEST_END);
    u.act("light-test", "", "");
    u.act("light", "huesync", "");
    assert!(u.light_test.is_none() && asked(&u).1 == lighting::TEST_END);
    done("guided");

    // Veteran mode names the way. A light on plain files has one way, and one light to look at.
    let mut u = ui("guided-files", Mode::Veteran);
    u.sample_lights("open");
    u.act("light-test", "", "");
    assert_eq!(asked(&u).1, "files");
    let q = rows(&parse(&u.page("module:lighting", "")).unwrap())[0].clone();
    assert_eq!(q.str_at("title"), Some("Did both rings turn red?"));
    assert!(q.str_at("line").unwrap().contains("Way: the light's files."), "{q:?}");
    u.act("light-answer", "yes", "");
    assert_eq!(Lighting::load(&u.folder()).route, "files");
    // Requests are read strictly: a stamp and a known way, or the end.
    for (text, want) in [("12 chip:m6\n", Some((12, "chip:m6"))), ("13 end", Some((13, "end"))), ("14", Some((14, "end"))), ("15 chip:m99", None), ("x files", None), ("16 ../etc", None)] {
        std::fs::write(u.folder().join("lighting-test"), text).unwrap();
        assert_eq!(lighting::test_asked(&u.folder()), want.map(|(a, r)| (a, r.to_string())), "{text}");
    }
    done("guided-files");
}

#[test]
fn the_lighting_check_writes_one_report_with_everything_and_says_where() {
    let mut u = ally_x("check", Mode::Guided);
    mode(&u, "dev/hidraw3", 0o602);
    u.set_host_lights(HostLights { asked: true, allyhub: Some(AllyHub { controller: "allyhub".into(), hid_method: "m6".into(), encoding: "hid".into() }), allyhub_agent: true, decky_running: false, rule: Some(turbinehub_helper::policy::Access::Lighting.rule().trim().to_string()), reapply: "enabled".into() });
    lighting::save_state(&u.data, crate::now() - 5, &["driving the lights: yes".to_string(), "chip reports sent: 41, not taken: 0".to_string()]);
    lighting::save_test(&u.data, crate::now() - 60, &["Works: way 1 of 5.".to_string()]);
    let d = parse(&u.act("light-report", "", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Run the lighting check?"));
    let plan = u.confirm().unwrap();
    let file = lighting::report_file(&u);
    // One press, one step, and the note says where the file is.
    assert_eq!(plan.steps, [Step::Report { game: lighting::REPORT.into() }]);
    assert_eq!(plan.done, format!("Lighting report written: {}", file.display()));
    assert!(file.ends_with("turbinehub/reports/TurbineHub-lighting-report.txt"));
    u.finish(&plan).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    for want in [
        "Screen: Lighting",
        "way in use: lighting chip, route m6 (HueSync custom colors) (Ally Hub's saved choice); route setting [chip:m6]",
        "driving: choices saved yes, background helper running, lights left to nobody",
        "others: Ally Hub settings found (controller allyhub, hid_method m6, encoding hid), its agent active; HueSync not installed, Decky Loader not running; setting the lights now: Ally Hub",
        "helper: not installed, lighting rule in its record: yes, rule file on the host: [SUBSYSTEM==\"leds\"",
        "unit that opens the lights at every start and wake: enabled",
        "step: ally:rgb:joystick_rings open to you for chip:m6: yes",
        "ally:rgb:joystick_rings: zones 4 index [rgb rgb rgb rgb]",
        "driven by lighting chip /dev/hidraw3 (Dynamic Lighting /dev/hidraw2), route m6 (HueSync custom colors), way picked",
        "  ally:rgb:joystick_rings/multi_max_intensity: mode 644, holds [255 255 255 255]",
        "hid hidraw3 0b05:1b4c: lighting commands, mode 602, usages 0001/0006, ff31/0080",
        "hid hidraw2 0b05:1b4c: Dynamic Lighting, mode 600, usages 0059/0001",
        "hid hidraw0 0b05:19b6: lighting commands, mode 600",
        "background helper's notes (",
        "  chip reports sent: 41, not taken: 0",
        "last light test (",
        "  Works: way 1 of 5.",
        "choices: {\"effect\":",
    ] {
        assert!(text.contains(want), "missing [{want}] in:\n{text}");
    }
    // Written again: the same file, replaced.
    parse(&u.act("light-report", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    u.finish(&plan).unwrap();
    assert_eq!(std::fs::read_dir(file.parent().unwrap()).unwrap().count(), 1);
    done("check");

    // A light on its files with the background helper off: the look's first moment is written first, so the report
    // can say what the files kept of it. Closed files are left alone (the report says they are closed).
    let mut u = ui("check-files", Mode::Guided);
    u.sample_lights("open");
    u.agent_on = Some(false);
    parse(&u.act("light-report", "", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps.len(), 3);
    assert!(matches!(&plan.steps[0], Step::HostWrite { path, .. } if path.ends_with("/multi_intensity")) && matches!(plan.steps[2], Step::Report { .. }));
    // As the app would have written them.
    for st in &plan.steps {
        if let Step::HostWrite { path, value } = st {
            std::fs::write(u.ctx.root.join(path.trim_start_matches('/')), format!("{value}\n")).unwrap();
        }
    }
    u.finish(&plan).unwrap();
    let text = std::fs::read_to_string(lighting::report_file(&u)).unwrap();
    assert!(text.contains("write and read back: /sys/class/leds/ally:rgb:joystick_rings/multi_intensity written [") && text.contains("]: kept"), "{text}");
    assert!(text.contains("others: the host wasn't asked yet") || text.contains("others: Ally Hub not found"), "{text}");
    u.sample_lights("closed");
    u.agent_on = Some(false);
    parse(&u.act("light-report", "", "")).unwrap();
    assert_eq!(u.confirm().unwrap().steps.len(), 1);
    let text = super::games::report_text(&u, lighting::REPORT, "now");
    assert!(text.contains("open to you for files: NO, closed: /sys/class/leds/ally:rgb:joystick_rings/multi_intensity (mode 644)"), "{text}");
    done("check-files");
}

#[test]
fn every_lighting_picture_shows_its_state() {
    // The states the screenshot list asks for exist on the made-up machine.
    let shots: Vec<&str> = Ui::shots().into_iter().map(|(name, _)| name).collect();
    for want in ["lighting-other-app", "lighting-other-huesync", "dialog-lighting-other", "lighting-left-to-other", "lighting-closed", "dialog-lighting-open", "lighting-stuck", "lighting-test", "lighting-test-veteran", "dialog-lighting-check", "home-lights-other", "home-lights-stuck"] {
        assert!(shots.contains(&want), "{want}");
    }
    let mut u = ui("shots", Mode::Guided);
    u.home_all = true;
    u.sample_lights("stuck");
    assert_eq!(titles(&mut u)[0], "The lights stay closed");
    assert!(parse(&u.page("home", "")).unwrap().to_text().contains("The lights stay closed"));
    u.sample_lights("allyhub");
    assert!(parse(&u.page("home", "")).unwrap().to_text().contains("Ally Hub is setting the lights too"));
    u.sample_lights("open");
    assert!(checkup(&u, "lights").is_none() && checkup(&u, "lights-rival").is_none());
    done("shots");
}
