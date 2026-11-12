//! Graphics card undervolting in My machine: Veteran mode only, folded, warnings first, a new value only ever tried,
//! and every state of the made-up RX 9070 XT.
use super::plan::Step;
use super::*;
use crate::fake::Preset;
use crate::json::parse;

fn dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("turbinehub-uv-{tag}-{}", std::process::id()))
}

fn ui(tag: &str, preset: Preset, mode: Mode, state: &str) -> Ui {
    let dir = dir(tag);
    let mut u = Ui::sample(&dir, preset, Some(mode));
    u.sample_with_helper();
    if !state.is_empty() {
        u.sample_undervolt(state);
    }
    u
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

fn rows(j: &Json) -> Vec<Json> {
    j.get("sections").map(Json::as_array).unwrap_or(&[]).iter().flat_map(|sec| sec.get("rows").map(Json::as_array).unwrap_or(&[]).to_vec()).collect()
}

fn ids(j: &Json) -> Vec<String> {
    rows(j).iter().filter_map(|r| r.str_at("id").map(String::from)).collect()
}

fn row(j: &Json, id: &str) -> Json {
    rows(j).into_iter().find(|r| r.str_at("id") == Some(id)).unwrap_or_else(|| panic!("no row {id}: {:?}", ids(j)))
}

fn gpu(u: &mut Ui) -> Json {
    parse(&u.page("module:gpu", "")).unwrap()
}

fn act(u: &mut Ui, a: &str, v: &str) -> Json {
    parse(&u.act(a, "", v)).unwrap()
}

fn host_lines(u: &Ui) -> Vec<String> {
    u.pending.as_ref().map(|p| p.steps.iter().filter_map(|s| if let Step::Host(c) = s { Some(c.join(" ")) } else { None }).collect()).unwrap_or_default()
}

#[test]
fn guided_mode_and_handhelds_show_nothing_of_it() {
    for state in ["", "on", "probation", "tripped"] {
        let mut u = ui("guided", Preset::Desktop, Mode::Guided, state);
        let page = gpu(&mut u);
        assert!(ids(&page).iter().all(|i| !i.starts_with("uv-") && !i.contains("tripped") && !i.contains("warn")), "{state}: {:?}", ids(&page));
        assert!(!u.page("home", "").contains("voltage"));
        done("guided");
    }
    // A handheld's chip is never offered it, in either mode.
    let mut u = ui("handheld", Preset::Handheld, Mode::Veteran, "on");
    for p in ["machine", "module:power", "home"] {
        let text = u.page(p, "");
        assert!(!text.contains("uv-") && !text.contains("Undervolting"), "{p}");
    }
    done("handheld");
}

#[test]
fn veteran_finds_it_folded_and_the_warnings_come_before_anything() {
    let mut u = ui("fold", Preset::Desktop, Mode::Veteran, "");
    let page = gpu(&mut u);
    let section = page.get("sections").map(Json::as_array).unwrap().last().unwrap().clone();
    assert_eq!(section.str_at("title"), Some("Advanced"));
    assert_eq!(ids(&page).iter().filter(|i| i.starts_with("uv-")).collect::<Vec<_>>(), ["uv-fold"]);
    act(&mut u, "uv-fold", "");
    let page = gpu(&mut u);
    let order = ids(&page);
    let at = |id: &str| order.iter().position(|i| i == id).unwrap_or_else(|| panic!("{id}: {order:?}"));
    assert!(at("uv-fold") < at("none:warn-crash") && at("none:warn-crash") < at("none:warn-tuning") && at("none:warn-tuning") < at("uv-on"));
    let warn = row(&page, "none:warn-crash");
    assert_eq!((warn.get("alert").and_then(Json::as_bool), warn.str_at("tone")), (Some(true), Some("stop")));
    let words = format!("{}{}", warn.str_at("line").unwrap(), row(&page, "none:warn-tuning").str_at("line").unwrap());
    for must in ["crash", "black", "Nothing is damaged", "turns the lower voltage back off by itself", "restart", "system log", "Valve", "varies"] {
        assert!(words.contains(must), "{must}");
    }
    assert!(row(&page, "none:warn-tuning").str_at("title").unwrap().contains("RX 7000 and RX 9000 only"));
    done("fold");
}

#[test]
fn turning_tuning_on_asks_twice_and_says_what_it_writes() {
    let mut u = ui("on", Preset::Desktop, Mode::Veteran, "off");
    let d = act(&mut u, "uv-on", "");
    let dlg = d.get("dialog").unwrap();
    assert_eq!(dlg.get("danger").and_then(Json::as_bool), Some(true));
    let text = dlg.to_text();
    for must in ["/etc/modprobe.d/turbinehub-amdgpu.conf", "ppfeaturemask=0xfff7ffff", "Needs a restart", "system log", "Valve"] {
        assert!(text.contains(must), "{must}: {text}");
    }
    assert!(host_lines(&u).iter().any(|l| l.contains("apply") && l.contains("\"gpu-overdrive\"") && l.contains("0xfff7ffff") && l.contains("7.2.7-valve2-2-neptune-72")));
    assert_eq!(u.pending.as_ref().unwrap().next, Some(("uv-restart".into(), "on".into(), String::new())));
    // The second step: restart now, or later.
    let d = parse(&u.act("uv-restart", "on", "")).unwrap();
    assert_eq!(d.get("dialog").unwrap().str_at("cancel"), Some("Later"));
    assert_eq!(host_lines(&u), ["flatpak-spawn --host systemctl reboot"]);
    done("on");
    // Waiting for the restart: the page says so and offers it.
    let mut u = ui("pending", Preset::Desktop, Mode::Veteran, "pending");
    let page = gpu(&mut u);
    assert_eq!(row(&page, "uv-restart").str_at("badge"), Some("Restart needed"));
    assert!(ids(&page).contains(&"uv-off".to_string()) && !ids(&page).contains(&"uv-try".to_string()));
    done("pending");
}

#[test]
fn a_new_value_is_only_tried_and_counts_down_to_going_back() {
    let mut u = ui("try", Preset::Desktop, Mode::Veteran, "on");
    let page = gpu(&mut u);
    let slider = row(&page, "uv-try");
    let num = |k: &str| slider.get(k).and_then(Json::as_f64).unwrap();
    assert_eq!((num("min"), num("max"), num("step"), num("value")), (-150.0, 0.0, 5.0, 0.0));
    assert_eq!(slider.get("showFrom").and_then(Json::as_bool), Some(true));
    let marks: Vec<f64> = slider.get("notes").map(Json::as_array).unwrap().iter().filter_map(|x| x.get("below").and_then(Json::as_f64)).collect();
    assert_eq!(marks, [-50.0, -100.0]);
    // Try: shown first, run as a job nothing records, then the countdown.
    let d = act(&mut u, "uv-try", "-30");
    let dlg = d.get("dialog").unwrap();
    assert_eq!(dlg.str_at("title"), Some("Try -30 mV for 15 seconds?"));
    assert_eq!(dlg.get("danger").and_then(Json::as_bool), Some(false));
    assert!(dlg.to_text().contains("0 mV (as made) \u{2192} -30 mV"));
    let lines = host_lines(&u);
    assert!(lines.len() == 1 && lines[0].contains(" gpu ") && lines[0].contains("\"do\":\"try\"") && lines[0].contains("\"offset\":-30") && lines[0].contains("\"slot\":\"0000:03:00.0\""), "{lines:?}");
    assert_eq!(u.pending.as_ref().unwrap().next, Some(("uv-keep-ask".into(), String::new(), "-30".into())));
    let keep = act(&mut u, "uv-keep-ask", "-30");
    let k = keep.get("keep").unwrap();
    assert_eq!((k.get("seconds").and_then(Json::as_f64), k.str_at("confirm"), k.str_at("cancel"), k.str_at("act"), k.str_at("keepAct"), k.str_at("keepValue")), (Some(15.0), Some("Keep"), Some("Go back"), Some("uv-back"), Some("uv-keep"), Some("-30")));
    assert!(k.str_at("text").unwrap().contains("{n}"));
    // Keep records it at once, with no question; Go back (or the end of the countdown) runs at once too.
    let kept = act(&mut u, "uv-keep", "-30");
    assert_eq!(kept.get("go").and_then(Json::as_bool), Some(true));
    assert!(host_lines(&u)[0].contains("apply") && host_lines(&u)[0].contains("\"gpu-voltage\""));
    let back = act(&mut u, "uv-back", "");
    assert!(back.get("go").is_some() && host_lines(&u)[0].contains("\"do\":\"back\""));
    // Deeper steps warn more, and the confirm button is no longer the one highlighted.
    let deep = act(&mut u, "uv-try", "-120");
    let text = deep.get("dialog").unwrap().to_text();
    assert!(text.contains("Below -50 mV") && text.contains("Below -100 mV"));
    assert_eq!(deep.get("dialog").unwrap().get("danger").and_then(Json::as_bool), Some(true));
    // Past Turbine Hub's floor, off the steps, or what is set already: refused in words.
    for bad in ["-155", "-33", "0"] {
        assert!(act(&mut u, "uv-try", bad).get("toast").is_some(), "{bad}");
    }
    done("try");
}

#[test]
fn safe_reset_is_one_press_and_turning_tuning_off_restarts() {
    let mut u = ui("reset", Preset::Desktop, Mode::Veteran, "probation");
    let r = act(&mut u, "uv-reset", "");
    assert!(r.get("dialog").is_none() && r.get("go").is_some());
    assert!(host_lines(&u)[0].contains("\"do\":\"reset\""));
    let off = act(&mut u, "uv-off", "");
    let text = off.get("dialog").unwrap().to_text();
    assert!(text.contains("-30 mV \u{2192} 0 mV (as made)") && text.contains("Needs a restart"));
    let lines = host_lines(&u);
    assert!(lines[0].contains("\"do\":\"reset\"") && lines[1].contains(" undo "));
    assert_eq!(u.pending.as_ref().unwrap().next, Some(("uv-restart".into(), "off".into(), String::new())));
    done("reset");
}

#[test]
fn probation_settling_and_a_trip_show_on_the_page_and_in_the_checkup() {
    let mut u = ui("probation", Preset::Desktop, Mode::Veteran, "probation");
    let page = gpu(&mut u);
    assert_eq!(row(&page, "uv-fold").str_at("badge"), Some("-30 mV on probation"));
    assert!(row(&page, "none:now").str_at("line").unwrap().contains("on probation"));
    let item = crate::checkup::items(&u.machine(), &u.facts_now()).into_iter().find(|i| i.id == "undervolt").unwrap();
    assert_eq!(item.level, crate::checkup::Level::Tip);
    assert!(item.title.contains("probation"));
    assert_eq!(parse(&u.act("checkup", "gpu", "")).unwrap().str_at("open"), Some("module:gpu"));
    done("probation");
    let mut u = ui("settled", Preset::Desktop, Mode::Veteran, "settled");
    assert_eq!(row(&gpu(&mut u), "uv-fold").str_at("tone"), Some("ok"));
    assert!(!crate::checkup::items(&u.machine(), &u.facts_now()).iter().any(|i| i.id == "undervolt"));
    done("settled");
    let mut u = ui("tripped", Preset::Desktop, Mode::Veteran, "tripped");
    u.uv_open = false;
    // Folded, the trip still shows.
    let page = gpu(&mut u);
    assert_eq!(row(&page, "none:tripped").str_at("line"), Some(undervolt::TRIPPED_WORDS));
    let item = crate::checkup::items(&u.machine(), &u.facts_now()).into_iter().find(|i| i.id == "undervolt").unwrap();
    assert_eq!(item.level, crate::checkup::Level::Look);
    assert!(item.line.contains("-60 mV") && item.line.contains("Try a smaller step?"));
    // A try nobody keeps goes back to 0, not to the value that tripped.
    let k = act(&mut u, "uv-keep-ask", "-40");
    assert!(k.get("keep").unwrap().str_at("text").unwrap().contains("Going back to 0 mV (as made)"));
    done("tripped");
}

#[test]
fn rx_6000_is_named_unsupported_and_the_power_limit_stops_at_the_cards_own() {
    let mut u = ui("unsupported", Preset::Desktop, Mode::Veteran, "unsupported");
    let page = gpu(&mut u);
    assert!(row(&page, "none:unsupported").str_at("line").unwrap().contains("RX 6000"));
    assert!(!ids(&page).contains(&"uv-on".to_string()));
    done("unsupported");
    // Overdrive off: the driver refuses more than the card's own 304 W, so the slider stops there.
    let mut u = ui("cap", Preset::Desktop, Mode::Veteran, "");
    assert_eq!(row(&gpu(&mut u), "gpu-power").get("max").and_then(Json::as_f64), Some(304.0));
    assert!(matches!(crate::modules::gpu_power(&u.machine(), 400), Some(crate::helper::Setting::Sysfs { value, .. }) if value == "304000000"));
    done("cap");
    let mut u = ui("cap-od", Preset::Desktop, Mode::Veteran, "on");
    assert_eq!(row(&gpu(&mut u), "gpu-power").get("max").and_then(Json::as_f64), Some(340.0));
    done("cap-od");
}
