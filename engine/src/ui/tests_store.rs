//! The Store's rows (state and button on the row), the line of work as the screens see it, and the top bar's status.
use super::store;
use super::{HANDHELD, Ui};
use crate::fake::Preset;
use crate::json::{Json, parse};
use crate::settings::Mode;

fn ui(tag: &str, preset: Preset) -> Ui {
    let dir = std::env::temp_dir().join(format!("turbinehub-store-{tag}-{}", std::process::id()));
    Ui::sample(&dir, preset, Some(Mode::Guided))
}
fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("turbinehub-store-{tag}-{}", std::process::id())));
}
fn page(u: &mut Ui, section: &str) -> Json {
    parse(&u.page("store", section)).unwrap()
}
fn rows(j: &Json) -> Vec<Json> {
    j.get("sections").unwrap().as_array().iter().flat_map(|s| s.get("rows").unwrap().as_array().to_vec()).collect()
}
fn row(j: &Json, title: &str) -> Json {
    rows(j).into_iter().find(|r| r.str_at("title") == Some(title)).unwrap_or_else(|| panic!("no row {title}"))
}
/// What a row says and does: (status word, tone, button, press).
fn shows(r: &Json) -> (&str, &str, &str, &str) {
    (r.str_at("badge").unwrap_or(""), r.str_at("tone").unwrap_or(""), r.str_at("button").unwrap_or(""), r.str_at("act").unwrap_or(""))
}
fn offers(r: &Json) -> Vec<String> {
    r.get("offers").map(Json::as_array).unwrap_or(&[]).iter().filter_map(|o| o.str_at("title").map(String::from)).collect()
}
fn act(u: &mut Ui, action: &str, arg: &str) -> Json {
    parse(&u.act(action, arg, "")).unwrap()
}
fn status(u: &Ui) -> Json {
    parse(&u.status()).unwrap()
}
/// Asks for a thing and says yes to its dialog, as the app does: the plan joins the line of work.
fn confirm(u: &mut Ui, action: &str, arg: &str) -> Json {
    let a = act(u, action, arg);
    assert!(a.get("dialog").is_some(), "{action} {arg} asked nothing: {a:?}");
    let plan = u.confirm().unwrap();
    assert!(!u.runs_covered(&plan), "{action} {arg} would run behind the cover");
    u.enqueue(plan)
}

#[test]
fn every_row_says_its_state_and_carries_its_button() {
    let mut u = ui("rows", HANDHELD);
    let apps = page(&mut u, "apps");
    // Installed, with the version the host told: A opens it.
    assert_eq!(shows(&row(&apps, "Discord")), ("Installed 0.0.98", "ok", "Open", "store-open"));
    assert_eq!(offers(&row(&apps, "Discord")), ["Remove", "Details", "Where it comes from"]);
    // Not installed: A installs, through the dialog.
    let spotify = row(&apps, "Spotify");
    assert_eq!(shows(&spotify), ("Not installed", "", "Install", "store"));
    assert_eq!(offers(&spotify), ["Details", "Where it comes from"]);
    assert!(act(&mut u, "store", "com.spotify.Client").get("dialog").is_some());
    u.cancel();
    // An update, from the version here to the newest: A updates.
    let heroic = row(&page(&mut u, "launchers"), "Heroic Games Launcher");
    assert_eq!(shows(&heroic), ("Update from 2.17.2 to 2.18.0", "warn", "Update", "store-update"));
    // Without a version from the host it still says what it is.
    u.installed.push("org.videolan.VLC".into());
    u.updates.push("org.videolan.VLC".into());
    let apps = page(&mut u, "apps");
    assert_eq!(shows(&row(&apps, "VLC")).0, "Update ready");
    u.updates.retain(|x| x != "org.videolan.VLC");
    assert_eq!(shows(&row(&page(&mut u, "apps"), "VLC")).0, "Installed");
    // The name of a row never changes with its state, so the highlight stays on it.
    assert_eq!(row(&apps, "VLC").str_at("id"), Some("app:org.videolan.VLC"));
    assert_eq!(spotify.str_at("id"), Some("app:com.spotify.Client"));
    // Tiles and Decky Loader have theirs too.
    let tv = page(&mut u, "tv");
    assert_eq!(shows(&row(&tv, "Netflix")), ("Not added", "", "Add", "tile"));
    assert!(offers(&row(&tv, "Netflix")).contains(&"Details".to_string()));
    std::fs::remove_file(u.ctx.home.join("homebrew/services/PluginLoader")).unwrap();
    let decky = page(&mut u, "decky");
    assert_eq!(shows(&row(&decky, "Decky Loader")), ("Not installed", "", "Install", "decky"));
    // Where a thing comes from is one line.
    assert_eq!(act(&mut u, "store-source", "com.spotify.Client").str_at("toast"), Some("Spotify comes from Flathub (com.spotify.Client)."));
    assert!(act(&mut u, "store-source", "tile:netflix").str_at("toast").unwrap().contains("netflix.com"));
    assert!(act(&mut u, "store-source", "decky").str_at("toast").unwrap().contains("decky-loader"));
    done("rows");
}

