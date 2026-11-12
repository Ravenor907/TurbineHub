//! Store: what Turbine Hub offers to install, by section, with each thing's state on this machine, a search over
//! all of it, and a page per thing (what it is, where it comes from, its size and version, install, update,
//! remove). Apps come from Flathub through the host's flatpak, Decky Loader through the helper, NonSteamLaunchers
//! from its own script (ui/nsl.rs), and TV and cloud services become library tiles.
use super::games::{matches, query, search_box};
use super::plan::{Plan, Step, dialog, guide, helper_plan, toast, with_helper};
use super::{Ui, arr, b, n, obj, row, s, section, with};
use crate::helper::{self, Setting};
use crate::json::{self, Json, quote};
use std::collections::BTreeMap;
use std::path::Path;

pub const LUDUSAVI: &str = crate::saves::LUDUSAVI;
pub const TURBINE: &str = "io.github.ravenor907.TurbineModManager";
pub const CHROME: &str = "com.google.Chrome";
pub const HEROIC: &str = "com.heroicgameslauncher.hgl";
const DECKY_SERVICE: &str = "plugin_loader.service";

pub struct App {
    pub id: &'static str,
    pub name: &'static str,
    pub line: &'static str,
    pub section: &'static str,
    /// What it is, in a sentence or two, for its own page.
    pub about: &'static str,
}

const fn app(section: &'static str, id: &'static str, name: &'static str, line: &'static str, about: &'static str) -> App {
    App { id, name, line, section, about }
}

/// Everything from Flathub the Store lists, in the order shown.
pub const APPS: &[App] = &[
    app("essentials", LUDUSAVI, "Ludusavi", "Save backups for every game", "Knows where thousands of games keep their saves and copies them to a safe place. Turbine Hub uses it for snapshots, restores and the saves on a Windows drive."),
    app("essentials", "com.vysp3r.ProtonPlus", "ProtonPlus", "Extra Proton versions", "Installs other versions of Proton, such as GE-Proton, that fix games Steam's own Proton has trouble with. They show up in Games, Try another Proton."),
    app("essentials", "io.github.radiolamp.mangojuice", "MangoJuice", "Set up the FPS and temps overlay", "Sets up MangoHud, the overlay that shows frame rate, temperatures and load while a game runs."),
    app("essentials", "com.github.tchx84.Flatseal", "Flatseal", "What each app may reach", "Shows and changes which folders and devices each installed app may reach."),
    app("apps", "com.discordapp.Discord", "Discord", "Voice chat and friends", "Voice, video and text chat. The same app and account as on Windows."),
    app("apps", "com.moonlight_stream.Moonlight", "Moonlight", "Stream games from your PC", "Plays games from another PC in the house on this machine, over the home network."),
    app("apps", "com.obsproject.Studio", "OBS Studio", "Record and stream", "Records the screen and streams it live. The same app as on Windows."),
    app("apps", "com.spotify.Client", "Spotify", "Music while you play", "Music and podcasts. Keeps playing in the background while a game runs."),
    app("apps", CHROME, "Chrome", "The browser the library tiles use", "Google's browser. The TV, streaming and cloud gaming tiles open in it, full screen, with a controller."),
    app("apps", "org.mozilla.firefox", "Firefox", "A browser for Desktop Mode", "Mozilla's browser, for Desktop Mode with a mouse and keyboard."),
    app("apps", "org.videolan.VLC", "VLC", "Plays any video file", "Plays nearly every video and music file without anything extra."),
    app("launchers", HEROIC, "Heroic Games Launcher", "Epic, GOG and Amazon games", "Installs and starts your games from Epic, GOG and Amazon, and adds them to your Steam library."),
    app("launchers", "net.lutris.Lutris", "Lutris", "Other launchers and older games", "Installs games from other launchers and older Windows games with ready-made recipes."),
    app("launchers", "com.usebottles.bottles", "Bottles", "Windows programs that aren't games", "Runs Windows programs that aren't games, each in its own set of Windows files."),
    app("emulation", "net.retrodeck.retrodeck", "RetroDECK", "Classic systems in one app", "Emulators for classic systems in one app, set up for a controller. Bring your own games."),
];

pub struct Tile {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub line: &'static str,
}

const DECKY_LINE: &str = "Plugins for Steam's quick menu, from its official release";

