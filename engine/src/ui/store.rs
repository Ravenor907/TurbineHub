//! Store: what Turbine Hub offers to install, by section, with each thing's state on this machine. Apps come from
//! Flathub through the host's flatpak, Decky Loader through the helper, and TV and cloud services become library tiles.
use super::plan::{Plan, Step, dialog, guide, helper_plan, toast, with_helper};
use super::{Ui, arr, b, hint, n, obj, row, s, section, with};
use crate::helper::{self, Setting};
use crate::json::{self, Json};

pub const LUDUSAVI: &str = "com.github.mtkennerly.ludusavi";
pub const TURBINE: &str = "io.github.ravenor907.TurbineModManager";
pub const CHROME: &str = "com.google.Chrome";
pub const HEROIC: &str = "com.heroicgameslauncher.hgl";
const DECKY_SERVICE: &str = "plugin_loader.service";

pub struct App {
    pub id: &'static str,
    pub name: &'static str,
    pub line: &'static str,
    pub section: &'static str,
}

const fn app(section: &'static str, id: &'static str, name: &'static str, line: &'static str) -> App {
    App { id, name, line, section }
}

/// Everything from Flathub the Store lists, in the order shown.
pub const APPS: &[App] = &[
    app("essentials", LUDUSAVI, "Ludusavi", "Save backups for every game"),
    app("essentials", "com.vysp3r.ProtonPlus", "ProtonPlus", "Extra Proton versions"),
    app("essentials", "io.github.radiolamp.mangojuice", "MangoJuice", "Set up the FPS and temps overlay"),
    app("essentials", "com.github.tchx84.Flatseal", "Flatseal", "What each app may reach"),
    app("apps", "com.discordapp.Discord", "Discord", "Voice chat and friends"),
    app("apps", "com.moonlight_stream.Moonlight", "Moonlight", "Stream games from your PC"),
    app("apps", "com.obsproject.Studio", "OBS Studio", "Record and stream"),
    app("apps", "com.spotify.Client", "Spotify", "Music while you play"),
    app("apps", CHROME, "Chrome", "The browser the library tiles use"),
    app("apps", "org.mozilla.firefox", "Firefox", "A browser for Desktop Mode"),
    app("apps", "org.videolan.VLC", "VLC", "Plays any video file"),
    app("launchers", HEROIC, "Heroic Games Launcher", "Epic, GOG and Amazon games"),
    app("launchers", "net.lutris.Lutris", "Lutris", "Other launchers and older games"),
    app("launchers", "com.usebottles.bottles", "Bottles", "Windows programs that aren't games"),
    app("emulation", "net.retrodeck.retrodeck", "RetroDECK", "Classic systems in one app"),
];