#[test]
fn decky_says_installed_and_turned_off() {
    let mut u = ui("decky", HANDHELD);
    std::fs::create_dir_all(u.ctx.home.join("homebrew/plugins/CSS Loader")).unwrap();
    let j = page(&mut u, "decky");
    let r = row(&j, "Decky Loader");
    assert_eq!(shows(&r), ("Installed", "ok", "", "open"));
    assert_eq!(offers(&r), ["Repair or update", "Turn all plugins off", "Details", "Where it comes from"]);
    assert_eq!(row(&j, "CSS Loader").str_at("badge"), Some("On"));
    // In Installed too.
    assert_eq!(shows(&row(&page(&mut u, "installed"), "Decky Loader")).0, "Installed");
    done("decky");
}

#[test]
fn the_sections_are_chips_and_update_all_is_the_main_button() {
    let mut u = ui("chips", HANDHELD);
    let j = page(&mut u, "apps");
    let chips: Vec<&str> = j.get("chips").unwrap().as_array().iter().filter_map(|c| c.str_at("title")).collect();
    assert_eq!(chips, ["Essentials", "Apps", "Decky plugins", "Launchers", "TV and streaming", "Emulation", "Installed"]);
    assert_eq!(j.get("index").and_then(Json::as_f64), Some(1.0));
    assert!(j.get("side").is_none());
    // Installed says how many updates wait in it.
    let installed = &j.get("chips").unwrap().as_array()[6];
    assert_eq!((installed.str_at("chip"), installed.str_at("tone")), (Some("1 update"), Some("warn")));
    // With an update ready, Start is Update all in every section, Installed included.
    for sec in ["", "apps", "installed"] {
        let j = page(&mut u, sec);
        assert_eq!(j.get("primary").unwrap().str_at("act"), Some("update-all"), "{sec}");
        let hints: Vec<&str> = j.get("hints").unwrap().as_array().iter().filter_map(|h| h.str_at("k")).collect();
        assert_eq!(hints, ["start", "y", "x", "view"], "{sec}");
    }
    // Nothing to update: no main button, and asking anyway says so.
    u.updates.clear();
    let j = page(&mut u, "installed");
    assert!(j.get("primary").is_none());
    assert_eq!(j.get("chips").unwrap().as_array()[6].str_at("chip"), Some(""));
    assert_eq!(act(&mut u, "update-all", "").str_at("toast"), Some("Everything is up to date."));
    // Help names the sections' keys on the Store and on My machine, and not on a page without sections.
    let titles = |j: &Json| -> Vec<String> { j.get("sections").unwrap().as_array().iter().flat_map(|s| s.get("rows").unwrap().as_array().to_vec()).filter_map(|r| r.str_at("title").map(String::from)).collect() };
    assert!(titles(&parse(&u.page("help", "store@apps")).unwrap()).contains(&"Sections".to_string()));
    assert!(titles(&parse(&u.page("help", "machine@")).unwrap()).contains(&"Sections".to_string()));
    assert!(!titles(&parse(&u.page("help", "settings@")).unwrap()).contains(&"Sections".to_string()));
    done("chips");
}