pub const PANEL_TITLE: &str = "Turbine Hub quick panel";
/// The quick panel's name in its package.json, whatever its folder among Decky's plugins is called.
pub const PANEL_PACKAGE: &str = "turbinehub-decky";
/// Where Decky Loader gets the quick panel: the zip every release carries.
pub const PANEL_URL: &str = "https://github.com/Ravenor907/TurbineHub/releases/latest/download/TurbineHub-Decky.zip";

/// Is Turbine Hub's quick panel among Decky's plugins? Read only: each plugin's package.json names it.
pub fn panel_installed(ui: &Ui) -> bool {
    std::fs::read_dir(ui.ctx.home.join("homebrew/plugins")).is_ok_and(|d| d.flatten().any(|e| std::fs::read_to_string(e.path().join("package.json")).ok().and_then(|t| json::parse(&t).ok()).is_some_and(|j| j.str_at("name") == Some(PANEL_PACKAGE))))
}

fn panel_row(ui: &Ui) -> Json {
    if panel_installed(ui) {
        with(with(row("none", "", "info", PANEL_TITLE, "Readings and this game's switches in Steam's quick menu"), "badge", s("Installed")), "tone", s("ok"))
    } else {
        let line = if decky_installed(ui) { "Readings and this game's switches in Steam's quick menu. Not installed yet: a few steps in Decky Loader's settings." } else { "Readings and this game's switches in Steam's quick menu. Not installed yet: needs Decky Loader first." };
        with(row("panel-guide", "", "button", PANEL_TITLE, line), "button", s("How"))
    }
}

/// Services that become a library tile opening the browser full screen.
pub const TILES: &[Tile] = &[
    Tile { id: "xcloud", name: "Xbox Cloud", url: "https://www.xbox.com/play", line: "Game Pass games, streamed" },
    Tile { id: "geforcenow", name: "GeForce NOW", url: "https://play.geforcenow.com", line: "Your PC games, streamed" },
    Tile { id: "netflix", name: "Netflix", url: "https://www.netflix.com", line: "Films and series" },
    Tile { id: "youtube", name: "YouTube", url: "https://www.youtube.com/tv", line: "The TV layout, made for a controller" },
    Tile { id: "disney", name: "Disney+", url: "https://www.disneyplus.com", line: "Films and series" },
    Tile { id: "twitch", name: "Twitch", url: "https://www.twitch.tv", line: "Live streams" },
    Tile { id: "plex", name: "Plex", url: "https://app.plex.tv", line: "Your own films and series" },
];

const SECTIONS: &[(&str, &str, &str)] = &[
    ("essentials", "Essentials", "The few things nearly everyone wants."),
    ("apps", "Apps", "From Flathub, always the newest version. Remove any of them later in Installed."),
    ("decky", "Decky plugins", "Decky Loader adds plugins to Steam's quick menu."),
    ("launchers", "Launchers", "Games from other stores, in your Steam library."),
    ("tv", "TV and streaming", "Each one becomes a tile in your library that opens full screen."),
    ("emulation", "Emulation", "Classic systems. Bring your own games."),
    ("installed", "Installed", "What Turbine Hub's Store put on this machine."),
];

pub(crate) fn tile(id: &str) -> Option<&'static Tile> {
    TILES.iter().find(|t| t.id == id)
}

/// The tile's entry in the Steam library: the browser in full-screen mode on one address.
pub fn tile_shortcut(t_name: &str, url: &str) -> crate::shortcuts::NewShortcut {
    crate::shortcuts::NewShortcut {
        name: t_name.into(),
        exe: "\"/usr/bin/flatpak\"".into(),
        start_dir: "\"/usr/bin/\"".into(),
        launch_options: format!("run --branch=stable --arch=x86_64 --command=/app/bin/chrome --file-forwarding {CHROME} @@u @@ --window-size=1280,800 --force-device-scale-factor=1.25 --device-scale-factor=1.25 --kiosk \"{url}\""),
        flatpak_id: CHROME.into(),
        icon: String::new(),
    }
}

/// The words in a tile's launch options that mark it as Turbine Hub's.
pub fn tile_marker(url: &str) -> String {
    format!("--kiosk \"{url}\"")
}

pub(crate) fn tile_added(ui: &Ui, t: &Tile) -> bool {
    let marker = tile_marker(t.url);
    crate::shortcuts::shortcut_files(&ui.ctx.home).iter().any(|f| std::fs::read(f).ok().and_then(|bytes| crate::shortcuts::parse(&bytes).ok()).is_some_and(|list| list.iter().any(|x| x.launch_options.contains(&marker))))
}

/// The apps Turbine Hub's Store installed itself: one id per line in Turbine Hub's folder. Review changes lists the
/// ones still there.
fn made_file(folder: &Path) -> std::path::PathBuf {
    folder.join("store-installs")
}

