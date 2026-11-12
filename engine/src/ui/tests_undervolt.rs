//! Graphics card undervolting in My machine: Veteran mode only, folded, warnings first, a new value only ever tried
//! and checked twice before it counts (10 seconds, then 5 minutes and 60 more), on until the restart and never put
//! back by itself, and every state of the made-up RX 9070 XT. The checks' clock is driven by hand.
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
    for state in ["", "on", "checking", "session", "tripped"] {
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

fn news(u: &mut Ui) -> Json {
    u.uv_tick().map(|t| parse(&t).unwrap()).unwrap_or(Json::Null)
}

/// Moves the checks' clock on by `secs` from where the made-up machine set it.
fn at(u: &mut Ui, ms: u64) {
    u.uv.clock.set(1_000_000 + ms);
}

fn od(u: &Ui, offset: i32) {
    std::fs::write(u.ctx.root.join(format!("sys/devices/pci0000:00/{}/pp_od_clk_voltage", crate::fake::GPU_SLOT)), turbinehub_helper::gpu::od_rx9000(offset)).unwrap();
}

fn put(u: &Ui, abs: &str, text: &str) {
    let f = u.ctx.root.join(abs.trim_start_matches('/'));
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(f, text).unwrap();
}

const BOOT: &str = "5e0c1d2a-0000-4000-8000-0000000000b1";

/// The helper's crash mark, as a try of `offset` leaves it in this boot.
fn armed(u: &Ui, offset: i32) {
    put(u, turbinehub_helper::gpu::ARMED, &turbinehub_helper::gpu::armed_text(BOOT, 1000, offset));
}

fn sleeps(u: &Ui, n: u64) {
    put(u, crate::agent::SUSPEND_COUNT, &format!("{n}\n"));
}

/// The job the window is handed, as one line.
fn run(u: &mut Ui) -> String {
    let line = u.uv_take_run().map(|l| l.join(" ")).unwrap_or_default();
    if !line.is_empty() {
        u.uv_done(Ok(()));
    }
    line
}

fn status(u: &Ui) -> String {
    parse(&u.status()).unwrap().str_at("check").unwrap_or("").to_string()
}

/// Try -30 mV on the "on" made-up card and land in the 10 second question, as the try plan's next press does.
fn tried(tag: &str, state: &str, offset: i32) -> (Ui, Json) {
    let mut u = ui(tag, Preset::Desktop, Mode::Veteran, state);
    sleeps(&u, 3);
    news(&mut u);
    let k = act(&mut u, "uv-keep-ask", &offset.to_string());
    armed(&u, offset);
    (u, k.get("keep").cloned().unwrap_or(Json::Null))
}

#[test]
fn try_asks_for_ten_seconds_and_no_answer_goes_back() {
    let mut u = ui("try", Preset::Desktop, Mode::Veteran, "on");
    let page = gpu(&mut u);
    let slider = row(&page, "uv-try");
    let num = |k: &str| slider.get(k).and_then(Json::as_f64).unwrap();
    assert_eq!((num("min"), num("max"), num("step"), num("value")), (-150.0, 0.0, 5.0, 0.0));
    assert_eq!(slider.get("showFrom").and_then(Json::as_bool), Some(true));
    let marks: Vec<f64> = slider.get("notes").map(Json::as_array).unwrap().iter().filter_map(|x| x.get("below").and_then(Json::as_f64)).collect();
    assert_eq!(marks, [-50.0, -100.0]);
    assert!(row(&page, "none:now").str_at("line").unwrap().contains("Nothing turns a lower one on by itself at start"));
    // Try: shown first, run as a job nothing records, then the question.
    let d = act(&mut u, "uv-try", "-30");
    let dlg = d.get("dialog").unwrap();
    assert_eq!(dlg.str_at("title"), Some("Try -30 mV?"));
    assert_eq!(dlg.get("danger").and_then(Json::as_bool), Some(false));
    let text = dlg.to_text();
    for must in ["0 mV (as made) \u{2192} -30 mV", "10 seconds", "5 minutes", "Nothing turns a lower voltage on by itself at start, login or wake"] {
        assert!(text.contains(must), "{must}: {text}");
    }
    let lines = host_lines(&u);
    assert!(lines.len() == 1 && lines[0].contains(" gpu ") && lines[0].contains("\"do\":\"try\"") && lines[0].contains("\"offset\":-30") && lines[0].contains("\"slot\":\"0000:03:00.0\""), "{lines:?}");
    assert_eq!(u.pending.as_ref().unwrap().next, Some(("uv-keep-ask".into(), String::new(), "-30".into())));
    done("try");
    // The question: 10 seconds, kept by the engine's clock; the window only counts down.
    let (mut u, k) = tried("ask", "on", -30);
    assert_eq!(k.str_at("title"), Some("Keep -30 mV?"));
    assert!(k.str_at("text").unwrap().starts_with("Going back to 0 mV in {n} s."));
    assert_eq!((k.get("seconds").and_then(Json::as_f64), k.get("engine").and_then(Json::as_bool), k.str_at("confirm"), k.str_at("cancel"), k.str_at("act"), k.str_at("keepAct")), (Some(10.0), Some(true), Some("Keep"), Some("Go back"), Some("uv-back"), Some("uv-keep")));
    assert_eq!(status(&u), "Trying -30 mV: 10 s");
    at(&mut u, 3_000);
    assert_eq!(news(&mut u).get("left").and_then(Json::as_f64), Some(7.0));
    assert!(u.uv_take_run().is_none());
    // Nobody answers: back to 0 at the tenth second, the question closes, and Keep is too late.
    at(&mut u, 10_000);
    let n = news(&mut u);
    assert_eq!(n.get("close").and_then(Json::as_bool), Some(true));
    assert!(n.str_at("toast").unwrap().contains("No answer: back to 0 mV"));
    let line = run(&mut u);
    assert!(line.contains("\"do\":\"back\"") && line.contains("\"offset\":0"), "{line}");
    assert!(act(&mut u, "uv-keep", "-30").str_at("toast").unwrap().contains("over already"));
    assert_eq!(status(&u), "");
    // Past Turbine Hub's floor, off the steps, or what is set already: refused in words.
    for bad in ["-155", "-33", "0"] {
        assert!(act(&mut u, "uv-try", bad).get("toast").is_some(), "{bad}");
    }
    // Deeper steps warn more, and the confirm button is no longer the one highlighted.
    let deep = act(&mut u, "uv-try", "-120");
    let text = deep.get("dialog").unwrap().to_text();
    assert!(text.contains("Below -50 mV") && text.contains("Below -100 mV"));
    assert_eq!(deep.get("dialog").unwrap().get("danger").and_then(Json::as_bool), Some(true));
    done("ask");
}

#[test]
fn keep_starts_five_minutes_then_asks_again_and_only_then_it_is_on_until_the_restart() {
    let (mut u, _) = tried("flow", "on", -30);
    let kept = act(&mut u, "uv-keep", "-30");
    assert!(kept.str_at("toast").unwrap().contains("5 minutes"));
    assert!(u.uv_take_run().is_none(), "Keep writes nothing");
    assert_eq!(status(&u), "Checking -30 mV: 5:00 left");
    at(&mut u, 48_000);
    assert_eq!(status(&u), "Checking -30 mV: 4:12 left");
    let page = gpu(&mut u);
    let stop = row(&page, "uv-stop");
    assert_eq!((stop.str_at("title"), stop.str_at("button"), stop.str_at("liveTitle")), (Some("Checking -30 mV: 4:12 left"), Some("Stop"), Some("check")));
    assert_eq!(row(&page, "uv-fold").str_at("badge"), Some("Checking -30 mV"));
    // Folded, the check and its Stop still show.
    u.uv_open = false;
    assert!(ids(&gpu(&mut u)).contains(&"uv-stop".to_string()));
    u.uv_open = true;
    // The card is watched all along: still -30, nothing to do.
    od(&u, -30);
    news(&mut u);
    assert!(u.uv_take_run().is_none());
    // The 5 minutes end: the second question, 60 seconds.
    at(&mut u, 3_000 + 300_000);
    let n = news(&mut u);
    let k = n.get("keep").unwrap_or_else(|| panic!("{}", n.to_text()));
    assert_eq!(k.str_at("title"), Some("Still running well at -30 mV?"));
    assert_eq!((k.str_at("confirm"), k.str_at("cancel"), k.get("seconds").and_then(Json::as_f64)), (Some("Keep it for this session"), Some("Go back to 0 mV"), Some(60.0)));
    assert!(k.str_at("text").unwrap().starts_with("Going back to 0 mV in {n} s."));
    // The same question again when asked (the pictures, a window that lost it).
    assert_eq!(act(&mut u, "uv-question", "").get("keep").and_then(|k| k.str_at("title")), Some("Still running well at -30 mV?"));
    let on = act(&mut u, "uv-keep", "-30");
    assert!(on.str_at("toast").unwrap().contains("on until the machine restarts"));
    assert!(u.uv_take_run().is_none());
    let page = gpu(&mut u);
    assert_eq!(row(&page, "none:now").str_at("line"), Some(undervolt::UNTIL_RESTART));
    assert_eq!(row(&page, "uv-fold").str_at("badge"), Some("-30 mV until restart"));
    assert!(!ids(&page).contains(&"uv-stop".to_string()) && !ids(&page).contains(&"uv-again".to_string()));
    assert_eq!(status(&u), "");
    // Closing the window leaves a value that passed on: it is the session's.
    assert!(u.uv_close().is_none());
    // The window opened again in the same boot finds it on; after a restart it is gone, and offered again.
    let w = undervolt::Watch::load(&u.data, &u.ctx.root);
    assert_eq!((w.session.on, w.note.as_ref().map(|x| x.offset)), (Some(-30), Some(-30)));
    put(&u, turbinehub_helper::gpu::BOOT_ID, "another-boot\n");
    od(&u, 0);
    u.uv = undervolt::Watch::load(&u.data, &u.ctx.root);
    assert_eq!(u.uv.session.on, None);
    let page = gpu(&mut u);
    let again = row(&page, "uv-again");
    assert_eq!(again.str_at("title"), Some("Use -30 mV again"));
    assert!(again.str_at("line").unwrap().contains("only the 10 second question"));
    done("flow");
}

#[test]
fn the_second_question_unanswered_stop_and_closing_go_back() {
    // The 60 seconds run out.
    let (mut u, _) = tried("again", "on", -30);
    act(&mut u, "uv-keep", "-30");
    at(&mut u, 300_000);
    assert!(news(&mut u).get("keep").is_some());
    at(&mut u, 360_000);
    let n = news(&mut u);
    assert!(n.get("close").is_some() && n.str_at("toast").unwrap().contains("No answer"));
    assert!(run(&mut u).contains("\"do\":\"back\""));
    done("again");
    // Stop in the middle of the 5 minutes: at once.
    let (mut u, _) = tried("stop", "on", -30);
    act(&mut u, "uv-keep", "-30");
    at(&mut u, 60_000);
    let s = act(&mut u, "uv-stop", "");
    assert_eq!(s.str_at("toast"), Some("Stopped: back to 0 mV"));
    assert!(run(&mut u).contains("\"offset\":0"));
    assert!(u.uv_close().is_none());
    done("stop");
    // Go back (or B) in the 10 second question goes back to the value that passed earlier in this session.
    let (mut u, k) = tried("back", "session", -40);
    assert!(k.str_at("text").unwrap().starts_with("Going back to -30 mV"));
    act(&mut u, "uv-back", "");
    assert!(run(&mut u).contains("\"offset\":-30"));
    assert_eq!(u.uv.session.on, Some(-30));
    done("back");
    // Closing the window during the 5 minutes: the command to run before it goes, and nothing left underway.
    let (mut u, _) = tried("close", "on", -30);
    act(&mut u, "uv-keep", "-30");
    let line = u.uv_close().unwrap().join(" ");
    assert!(line.contains("\"do\":\"back\"") && line.contains("\"offset\":0"), "{line}");
    assert!(undervolt::Watch::load(&u.data, &u.ctx.root).session.underway.is_none());
    done("close");
    // A window killed in the middle of a check: its next start goes back, and says why.
    let (mut u, _) = tried("killed", "on", -30);
    act(&mut u, "uv-keep", "-30");
    u.uv = undervolt::Watch::load(&u.data, &u.ctx.root);
    let n = news(&mut u);
    assert!(n.str_at("toast").unwrap().contains("closed during the check last time"));
    assert!(run(&mut u).contains("\"offset\":0"));
    done("killed");
}

#[test]
fn a_driver_reset_or_a_sleep_during_the_check_goes_back_and_says_why() {
    // The card reads 0 with no sleep while the mark still names -30: the driver reset it. The helper notes it.
    let (mut u, _) = tried("reset-check", "on", -30);
    act(&mut u, "uv-keep", "-30");
    od(&u, 0);
    at(&mut u, 2_000);
    let n = news(&mut u);
    assert!(n.str_at("toast").unwrap().contains("driver reset the card during the check"));
    assert!(n.get("close").is_some());
    assert!(run(&mut u).contains("\"do\":\"trip\""));
    assert_eq!(u.uv.session.on, None);
    done("reset-check");
    // A sleep in the middle: back to 0.
    let (mut u, _) = tried("sleep-check", "on", -30);
    act(&mut u, "uv-keep", "-30");
    sleeps(&u, 4);
    at(&mut u, 2_000);
    assert!(news(&mut u).str_at("toast").unwrap().contains("slept during the check"));
    assert!(run(&mut u).contains("\"offset\":0"));
    done("sleep-check");
}

#[test]
fn after_a_wake_a_value_is_looked_at_and_reported_never_put_back() {
    let mut u = ui("wake", Preset::Desktop, Mode::Veteran, "session");
    armed(&u, -30);
    sleeps(&u, 5);
    news(&mut u);
    // A sleep that kept it: said, nothing written.
    sleeps(&u, 6);
    at(&mut u, 2_000);
    assert_eq!(news(&mut u).str_at("toast"), Some("-30 mV is still on after the sleep"));
    assert!(u.uv_take_run().is_none());
    // A sleep that lost it (the sleep unit cleared the mark too): said, and offered on the page. Never written.
    od(&u, 0);
    let _ = std::fs::remove_file(u.ctx.root.join(turbinehub_helper::gpu::ARMED.trim_start_matches('/')));
    sleeps(&u, 7);
    at(&mut u, 4_000);
    assert!(news(&mut u).str_at("toast").unwrap().contains("off after the sleep"));
    assert!(u.uv_take_run().is_none());
    assert_eq!(row(&gpu(&mut u), "uv-again").str_at("title"), Some("Use -30 mV again"));
    done("wake");
    // With no sleep, a 0 while the mark still names the value is a driver reset.
    let mut u = ui("wake-reset", Preset::Desktop, Mode::Veteran, "session");
    armed(&u, -30);
    sleeps(&u, 5);
    news(&mut u);
    od(&u, 0);
    at(&mut u, 2_000);
    assert!(news(&mut u).str_at("toast").unwrap().contains("driver reset"));
    assert!(run(&mut u).contains("\"do\":\"trip\""));
    done("wake-reset");
}

#[test]
fn use_again_goes_through_try_and_skips_the_five_minutes_only_on_the_same_kernel() {
    let mut u = ui("use-again", Preset::Desktop, Mode::Veteran, "again");
    let d = act(&mut u, "uv-again", "");
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Try -30 mV?"));
    assert!(d.get("dialog").unwrap().to_text().contains("passed the 5 minute check on this system before"));
    assert_eq!(u.pending.as_ref().unwrap().next, Some(("uv-keep-ask".into(), "short".into(), "-30".into())));
    // The 10 second question still comes, and Keep there turns it on at once.
    let k = parse(&u.act("uv-keep-ask", "short", "-30")).unwrap();
    assert!(k.get("keep").unwrap().str_at("text").unwrap().contains("Keep turns it on until the machine restarts"));
    assert!(act(&mut u, "uv-keep", "-30").str_at("toast").unwrap().contains("until the machine restarts"));
    assert_eq!(u.uv.session.on, Some(-30));
    done("use-again");
    // Passed on another kernel: both checks again.
    let mut u = ui("use-again-kernel", Preset::Desktop, Mode::Veteran, "again");
    u.uv.note.as_mut().unwrap().kernel = "7.1.0-valve1".into();
    assert!(row(&gpu(&mut u), "uv-again").str_at("line").unwrap().contains("both checks"));
    act(&mut u, "uv-again", "");
    assert_eq!(u.pending.as_ref().unwrap().next, Some(("uv-keep-ask".into(), String::new(), "-30".into())));
    done("use-again-kernel");
}

#[test]
fn safe_reset_is_one_press_and_turning_tuning_off_restarts() {
    let mut u = ui("reset", Preset::Desktop, Mode::Veteran, "session");
    let off = act(&mut u, "uv-off", "");
    let text = off.get("dialog").unwrap().to_text();
    assert!(text.contains("-30 mV \u{2192} 0 mV (as made)") && text.contains("Needs a restart"));
    let lines = host_lines(&u);
    assert!(lines[0].contains("\"do\":\"reset\"") && lines[1].contains(" undo "));
    assert_eq!(u.pending.as_ref().unwrap().next, Some(("uv-restart".into(), "off".into(), String::new())));
    u.cancel();
    let r = act(&mut u, "uv-reset", "");
    assert!(r.get("dialog").is_none() && r.get("go").is_some());
    assert!(host_lines(&u)[0].contains("\"do\":\"reset\""));
    assert_eq!(u.uv.session.on, None);
    done("reset");
    // While a value is being checked, Stop comes first.
    let (mut u, _) = tried("off-checking", "on", -30);
    assert!(act(&mut u, "uv-off", "").str_at("toast").unwrap().contains("Stop it first"));
    assert!(act(&mut u, "uv-try", "-40").str_at("toast").unwrap().contains("Stop it first"));
    done("off-checking");
}

#[test]
fn a_trip_shows_on_the_page_and_in_the_checkup() {
    let mut u = ui("tripped", Preset::Desktop, Mode::Veteran, "tripped");
    u.uv_open = false;
    // Folded, the trip still shows.
    let page = gpu(&mut u);
    assert_eq!(row(&page, "none:tripped").str_at("line"), Some(undervolt::TRIPPED_WORDS));
    let item = crate::checkup::items(&u.machine(), &u.facts_now()).into_iter().find(|i| i.id == "undervolt").unwrap();
    assert_eq!(item.level, crate::checkup::Level::Look);
    assert!(item.line.contains("-60 mV") && item.line.contains("Try a smaller step?"));
    assert_eq!(parse(&u.act("checkup", "gpu", "")).unwrap().str_at("open"), Some("module:gpu"));
    // A value that passed and is on has no checkup item: the page says it plainly.
    done("tripped");
    let mut u = ui("session", Preset::Desktop, Mode::Veteran, "session");
    assert!(!crate::checkup::items(&u.machine(), &u.facts_now()).iter().any(|i| i.id == "undervolt"));
    assert_eq!(row(&gpu(&mut u), "uv-fold").str_at("tone"), Some("ok"));
    done("session");
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