#[test]
fn an_install_joins_the_line_and_its_row_says_working() {
    let mut u = ui("line", HANDHELD);
    let a = confirm(&mut u, "store", "com.spotify.Client");
    assert_eq!(a.get("queued").and_then(Json::as_bool), Some(true));
    assert!(a.str_at("toast").unwrap().starts_with("Spotify: working on it."));
    // The app takes the plan to run it; from then the bar and the row say so.
    let (id, plan) = u.queue_next().unwrap();
    assert_eq!(plan.steps.len(), 1);
    assert!(u.queue_next().is_none());
    let st = status(&u);
    assert_eq!((st.str_at("work"), st.str_at("workShort"), st.get("jobs").and_then(Json::as_f64)), (Some("Working: Spotify"), Some("Spotify"), Some(1.0)));
    let r = row(&page(&mut u, "apps"), "Spotify");
    assert_eq!(shows(&r), ("Working", "", "", "open"));
    assert_eq!(r.str_at("arg"), Some("store-item|com.spotify.Client"));
    // It shows under Installed while it is on its way.
    assert_eq!(shows(&row(&page(&mut u, "installed"), "Spotify")).0, "Working");
    // A second one waits, and says how many are ahead.
    let b = confirm(&mut u, "store", "com.obsproject.Studio");
    assert_eq!(b.str_at("toast"), Some("OBS Studio waits for its turn: 1 thing is ahead of it."));
    assert_eq!(shows(&row(&page(&mut u, "apps"), "OBS Studio")).0, "Waiting");
    assert_eq!(status(&u).str_at("work"), Some("Working: Spotify (1 more waiting)"));
    // Progress goes to the bar.
    u.queue_say(id, "Installing Spotify", Some(0.5));
    let st = status(&u);
    assert_eq!((st.str_at("detail"), st.get("part").and_then(Json::as_f64)), (Some("Installing Spotify"), Some(0.5)));
    // It ends: the note with its time, the row's new state, the next one's turn.
    let end = u.queue_done(id, Ok(())).unwrap();
    assert_eq!((end.get("ok").and_then(Json::as_bool), end.str_at("toast")), (Some(true), Some("Spotify installed (12 s)")));
    assert!(u.queue_done(id, Ok(())).is_none());
    assert_eq!(shows(&row(&page(&mut u, "apps"), "Spotify")).2, "Open");
    let (id, _) = u.queue_next().unwrap();
    assert_eq!(status(&u).str_at("work"), Some("Working: OBS Studio"));
    // A failure says what and why, and the row goes back to what it was.
    let end = u.queue_done(id, Err("Flathub didn't answer.".into())).unwrap();
    assert_eq!((end.get("ok").and_then(Json::as_bool), end.str_at("toast")), (Some(false), Some("OBS Studio didn't work: Flathub didn't answer.")));
    assert_eq!(shows(&row(&page(&mut u, "apps"), "OBS Studio")), ("Not installed", "", "Install", "store"));
    let st = status(&u);
    assert_eq!((st.str_at("work"), st.get("jobs").and_then(Json::as_f64), st.str_at("closing")), (Some(""), Some(0.0), Some("")));
    done("line");
}

#[test]
fn the_same_thing_is_never_asked_for_twice() {
    let mut u = ui("twice", HANDHELD);
    confirm(&mut u, "store", "com.spotify.Client");
    u.queue_next();
    // Refused before any dialog: installing it again, removing it, updating it.
    for action in ["store", "store-update", "store-remove"] {
        u.installed.push("com.spotify.Client".into());
        let a = act(&mut u, action, "com.spotify.Client");
        u.installed.retain(|x| x != "com.spotify.Client");
        assert!(a.get("dialog").is_none(), "{action}");
        assert!(a.str_at("toast").unwrap().starts_with("Spotify is being worked on right now."), "{action}: {a:?}");
    }
    // Update all touches every app: refused while one is in line, and the other way round.
    assert!(act(&mut u, "update-all", "").str_at("toast").unwrap().contains("being worked on"));
    let mut u2 = ui("twice-all", HANDHELD);
    confirm(&mut u2, "update-all", "");
    assert_eq!(status(&u2).get("jobs").and_then(Json::as_f64), Some(1.0));
    let a = act(&mut u2, "store", "com.spotify.Client");
    assert!(a.str_at("toast").unwrap().contains("waiting for its turn already"), "{a:?}");
    // A tile that needs the browser waits for the browser's own install to end (it runs behind the cover).
    let mut u3 = ui("twice-tile", Preset::TvPc);
    confirm(&mut u3, "store", store::CHROME);
    act(&mut u3, "tile", "netflix");
    let plan = u3.confirm().unwrap();
    assert!(u3.runs_covered(&plan));
    assert!(u3.work_refusal(&plan).unwrap().starts_with("Chrome is waiting"));
    // Turbine Hub never replaces itself while work runs.
    let mut p = super::Plan::new("Update Turbine Hub?", "", "Update");
    p.steps.push(super::Step::SelfUpdate);
    assert!(u3.runs_covered(&p));
    assert!(u3.work_refusal(&p).unwrap().starts_with("Turbine Hub updates itself once"));
    // Leaving for the other session waits for the line too.
    assert!(act(&mut u3, "session", "desktop").str_at("toast").unwrap().starts_with("Something is still being worked on."));
    done("twice");
    done("twice-all");
    done("twice-tile");
}