pub fn made_by_hub(folder: &Path) -> Vec<String> {
    std::fs::read_to_string(made_file(folder)).unwrap_or_default().lines().map(str::trim).filter(|l| APPS.iter().any(|a| a.id == *l)).map(String::from).collect()
}

/// Notes an install (`add`) or a removal of one of the Store's apps.
pub fn note_made(folder: &Path, id: &str, add: bool) {
    let mut list = made_by_hub(folder);
    list.retain(|x| x != id);
    if add {
        list.push(id.to_string());
    }
    let text: String = list.iter().map(|x| format!("{x}\n")).collect();
    let _ = std::fs::create_dir_all(folder);
    let _ = crate::write_atomic(&made_file(folder), text.as_bytes());
}

pub(crate) fn is_installed(ui: &Ui, id: &str) -> bool {
    ui.installed.iter().any(|a| a == id)
}

fn flatpak(args: &[&str]) -> Vec<String> {
    ["flatpak-spawn", "--host", "flatpak"].iter().chain(args).map(|x| x.to_string()).collect()
}

pub(crate) fn install_step(id: &str) -> Step {
    Step::Host(flatpak(&["install", "-y", "--noninteractive", "flathub", id]))
}

/// The steps that add one tile: the browser first when it's missing, its access to controllers, then the tile.
pub(crate) fn tile_steps(ui: &Ui, t: &Tile) -> Vec<Step> {
    let mut steps = Vec::new();
    if !is_installed(ui, CHROME) {
        steps.push(install_step(CHROME));
    }
    steps.push(Step::Host(flatpak(&["override", "--user", "--filesystem=/run/udev:ro", CHROME])));
    steps.push(Step::Tile { name: t.name.into(), url: t.url.into() });
    steps
}

/// An app's row in a list: its state as a badge. A press opens the app's own page.
fn app_row(ui: &Ui, a: &App) -> Json {
    let (badge, tone) = match (is_installed(ui, a.id), ui.updates.iter().any(|u| u == a.id)) {
        (true, true) => ("Update ready", "warn"),
        (true, false) => ("Installed", "ok"),
        _ => ("", ""),
    };
    with(with(row("open", &format!("store-item|{}", a.id), "button", a.name, a.line), "badge", s(badge)), "tone", s(tone))
}

fn tile_row(ui: &Ui, t: &Tile) -> Json {
    let added = tile_added(ui, t);
    with(with(row("open", &format!("store-item|tile:{}", t.id), "button", t.name, t.line), "badge", s(if added { "In your library" } else { "" })), "tone", s("ok"))
}

fn decky_row(ui: &Ui) -> Json {
    let has = decky_installed(ui);
    with(with(row("open", "store-item|decky", "button", "Decky Loader", DECKY_LINE), "badge", s(if has { "Installed" } else { "" })), "tone", s("ok"))
}

pub(crate) fn decky_installed(ui: &Ui) -> bool {
    ui.ctx.home.join("homebrew/services/PluginLoader").exists()
}

fn plugins(ui: &Ui) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(ui.ctx.home.join("homebrew/plugins"))
        .map(|d| d.flatten().filter(|e| e.path().is_dir()).map(|e| std::fs::read_to_string(e.path().join("plugin.json")).ok().and_then(|t| json::parse(&t).ok()).and_then(|j| j.str_at("name").map(String::from)).unwrap_or_else(|| e.file_name().to_string_lossy().into_owned())).collect())
        .unwrap_or_default();
    v.sort_by_key(|x| x.to_lowercase());
    v
}

fn plugins_off(ui: &Ui) -> bool {
    ui.ledger().settings().iter().any(|x| matches!(x, Setting::Service { unit, enabled: false } if unit == DECKY_SERVICE))
}

