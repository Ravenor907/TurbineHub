//! NonSteamLaunchers in the Store, the launcher rows and Bring it over, on the made-up machine.
use super::plan::Step;
use super::*;
use crate::fake::Preset;
use crate::json::parse;

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-nsl-ui-{tag}-{}", std::process::id()))
}

fn ui(tag: &str, preset: Preset) -> Ui {
    Ui::sample(&dir(tag), preset, Some(Mode::Guided))
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

fn rows(j: &Json) -> Vec<Json> {
    j.get("sections").map(Json::as_array).unwrap_or(&[]).iter().flat_map(|sec| sec.get("rows").map(Json::as_array).unwrap_or(&[]).to_vec()).collect()
}

fn find(j: &Json, title: &str) -> Json {
    rows(j).into_iter().find(|r| r.str_at("title") == Some(title)).unwrap_or_else(|| panic!("no row {title}"))
}

fn dialog(j: &Json) -> &Json {
    j.get("dialog").expect("a dialog")
}

fn items(d: &Json) -> Vec<(String, String)> {
    d.get("items").map(Json::as_array).unwrap_or(&[]).iter().map(|i| (i.str_at("title").unwrap_or("").to_string(), i.str_at("line").unwrap_or("").to_string())).collect()
}

#[test]
fn not_installed_the_page_offers_install_and_game_mode_explains_desktop_mode() {
    let mut u = ui("new", HANDHELD);
    let j = parse(&u.page("store-item", "nsl")).unwrap();
    assert_eq!((j.str_at("crumb"), j.str_at("title")), (Some("Store"), Some("NonSteamLaunchers")));
    assert!(j.str_at("intro").unwrap().contains("Battle.net"));
    let install = find(&j, "Install");
    assert_eq!((install.str_at("act"), install.str_at("arg"), install.str_at("button")), (Some("nsl"), Some("install"), Some("How")));
    assert_eq!(find(&j, "Not installed").str_at("line"), Some("Nothing of it is on this machine."));
    assert_eq!(find(&j, "Newest").str_at("line"), Some("Version v4.2.94"));
    assert!(find(&j, "Source").str_at("line").unwrap().contains("github.com/moraroy/NonSteamLaunchers-On-Steam-Deck"));
    // Decky Loader is there, its plugin isn't: offered.
    assert_eq!(find(&j, "Get its Decky plugin").str_at("act"), Some("nsl-plugin"));
    // Game Mode: only how to get to Desktop Mode, nothing waits to run.
    let g = parse(&u.act("nsl", "install", "")).unwrap();
    let d = dialog(&g);
    assert_eq!((d.str_at("title"), d.str_at("confirm")), (Some("Install NonSteamLaunchers in Desktop Mode"), Some("")));
    let steps = items(d);
    assert!(steps[0].1.contains("Switch to Desktop"));
    assert!(steps.iter().any(|(t, l)| t == "Or" && l.contains("Decky plugin")));
    assert!(u.confirm().is_none());
    let g = parse(&u.act("nsl-plugin", "", "")).unwrap();
    assert_eq!(dialog(&g).str_at("title"), Some("Get its Decky plugin in Desktop Mode"));
    assert!(u.confirm().is_none());
    done("new");
}

#[test]
fn in_desktop_mode_install_fetches_the_release_script_and_starts_it() {
    let mut u = ui("plan", Preset::Desktop);
    u.window = true;
    let j = parse(&u.page("store-item", "nsl")).unwrap();
    assert_eq!(find(&j, "Install").str_at("button"), Some("Install"));
    let d = parse(&u.act("nsl", "install", "")).unwrap();
    let d = dialog(&d);
    assert_eq!((d.str_at("title"), d.str_at("confirm"), d.str_at("cancel")), (Some("Install NonSteamLaunchers?"), Some("Install"), Some("Cancel")));
    let it = items(d);
    assert_eq!(it[0], ("Get its script".to_string(), "NonSteamLaunchers.sh, release v4.2.94, from github.com/moraroy/NonSteamLaunchers-On-Steam-Deck".to_string()));
    assert!(it.iter().any(|(t, l)| t == "Start it" && l.contains("no password")));
    assert!(it.iter().any(|(t, _)| t == "Steam restarts"));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![Step::Nsl { tag: "v4.2.94".into(), plugin: false }]);
    // Never the helper, never a password.
    assert_eq!(plan.ask, super::plan::Ask::None);
    // The plugin: its own installer at the same release, which asks for the password in its own window.
    let d = parse(&u.act("nsl-plugin", "", "")).unwrap();
    assert!(items(dialog(&d)).iter().any(|(t, l)| t == "Its window asks" && l.contains("SteamOS password")));
    assert_eq!(u.confirm().unwrap().steps, vec![Step::Nsl { tag: "v4.2.94".into(), plugin: true }]);
    // No release known (never online): nothing is asked, it says why.
    u.nsl_tag = None;
    assert!(parse(&u.act("nsl", "install", "")).unwrap().str_at("toast").unwrap().starts_with("Couldn't reach GitHub"));
    assert!(u.confirm().is_none());
    // A tag that isn't one never reaches an address.
    u.nsl_tag = Some("main".into());
    assert!(parse(&u.act("nsl", "install", "")).unwrap().get("toast").is_some());
    done("plan");
}