#[test]
fn what_runs_behind_the_cover_stays_there() {
    let mut u = ui("cover", HANDHELD);
    // System changes (the helper, and its password the first time), Steam's files, Turbine Hub in Steam.
    for (action, arg) in [("decky", ""), ("tile", "netflix"), ("tuneup", "all"), ("self-tile", "add"), ("session", "desktop")] {
        let a = act(&mut u, action, arg);
        assert!(a.get("dialog").is_some(), "{action}: {a:?}");
        let plan = u.confirm().unwrap();
        assert!(u.runs_covered(&plan), "{action} would run while the player browses");
    }
    // Installs, updates and removals from the Store don't.
    for (action, arg) in [("store", "com.spotify.Client"), ("store-update", store::HEROIC), ("store-remove", "com.discordapp.Discord"), ("update-all", "")] {
        act(&mut u, action, arg);
        let plan = u.confirm().unwrap();
        assert!(!u.runs_covered(&plan), "{action} would cover the screen");
    }
    done("cover");
}

#[test]
fn closing_stops_the_line_and_says_how_much_runs() {
    let mut u = ui("close", HANDHELD);
    u.sample_queue(2);
    let st = status(&u);
    assert_eq!(st.str_at("work"), Some("Working: Discord (2 more waiting)"));
    assert_eq!(st.str_at("closing"), Some("3 things are still running. Close anyway? The one being worked on is stopped cleanly and the rest are dropped."));
    // Closing drops what waits and hands back the running plan's number for the app to stop.
    let running = u.queue_stop();
    assert!(running.is_some());
    assert_eq!(status(&u).get("jobs").and_then(Json::as_f64), Some(1.0));
    let end = u.queue_done(running.unwrap(), Err("Stopped, because Turbine Hub was closed.".into())).unwrap();
    assert_eq!(end.get("ok").and_then(Json::as_bool), Some(false));
    assert!(u.queue_next().is_none());
    done("close");
}

#[test]
fn the_top_bar_shows_the_battery_only_where_there_is_one() {
    let u = ui("bar", HANDHELD);
    let st = status(&u);
    assert!(st.get("battery").and_then(Json::as_f64).unwrap() >= 0.0);
    assert_eq!(st.get("clock").and_then(Json::as_bool), Some(true));
    assert_eq!(st.str_at("clockText"), Some("20:41"));
    assert_eq!(st.get("updates").and_then(Json::as_f64), Some(1.0));
    let mut pc = ui("bar-pc", Preset::Desktop);
    let st = status(&pc);
    assert_eq!((st.get("battery").and_then(Json::as_f64), st.get("charging").and_then(Json::as_bool)), (Some(-1.0), Some(false)));
    // A Desktop Mode window sits under the desktop's own clock.
    pc.window = true;
    assert_eq!(status(&pc).get("clock").and_then(Json::as_bool), Some(false));
    // A newer Turbine Hub counts as an update that is ready.
    pc.update = Some("9.9".into());
    assert_eq!(status(&pc).get("updates").and_then(Json::as_f64), Some(2.0));
    done("bar");
    done("bar-pc");
}

#[test]
fn the_menu_leads_to_the_other_session_and_asks_first() {
    let mut u = ui("session", HANDHELD);
    let titles = |j: &Json| -> Vec<String> { j.get("dialog").unwrap().get("rows").unwrap().as_array().iter().filter_map(|r| r.str_at("title").map(String::from)).collect() };
    let m = act(&mut u, "menu", "home@");
    assert!(titles(&m).contains(&"Switch to Desktop".to_string()) && !titles(&m).contains(&"Return to Gaming Mode".to_string()));
    let d = act(&mut u, "session", "desktop");
    let d = d.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.str_at("confirm"), d.get("danger").and_then(Json::as_bool)), (Some("Switch to Desktop?"), Some("Switch"), Some(true)));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![super::Step::Host(["flatpak-spawn", "--host", "steamos-session-select", "plasma"].iter().map(|x| x.to_string()).collect())]);
    // In a Desktop Mode window it is the way back.
    u.window = true;
    let m = act(&mut u, "menu", "home@");
    assert!(titles(&m).contains(&"Return to Gaming Mode".to_string()) && !titles(&m).contains(&"Switch to Desktop".to_string()));
    act(&mut u, "session", "gamemode");
    assert!(matches!(&u.confirm().unwrap().steps[..], [super::Step::Host(c)] if c.last().unwrap() == "gamescope"));
    done("session");
}