fn rows(ui: &Ui, sec: &str) -> Vec<Json> {
    let mut out: Vec<Json> = APPS.iter().filter(|a| a.section == sec).map(|a| app_row(ui, a)).collect();
    match sec {
        "decky" => {
            out.push(decky_row(ui));
            out.push(panel_row(ui));
            if decky_installed(ui) {
                out.push(with(row("decky-off", "", "toggle", "Plugins on", "Turn every plugin off to see if one of them causes a problem"), "on", b(!plugins_off(ui))));
                out.extend(plugins(ui).iter().map(|p| row("none", "", "info", p, "Plugin")));
            }
        }
        "launchers" => {
            // NonSteamLaunchers and the Windows launchers it installs, each leading to its page (ui/nsl.rs).
            out.push(super::nsl::store_row(ui));
            out.extend(super::nsl::launcher_rows(ui));
        }
        "tv" => out.extend(TILES.iter().map(|t| tile_row(ui, t))),
        "emulation" => out.push(row("none", "", "info", "EmuDeck", "Installs from emudeck.com in Desktop Mode. RetroDECK above needs no setup outside the app.")),
        "installed" => {
            out = APPS.iter().filter(|a| is_installed(ui, a.id)).map(|a| app_row(ui, a)).collect();
            if decky_installed(ui) {
                out.push(decky_row(ui));
            }
            out.extend(TILES.iter().filter(|t| tile_added(ui, t)).map(|t| tile_row(ui, t)));
            if super::nsl::state(ui).installed {
                out.push(super::nsl::store_row(ui));
            }
        }
        _ => {}
    }
    out
}

/// Everything in the Store that matches what the player typed, whatever its section: apps, Decky Loader, the
/// Windows launchers and the tiles. Names are looked at first, then the words under them.
fn found(ui: &Ui, q: &str) -> Vec<Json> {
    let mut by_name = Vec::new();
    let mut by_words = Vec::new();
    let mut put = |name: &str, words: &str, r: Json| {
        if matches(q, name) {
            by_name.push(r);
        } else if matches(q, &format!("{name} {words}")) {
            by_words.push(r);
        }
    };
    for a in APPS {
        put(a.name, &format!("{} {}", a.line, a.about), app_row(ui, a));
    }
    put("Decky Loader", &format!("{DECKY_LINE} plugins"), decky_row(ui));
    put(PANEL_TITLE, "Decky plugin quick menu readings switches", panel_row(ui));
    put(super::nsl::NAME, &format!("{} {}", super::nsl::LINE, super::nsl::WORDS), super::nsl::store_row(ui));
    let nsl = super::nsl::state(ui);
    for x in crate::nsl::LAUNCHERS {
        put(x.name, &format!("{} launcher NonSteamLaunchers nsl", x.tick), super::nsl::launcher_row(ui, &nsl, x));
    }
    for t in TILES {
        put(t.name, &format!("{} tile streaming", t.line), tile_row(ui, t));
    }
    by_name.append(&mut by_words);
    by_name
}

/// The Store tab. `arg` is the section. While the search box holds something, the matches from every section show
/// instead.
pub(crate) fn page(ui: &mut Ui, arg: &str) -> Json {
    let index = SECTIONS.iter().position(|(id, _, _)| *id == arg).unwrap_or(0);
    let (id, title, line) = SECTIONS[index];
    let q = query(ui, "store");
    let searching = !q.trim().is_empty();
    let list = if searching { found(ui, &q) } else { rows(ui, id) };
    let empty = match (list.is_empty(), searching) {
        (false, _) => String::new(),
        (true, true) => format!("Nothing in the Store matches \"{}\".", q.trim()),
        (true, false) => "Nothing installed from the Store yet.".into(),
    };
    let updates = APPS.iter().filter(|a| ui.updates.iter().any(|u| u == a.id)).count();
    let (title, line) = if searching { ("Search", match list.len() { 1 => format!("1 match for \"{}\"", q.trim()), c => format!("{c} matches for \"{}\"", q.trim()) }) } else { (title, line.to_string()) };
    obj! {
        // While searching, the sections step aside: the matches come from all of them.
        "side": arr(SECTIONS.iter().filter(|_| !searching).map(|(id, title, _)| obj! {"id": s(*id), "title": s(*title), "chip": s(""), "tone": s(""), "lock": b(false)})),
        "sideTitle": s(if searching { "Every section" } else { "" }),
        "index": n(index as f64),
        "search": search_box(ui, "store", "Search the Store"),
        "header": obj! {"title": s(title), "chips": arr(if ui.offline { vec![obj! {"text": s("Offline now. This is what was found last; installing needs the internet."), "tone": s("warn")}] } else { vec![] }), "line": s(line)},
        "sections": arr([with(with(section("", list), "columns", n(2.0)), "empty", s(empty))]),
        "options": arr(if updates > 0 { vec![super::controls::page_button("Update all", "update-all", "")] } else { vec![] }),
    }
}

// ---- Versions and sizes, as the host's flatpak last told them ----

/// What is known about one app: the version installed and its size on the drive, the newest version on Flathub
/// and what a fresh install downloads and takes. "" for what isn't known.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppFacts {
    pub version: String,
    pub size: String,
    pub newest: String,
    pub download: String,
    pub full: String,
}