#[test]
fn installed_it_shows_its_version_launchers_open_and_update() {
    let mut u = ui("have", Preset::Desktop);
    u.window = true;
    u.sample_with_nsl();
    let j = parse(&u.page("store-item", "nsl")).unwrap();
    assert_eq!(find(&j, "Installed").str_at("line"), Some("Version v4.2.94"));
    assert_eq!(find(&j, "In your library").str_at("line"), Some("Battle.net, EA app"));
    assert_eq!(find(&j, "Open").str_at("button"), Some("Open"));
    assert!(rows(&j).iter().all(|r| r.str_at("title") != Some("Update")));
    parse(&u.act("nsl", "open", "")).unwrap();
    let plan = u.confirm().unwrap();
    assert_eq!((plan.title.as_str(), plan.confirm.as_str()), ("Open NonSteamLaunchers?", "Open"));
    // A newer release: Update, and the Store's row says so.
    u.nsl_tag = Some("v4.2.100".into());
    let j = parse(&u.page("store-item", "nsl")).unwrap();
    assert_eq!(find(&j, "Update").str_at("line"), Some("Version v4.2.100 is ready"));
    let row = find(&parse(&u.page("store", "launchers")).unwrap(), "NonSteamLaunchers");
    assert_eq!((row.str_at("badge"), row.str_at("arg")), (Some("Update ready"), Some("store-item|nsl")));
    parse(&u.act("nsl", "update", "")).unwrap();
    assert_eq!(u.confirm().unwrap().steps, vec![Step::Nsl { tag: "v4.2.100".into(), plugin: false }]);
    // Set up some other way: installed, version not known, and no Update guessed.
    let _ = std::fs::remove_file(u.folder().join("nsl-version"));
    let j = parse(&u.page("store-item", "nsl")).unwrap();
    assert!(find(&j, "Installed").str_at("line").unwrap().contains("isn't known"));
    assert!(rows(&j).iter().all(|r| r.str_at("title") != Some("Update")));
    // Its Decky plugin, once there, is shown instead of offered.
    let p = u.ctx.home.join("homebrew/plugins/NonSteamLaunchers/package.json");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, r#"{"name":"nonsteamlaunchers","version":"1.3.90"}"#).unwrap();
    let j = parse(&u.page("store-item", "nsl")).unwrap();
    assert_eq!(find(&j, "Decky plugin").str_at("badge"), Some("Installed"));
    assert_eq!(parse(&u.act("nsl-plugin", "", "")).unwrap().str_at("toast"), Some("Its Decky plugin is installed already."));
    done("have");
}

#[test]
fn launcher_rows_lead_to_its_page_and_say_what_to_tick() {
    let mut u = ui("rows", HANDHELD);
    let j = parse(&u.page("store", "launchers")).unwrap();
    for name in ["NonSteamLaunchers", "Battle.net", "EA app", "Ubisoft Connect", "Rockstar Games Launcher", "Epic Games Launcher", "GOG Galaxy", "Amazon Games", "itch.io", "Legacy Games", "Humble Games"] {
        assert_eq!(find(&j, name).str_at("arg"), Some("store-item|nsl"), "{name}");
    }
    assert_eq!(find(&j, "Battle.net").str_at("line"), Some("Installs through NonSteamLaunchers"));
    // Heroic is installed on the made-up machine: it is named first for Epic, GOG and Amazon.
    assert!(find(&j, "Epic Games Launcher").str_at("line").unwrap().starts_with("Heroic, which you have, is the easier way"));
    u.sample_with_nsl();
    let j = parse(&u.page("store", "launchers")).unwrap();
    assert_eq!((find(&j, "Battle.net").str_at("badge"), find(&j, "Battle.net").str_at("line")), (Some("Installed"), Some("In your library, through NonSteamLaunchers")));
    assert_eq!(find(&j, "Ubisoft Connect").str_at("line"), Some("Open NonSteamLaunchers and tick Ubisoft Connect"));
    assert_eq!(find(&j, "Humble Games").str_at("line"), Some("Open NonSteamLaunchers and tick Humble Games Collection"));
    assert_eq!(find(&j, "NonSteamLaunchers").str_at("badge"), Some("Installed"));
    // Installed shows it too.
    assert!(rows(&parse(&u.page("store", "installed")).unwrap()).iter().any(|r| r.str_at("title") == Some("NonSteamLaunchers")));
    // Search finds it by any of its names.
    for q in ["nonsteam", "nsl", "launchers", "NonSteamLaunchers"] {
        u.act("search", "store", q);
        let found = rows(&parse(&u.page("store", "")).unwrap());
        assert!(found.iter().any(|r| r.str_at("title") == Some("NonSteamLaunchers")), "{q}");
    }
    done("rows");
}

#[test]
fn bring_it_over_sends_windows_launchers_to_its_page() {
    let mut u = ui("bring", HANDHELD);
    let j = parse(&u.page("winapps", "")).unwrap();
    for name in ["Battle.net", "EA app", "Ubisoft Connect", "Rockstar Games Launcher"] {
        let r = find(&j, name);
        assert_eq!((r.str_at("act"), r.str_at("arg")), (Some("open"), Some("store-item|nsl")), "{name}");
    }
    // The made-up Windows drive has Battle.net.
    assert_eq!(find(&j, "Battle.net").str_at("badge"), Some("On your Windows drive"));
    assert!(matches!(crate::bring::APPS.iter().find(|a| a.id == "ea").unwrap().becomes, crate::bring::Becomes::Nsl("ea")));
    u.sample_with_nsl();
    let j = parse(&u.page("winapps", "")).unwrap();
    assert_eq!(find(&j, "Battle.net").str_at("badge"), Some("Installed"));
    assert!(find(&j, "Ubisoft Connect").str_at("line").unwrap().ends_with("Open NonSteamLaunchers and tick Ubisoft Connect"));
    done("bring");
}