pub struct Tile {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub line: &'static str,
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

/// Windows launchers that need NonSteamLaunchers.
const NSL: &[(&str, &str)] = &[("battlenet", "Battle.net"), ("ea", "EA app"), ("ubisoft", "Ubisoft Connect"), ("rockstar", "Rockstar Games Launcher")];

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

fn app_row(ui: &Ui, a: &App, removing: bool) -> Json {
    let installed = is_installed(ui, a.id);
    let update = ui.updates.iter().any(|u| u == a.id);
    let (badge, tone, button) = match (installed, update, removing) {
        (true, _, true) => ("", "", "Remove"),
        (true, true, _) => ("Update ready", "warn", "Update"),
        (true, false, _) => ("Installed", "ok", "Remove"),
        _ => ("", "", "Install"),
    };
    let r = with(row("store", a.id, "button", a.name, a.line), "button", s(button));
    with(with(r, "badge", s(badge)), "tone", s(tone))
}

fn decky_installed(ui: &Ui) -> bool {
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
    let mut out: Vec<Json> = APPS.iter().filter(|a| a.section == sec).map(|a| app_row(ui, a, false)).collect();
    match sec {
        "decky" => {
            let has = decky_installed(ui);
            out.push(with(with(with(row("decky", "", "button", "Decky Loader", "Plugins for Steam's quick menu, from its official release"), "button", s(if has { "Repair" } else { "Install" })), "badge", s(if has { "Installed" } else { "" })), "tone", s("ok")));
            if has {
                out.push(with(row("decky-off", "", "toggle", "Plugins on", "Turn every plugin off to see if one of them causes a problem"), "on", b(!plugins_off(ui))));
                out.extend(plugins(ui).iter().map(|p| row("none", "", "info", p, "Plugin")));
            }
        }
        "launchers" => out.extend(NSL.iter().map(|(id, name)| with(row("launcher-guide", id, "button", name, "Installs through NonSteamLaunchers"), "button", s("How")))),
        "tv" => out.extend(TILES.iter().map(|t| {
            let added = tile_added(ui, t);
            with(with(with(row("tile", t.id, if added { "info" } else { "button" }, t.name, t.line), "button", s(if added { "" } else { "Add tile" })), "badge", s(if added { "In your library" } else { "" })), "tone", s("ok"))
        })),
        "emulation" => out.push(row("none", "", "info", "EmuDeck", "Installs from emudeck.com in Desktop Mode. RetroDECK above needs no setup outside the app.")),
        "installed" => out = APPS.iter().filter(|a| is_installed(ui, a.id)).map(|a| app_row(ui, a, true)).collect(),
        _ => {}
    }
    out
}

/// The Store tab. `arg` is the section.
pub(crate) fn page(ui: &mut Ui, arg: &str) -> Json {
    let index = SECTIONS.iter().position(|(id, _, _)| *id == arg).unwrap_or(0);
    let (id, title, line) = SECTIONS[index];
    let list = rows(ui, id);
    let empty = if list.is_empty() { "Nothing installed from the Store yet." } else { "" };
    let updates = APPS.iter().filter(|a| ui.updates.iter().any(|u| u == a.id)).count();
    obj! {
        "side": arr(SECTIONS.iter().map(|(id, title, _)| obj! {"id": s(*id), "title": s(*title), "chip": s(""), "tone": s(""), "lock": b(false)})),
        "sideTitle": s(""),
        "index": n(index as f64),
        "header": obj! {"title": s(title), "chips": arr(if ui.offline { vec![obj! {"text": s("Offline now. This is what was found last; installing needs the internet."), "tone": s("warn")}] } else { vec![] }), "line": s(line)},
        "sections": arr([with(with(section("", list), "columns", n(2.0)), "empty", s(empty))]),
        "hints": arr(if updates > 0 { vec![hint("y", "Update all")] } else { vec![] }),
        "y": if updates > 0 { obj! {"act": s("update-all"), "arg": s("")} } else { Json::Null },
    }
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    match action {
        "store" => {
            let a = APPS.iter().find(|a| a.id == arg)?;
            let installed = is_installed(ui, a.id);
            let update = ui.updates.iter().any(|u| u == a.id);
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
            if ui.steam_running {
                p.items.push(("Steam is running".into(), "Steam only reads new tiles when it starts. Quit Steam in Desktop Mode first, or the tile can't be added.".into()));
            }
            p.steps = tile_steps(ui, t);
            p.done = format!("{} added. It shows after Steam starts again.", t.name);
            Some(dialog(ui, p))
        }
        "launcher-guide" => {
            let (_, name) = NSL.iter().find(|(id, _)| *id == arg)?;
            Some(guide(&format!("{name} on SteamOS"), "Windows launchers install through NonSteamLaunchers, a Decky plugin. Each one becomes a tile in your library.", &[("1", "Install Decky Loader: Store, Decky plugins."), ("2", "Open Steam's quick menu, the plug icon, then the store icon."), ("3", "Install NonSteamLaunchers and pick the launcher there.")]))
        }
        "decky" => {
            let Some(tag) = ui.decky_tag.clone() else { return Some(toast("Couldn't reach GitHub to find Decky Loader's newest release. Try again in a moment.")) };
            let mut p = Plan::new(if decky_installed(ui) { "Repair Decky Loader?" } else { "Install Decky Loader?" }, "Turbine Hub's helper downloads Decky Loader's official release and installs it only when it matches the checksum GitHub publishes.", "Install");
            p.items.push((format!("Decky Loader {tag}"), "From github.com/SteamDeckHomebrew/decky-loader".into()));
            p.steps.push(Step::Host(helper::job("decky-install", Some(&format!("{{\"tag\":{}}}", json::quote(&tag))))));
            p.done = "Decky Loader installed. Its plug icon is in Steam's quick menu.".into();
            Some(with_helper(ui, p))
        }
        "decky-off" => {
            let turn_on = plugins_off(ui);
            let setting = Setting::Service { unit: DECKY_SERVICE.into(), enabled: turn_on };
            let mut p = Plan::new(if turn_on { "Turn plugins back on?" } else { "Turn all plugins off?" }, if turn_on { "Decky Loader starts again with every plugin." } else { "Decky Loader stops, and every plugin with it, until this is turned back on. Nothing is removed." }, if turn_on { "Turn on" } else { "Turn all off" });
            p.items.push((setting.words(), String::new()));
            p.done = if turn_on { "Plugins on".into() } else { "Plugins off".into() };
            Some(helper_plan(ui, p, &[setting]))
        }
        _ => None,
    }
}