/// The columns asked of `flatpak list --app` and of `flatpak remote-ls flathub --app`, in the order they are read.
pub const INSTALLED_COLUMNS: &str = "--columns=application,version,size";
pub const REMOTE_COLUMNS: &str = "--columns=application,version,download-size,installed-size";

fn facts_file(cache: &Path) -> std::path::PathBuf {
    cache.join("store.json")
}

/// The facts kept from the last time the host answered, with when the Flathub part was fetched.
pub fn app_facts(cache: &Path) -> (Option<u64>, BTreeMap<String, AppFacts>) {
    let Some(j) = std::fs::read_to_string(facts_file(cache)).ok().and_then(|t| json::parse(&t).ok()) else { return (None, BTreeMap::new()) };
    let text = |v: &Json, k: &str| v.str_at(k).unwrap_or("").to_string();
    let apps = j.get("apps").map(Json::entries).unwrap_or(&[]).iter().map(|(id, v)| (id.clone(), AppFacts { version: text(v, "version"), size: text(v, "size"), newest: text(v, "newest"), download: text(v, "download"), full: text(v, "full") })).collect();
    (j.get("at").and_then(Json::as_f64).map(|x| x as u64), apps)
}

fn save_facts(cache: &Path, at: Option<u64>, apps: &BTreeMap<String, AppFacts>) {
    let body: Vec<String> = apps.iter().map(|(id, f)| format!("{}:{{\"version\":{},\"size\":{},\"newest\":{},\"download\":{},\"full\":{}}}", quote(id), quote(&f.version), quote(&f.size), quote(&f.newest), quote(&f.download), quote(&f.full))).collect();
    let text = format!("{{\"at\":{},\"apps\":{{{}}}}}\n", at.map(|x| x.to_string()).unwrap_or_else(|| "null".into()), body.join(","));
    let _ = crate::write_atomic(&facts_file(cache), text.as_bytes());
}

/// The lines of a flatpak listing as their columns, for the Store's own apps only. Sizes come with a space that
/// doesn't break ("85.1 MB"); it becomes a plain one.
fn columns(text: &str) -> Vec<Vec<String>> {
    text.lines().map(|l| l.split('\t').map(|c| c.replace(['\u{a0}', '\u{202f}'], " ").trim().to_string()).collect::<Vec<_>>()).filter(|c| APPS.iter().any(|a| Some(a.id) == c.first().map(String::as_str))).collect()
}

/// Keeps what `flatpak list --app` with `INSTALLED_COLUMNS` said: each installed app's version and size. Apps it
/// doesn't list aren't installed any more.
pub fn remember_installed(cache: &Path, text: &str) {
    let (at, mut apps) = app_facts(cache);
    for f in apps.values_mut() {
        f.version.clear();
        f.size.clear();
    }
    for c in columns(text) {
        let f = apps.entry(c[0].clone()).or_default();
        f.version = c.get(1).cloned().unwrap_or_default();
        f.size = c.get(2).cloned().unwrap_or_default();
    }
    save_facts(cache, at, &apps);
}

/// Keeps what `flatpak remote-ls flathub --app` with `REMOTE_COLUMNS` said: the newest version and the sizes. An
/// empty answer (no internet) changes nothing, so the last good one stays with its age.
pub fn remember_remote(cache: &Path, text: &str, now: u64) {
    let list = columns(text);
    if list.is_empty() {
        return;
    }
    let (_, mut apps) = app_facts(cache);
    for c in list {
        let f = apps.entry(c[0].clone()).or_default();
        f.newest = c.get(1).cloned().unwrap_or_default();
        f.download = c.get(2).cloned().unwrap_or_default();
        f.full = c.get(3).cloned().unwrap_or_default();
    }
    save_facts(cache, Some(now), &apps);
}

fn info(title: &str, line: &str) -> Json {
    row("none", "", "info", title, line)
}

fn missing_item() -> Json {
    obj! {"crumb": s("Store"), "title": s("Not found"), "intro": s(""), "sections": arr([with(section("", vec![]), "empty", s("The Store has nothing by that name."))])}
}

/// One thing's own page: what it is, where it comes from, its size and version when known, and install, update or
/// remove. `arg` is an app's id, "decky", "nsl" (NonSteamLaunchers, in ui/nsl.rs) or "tile:<id>".
pub(crate) fn item(ui: &mut Ui, arg: &str) -> Json {
    if arg == "nsl" {
        return super::nsl::item(ui);
    }
    if arg == "decky" {
        let has = decky_installed(ui);
        let action = with(with(row("decky", "", "button", if has { "Repair or update" } else { "Install" }, if has { "Installs the newest release again, over the one that is here" } else if super::plan::asks_password(ui) { "A system change, so its dialog asks for your SteamOS password" } else { "The official release, checked before it is installed" }), "icon", s(if has { "refresh" } else { "download" })), "button", s(if has { "Repair" } else { "Install" }));
        let newest = match (&ui.decky_tag, ui.offline) {
            (Some(tag), false) => format!("Version {tag}"),
            (Some(tag), true) => format!("Version {tag}, as found last. Offline now."),
            (None, _) => "Not known yet. It shows once the internet is there.".to_string(),
        };
        let state = with(with(info(if has { "Installed" } else { "Not installed" }, if has { "Its plug icon is in Steam's quick menu." } else { "Nothing of it is on this machine." }), "badge", s(if has { "Installed" } else { "" })), "tone", s("ok"));
        return obj! {
            "crumb": s("Store"),
            "title": s("Decky Loader"),
            "intro": s("Adds plugins to Steam's quick menu: themes, extra controls, frame generation and more."),
            "sections": arr([section("", vec![action]), section("About", vec![state, info("Newest", &newest), info("Source", "Its official release on github.com/SteamDeckHomebrew/decky-loader, checked against the checksum GitHub publishes"), info("Remove", "Decky Loader's own settings have Uninstall. Plugins on, in Store, Decky plugins, turns it off without removing anything.")])]),
            };
    }
    if let Some(t) = arg.strip_prefix("tile:").and_then(tile) {
        let added = tile_added(ui, t);
        let action = if added { with(with(row("tile-remove", t.id, "button", "Remove from your library", "The tile goes. Chrome and your sign-in stay."), "icon", s("remove")), "button", s("Remove")) } else { with(with(row("tile", t.id, "button", "Add to your library", "Steam shows it after its next start"), "icon", s("download")), "button", s("Add tile")) };
        let state = with(with(info(if added { "In your library" } else { "Not added yet" }, if added { "Under Non-Steam in your Steam library." } else { "Adding it changes nothing else." }), "badge", s(if added { "In your library" } else { "" })), "tone", s("ok"));
        let opens = if is_installed(ui, CHROME) { "Chrome, full screen, with a controller" } else { "Chrome, full screen, with a controller. Chrome is installed first." };
        return obj! {
            "crumb": s("Store"),
            "title": s(t.name),
            "intro": s(format!("{}. It becomes a tile in your Steam library.", t.line)),
            "sections": arr([section("", vec![action]), section("About", vec![state, info("Opens in", opens), info("Source", t.url), info("Size", "A tile takes no room. Chrome is about 400 MB when it isn't installed yet.")])]),
            };
    }
    let Some(a) = APPS.iter().find(|a| a.id == arg) else { return missing_item() };
    let installed = is_installed(ui, a.id);
    let update = installed && ui.updates.iter().any(|u| u == a.id);
    let (at, all) = app_facts(&ui.cache);
    let f = all.get(a.id).cloned().unwrap_or_default();
    let mut actions = Vec::new();
    if !installed {
        actions.push(with(with(row("store", a.id, "button", "Install", "From Flathub. No password needed."), "icon", s("download")), "button", s("Install")));
    }
    if update {
        actions.push(with(with(with(with(row("store-update", a.id, "button", "Update", &if f.newest.is_empty() { "A newer version is ready".to_string() } else { format!("Version {} is ready", f.newest) }), "icon", s("refresh")), "button", s("Update")), "badge", s("Update ready")), "tone", s("warn")));
    }
    // An update ready comes first; then the app itself.
    if installed {
        actions.push(with(with(row("store-open", a.id, "button", &format!("Open {}", a.name), "It opens on top of Turbine Hub"), "icon", s("external")), "button", s("Open")));
    }
    // Remove sits under what the page says about the app, never where the highlight starts.
    let remove = if installed { vec![with(with(row("store-remove", a.id, "button", "Remove", "The app goes. Its own settings and files stay."), "icon", s("remove")), "button", s("Remove"))] } else { vec![] };
    let state = match (installed, f.version.is_empty()) {
        (true, false) => with(with(info("Installed", &format!("Version {}", f.version)), "badge", s("Installed")), "tone", s("ok")),
        (true, true) => with(with(info("Installed", "Keeps itself up to date"), "badge", s("Installed")), "tone", s("ok")),
        (false, _) if !f.newest.is_empty() => info("Not installed", &format!("Newest version {}", f.newest)),
        (false, _) => info("Not installed", "Installing gets the newest version"),
    };
    let mut about = vec![state];
    let size = match (installed, f.size.is_empty(), f.download.is_empty(), f.full.is_empty()) {
        (true, false, _, _) => format!("{} on the drive", f.size),
        (_, _, false, false) => format!("{} to download, {} on the drive", f.download, f.full),
        (_, _, false, true) => format!("{} to download", f.download),
        _ => String::new(),
    };
    if !size.is_empty() {
        about.push(info("Size", &size));
    }
    about.push(info("Source", &format!("Flathub \u{b7} {}", a.id)));
    about.push(info("Updates", "Update all in the Store brings every app to its newest version."));
    let fresh = match at {
        Some(t) => format!("Sizes and versions: {}", super::checked_words(crate::now(), Some(t), ui.offline).to_lowercase()),
        None if ui.offline => "Offline now. Sizes and versions show once the internet is there.".into(),
        None => String::new(),
    };
    obj! {
        "crumb": s("Store"),
        "title": s(a.name),
        "intro": s(a.about),
        "sections": arr([section("", actions), section("About", about), section(if remove.is_empty() { "" } else { "Remove" }, remove)]),
        "note": s(fresh),
    }
}

/// The command that starts an installed app on the host.
pub fn open_line(id: &str) -> Vec<String> {
    flatpak(&["run", id])
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    match action {
        // Starting an app changes nothing, so it needs no dialog: the app runs the line at once.
        "store-open" => {
            let a = APPS.iter().find(|a| a.id == arg)?;
            if !is_installed(ui, a.id) {
                return Some(toast(&format!("{} isn't installed.", a.name)));
            }
            Some(obj! {"toast": s(format!("Opening {}", a.name)), "run": super::arr(open_line(a.id).into_iter().map(s))})
        }
        // "store" does what the app's state calls for; an app's own page asks for one thing by name.
        "store" | "store-update" | "store-remove" => {
            let a = APPS.iter().find(|a| a.id == arg)?;
            let installed = is_installed(ui, a.id);
            let update = match action {
                "store-update" => true,
                "store-remove" => false,
                _ => ui.updates.iter().any(|u| u == a.id),
            };
            if !installed && action != "store" {
                return Some(toast(&format!("{} isn't installed.", a.name)));
            }
            let mut p = match (installed, update) {
                (false, _) => {
                    let mut p = Plan::new(&format!("Install {}?", a.name), &format!("{}. It comes from Flathub and keeps itself up to date. No password needed.", a.line), "Install");
                    p.steps.push(install_step(a.id));
                    p.done = format!("{} installed", a.name);
                    p
                }
                (true, true) => {
                    let mut p = Plan::new(&format!("Update {}?", a.name), "Gets the newest version from Flathub.", "Update");
                    p.steps.push(Step::Host(flatpak(&["update", "-y", "--noninteractive", a.id])));
                    p.done = format!("{} updated", a.name);
                    p
                }
                (true, false) => {
                    let mut p = Plan::new(&format!("Remove {}?", a.name), "The app goes. Its own settings and files stay, in case it comes back.", "Remove");
                    p.danger = true;
                    p.steps.push(Step::Host(flatpak(&["uninstall", "-y", "--noninteractive", a.id])));
                    p.done = format!("{} removed", a.name);
                    p
                }
            };
            p.items.push((a.name.to_string(), a.id.to_string()));
            Some(dialog(ui, p))
        }
        "tile-remove" => {
            let t = tile(arg)?;
            if !tile_added(ui, t) {
                return Some(toast("It isn't in your library."));
            }
            if ui.steam_running {
                return Some(guide(&format!("Remove {} in Steam", t.name), if ui.steam_unsure { "Turbine Hub couldn't check whether Steam is running just now, so it leaves Steam's list alone. Steam lets go of a tile from its own menu." } else { "Steam is running, and it only lets go of a tile from its own menu." }, &[("1", "Find the tile in your library, under Non-Steam."), ("2", "Open its options, then Manage, then Remove non-Steam game.")]));
            }
            let mut p = Plan::new(&format!("Remove {} from your library?", t.name), "The tile goes from Steam's library. Chrome and your sign-in there stay.", "Remove");
            p.danger = true;
            p.items.push((t.name.to_string(), t.url.to_string()));
            p.steps.push(Step::TileRemove { url: t.url.into() });
            p.done = format!("{} removed from your library", t.name);
            Some(dialog(ui, p))
        }
        "update-all" => {
            let names: Vec<&str> = APPS.iter().filter(|a| ui.updates.iter().any(|u| u == a.id)).map(|a| a.name).collect();
            let mut p = Plan::new("Update everything?", "Gets the newest version of every app with an update. No password needed.", "Update all");
            p.items = names.iter().map(|x| (x.to_string(), String::new())).collect();
            p.steps.push(Step::Host(flatpak(&["update", "-y", "--noninteractive"])));
            p.done = "Everything is up to date".into();
            Some(dialog(ui, p))
        }
        "tile" => {
            let t = tile(arg)?;
            if tile_added(ui, t) {
                return Some(toast("It's in your library already."));
            }
            let mut p = Plan::new(&format!("Add {} to your library?", t.name), "It becomes a tile in Steam that opens full screen and works with a controller. Steam shows it after its next start.", "Add tile");
            if !is_installed(ui, CHROME) {
                p.items.push(("Chrome".into(), "The tile opens in it. Installed from Flathub first.".into()));
            }
            p.items.push((t.name.to_string(), t.url.to_string()));
            if ui.steam_unsure {
                p.items.push(("Close Steam first".into(), super::selftile::UNSURE.into()));
            } else if ui.steam_running {
                p.items.push(("Steam is running".into(), "Steam only reads new tiles when it starts. Quit Steam in Desktop Mode first, or the tile can't be added.".into()));
            }
            p.steps = tile_steps(ui, t);
            p.done = format!("{} added. It shows after Steam starts again.", t.name);
            Some(dialog(ui, p))
        }
        "panel-guide" => {
            if panel_installed(ui) {
                return Some(toast("The quick panel is installed already."));
            }
            let mut items = vec![
                ("1", "Press the button with three dots (Steam's quick menu), pick the plug icon, then the gear at the top: Decky Loader's settings."),
                ("2", "Under General, turn on Developer mode. It only adds a Developer page to Decky's settings; SteamOS doesn't change."),
                ("3", "On the Developer page, pick Install Plugin from URL. Steam button and X brings up the keyboard."),
                ("Address", PANEL_URL),
                ("4", "Type the address, pick Install and confirm. Turbine Hub shows under the plug icon."),
                ("Afterwards", "Turn Developer mode off again if you like. The quick panel stays."),
            ];
            if !decky_installed(ui) {
                items.insert(0, ("First", "Install Decky Loader: its row here in Store, Decky plugins."));
            }
            Some(guide("Get the Turbine Hub quick panel", "The quick panel puts Turbine Hub's readings and the running game's switches into Steam's quick menu. It isn't in Decky's own store, so Decky Loader installs it from an address: Turbine Hub's newest release. That takes a switch in Decky's settings and typing one address, a few minutes once.", &items))
        }
        "decky" => {
            let Some(tag) = ui.decky_tag.clone() else { return Some(toast("Couldn't reach GitHub to find Decky Loader's newest release. Try again in a moment.")) };
            let repair = decky_installed(ui);
            let mut p = Plan::new(if repair { "Repair Decky Loader?" } else { "Install Decky Loader?" }, "Turbine Hub's helper downloads Decky Loader's official release and installs it only when it matches the checksum GitHub publishes.", if repair { "Repair" } else { "Install" });
            p.items.push((format!("Decky Loader {tag}"), "From github.com/SteamDeckHomebrew/decky-loader".into()));
            p.steps.push(Step::Host(helper::job("decky-install", Some(&format!("{{\"tag\":{}}}", json::quote(&tag))))));
            p.done = if repair { "Decky Loader repaired. Its plug icon is in Steam's quick menu." } else { "Decky Loader installed. Its plug icon is in Steam's quick menu." }.into();
            Some(with_helper(ui, p))
        }
        "decky-off" => {
            let turn_on = plugins_off(ui);
            let setting = Setting::Service { unit: DECKY_SERVICE.into(), enabled: turn_on };
            let mut p = Plan::new(if turn_on { "Turn plugins back on?" } else { "Turn all plugins off?" }, if turn_on { "Decky Loader starts again with every plugin." } else { "Decky Loader stops, and every plugin with it, until this is turned back on. Nothing is removed." }, if turn_on { "Turn on" } else { "Turn all off" });
            p.items.push(super::plan::change_item(ui, &setting));
            p.done = if turn_on { "Plugins on".into() } else { "Plugins off".into() };
            Some(helper_plan(ui, p, &[setting]))
        }
        _ => None,
    }
}
