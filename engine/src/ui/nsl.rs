//! NonSteamLaunchers in the Store: its own page, its row, the rows of the launchers it installs, and Install, Open,
//! Update and its Decky plugin. What it is and what its scripts do is in `crate::nsl`. Its windows only show in
//! Desktop Mode, so in Game Mode every press explains how to get there instead.
use super::plan::{Plan, Step, dialog, guide, toast};
use super::store::{HEROIC, decky_installed, is_installed};
use super::{Ui, arr, obj, row, s, section, with};
use crate::json::Json;
use crate::nsl::{self, Launcher, State};

pub const NAME: &str = "NonSteamLaunchers";
/// The words under its name in a list, and what a search looks at besides the name.
pub const LINE: &str = "Battle.net, EA app, Ubisoft Connect and more, in your library";
pub const WORDS: &str = "nonsteam nsl launchers windows launcher battle.net ea ubisoft rockstar itch.io legacy humble epic gog amazon";
const ABOUT: &str = "Installs Windows game launchers, such as Battle.net, EA app and Ubisoft Connect, and adds them and their games to your Steam library. A free project by moraroy, not part of Steam or Turbine Hub.";

pub(crate) fn state(ui: &Ui) -> State {
    nsl::state(&ui.ctx.home, &ui.folder())
}

/// A newer release than the one Turbine Hub last started. Not known (set up some other way) counts as none.
fn update(ui: &Ui, st: &State) -> Option<String> {
    match (&ui.nsl_tag, &st.version) {
        (Some(tag), Some(have)) if st.installed && nsl::newer(tag, have) => Some(tag.clone()),
        _ => None,
    }
}

fn info(title: &str, line: &str) -> Json {
    row("none", "", "info", title, line)
}

/// Its row in the Store's lists: its state as a badge, a press opens its page.
pub(crate) fn store_row(ui: &Ui) -> Json {
    let st = state(ui);
    let (badge, tone) = match (st.installed, update(ui, &st).is_some()) {
        (true, true) => ("Update ready", "warn"),
        (true, false) => ("Installed", "ok"),
        _ => ("", ""),
    };
    with(with(row("open", "store-item|nsl", "button", NAME, LINE), "badge", s(badge)), "tone", s(tone))
}

/// What a launcher's row says: installed, what to tick, or that NonSteamLaunchers brings it. Epic, GOG and Amazon
/// point to Heroic first.
pub(crate) fn launcher_line(ui: &Ui, st: &State, x: &Launcher) -> String {
    if st.launchers.contains(&x.id) {
        return "In your library, through NonSteamLaunchers".into();
    }
    let heroic = if is_installed(ui, HEROIC) { "Heroic, which you have, is the easier way" } else { "Heroic Games Launcher is the easier way" };
    match (x.heroic, st.installed) {
        (true, true) => format!("{heroic}. Or open NonSteamLaunchers and tick {}", x.tick),
        (true, false) => format!("{heroic}. NonSteamLaunchers has it too"),
        (false, true) => format!("Open NonSteamLaunchers and tick {}", x.tick),
        (false, false) => "Installs through NonSteamLaunchers".into(),
    }
}

/// A launcher's row: it leads to NonSteamLaunchers' page.
pub(crate) fn launcher_row(ui: &Ui, st: &State, x: &Launcher) -> Json {
    let here = st.launchers.contains(&x.id);
    with(with(row("open", "store-item|nsl", "button", x.name, &launcher_line(ui, st, x)), "badge", s(if here { "Installed" } else { "" })), "tone", s("ok"))
}

pub(crate) fn launcher_rows(ui: &Ui) -> Vec<Json> {
    let st = state(ui);
    nsl::LAUNCHERS.iter().map(|x| launcher_row(ui, &st, x)).collect()
}

/// The page: what it is, its state and version, Install or Open and Update, and its Decky plugin when Decky Loader
/// is there.
pub(crate) fn item(ui: &Ui) -> Json {
    let st = state(ui);
    let desk = ui.window;
    let newer = update(ui, &st);
    let mut actions = Vec::new();
    let how = |label: &str| if desk { label.to_string() } else { "How".into() };
    if st.installed {
        let line = if desk { "Its window opens: tick launchers to add them, or pick Uninstall" } else { "It opens in Desktop Mode, in its own window" };
        actions.push(with(with(row("nsl", "open", "button", "Open", line), "icon", s("external")), "button", s(how("Open"))));
        if let Some(tag) = &newer {
            actions.push(with(with(with(with(row("nsl", "update", "button", "Update", &format!("Version {tag} is ready")), "icon", s("refresh")), "button", s(how("Update"))), "badge", s("Update ready")), "tone", s("warn")));
        }
    } else {
        let line = if desk { "Its window opens, where you tick the launchers you want. No password needed." } else { "It opens in Desktop Mode, in its own window" };
        actions.push(with(with(row("nsl", "install", "button", "Install", line), "icon", s("download")), "button", s(how("Install"))));
    }
    if decky_installed(ui) {
        match &st.plugin {
            Some(v) => actions.push(with(with(info("Decky plugin", &if v.is_empty() { "Installed. In Steam's quick menu, the plug icon.".to_string() } else { format!("Version {v}. In Steam's quick menu, the plug icon.") }), "badge", s("Installed")), "tone", s("ok"))),
            None => actions.push(with(with(row("nsl-plugin", "", "button", "Get its Decky plugin", "Pick launchers from Steam's quick menu, in Game Mode. Set up once from Desktop Mode."), "icon", s("download")), "button", s(how("Get")))),
        }
    }
    let state_row = match (st.installed, &st.version) {
        (true, Some(v)) => with(with(info("Installed", &format!("Version {v}")), "badge", s("Installed")), "tone", s("ok")),
        (true, None) => with(with(info("Installed", "Set up outside Turbine Hub, so its version isn't known"), "badge", s("Installed")), "tone", s("ok")),
        (false, _) => info("Not installed", "Nothing of it is on this machine."),
    };
    let names: Vec<&str> = nsl::LAUNCHERS.iter().filter(|x| st.launchers.contains(&x.id)).map(|x| x.name).collect();
    let mut about = vec![state_row];
    if st.installed {
        about.push(info("In your library", &if names.is_empty() { "No launchers ticked yet".to_string() } else { names.join(", ") }));
    }
    let newest = match (&ui.nsl_tag, ui.offline) {
        (Some(tag), false) => format!("Version {tag}"),
        (Some(tag), true) => format!("Version {tag}, as found last. Offline now."),
        (None, _) => "Not known yet. It shows once the internet is there.".to_string(),
    };
    about.push(info("Newest", &newest));
    about.push(info("Opens in", "Desktop Mode, in its own windows. The launchers and their games then start from your library in Game Mode."));
    about.push(info("What it adds", "One set of Windows files for all its launchers, GE-Proton to run them, and a game scanner that adds each game you install to Steam."));
    about.push(info("Source", &format!("Its official script from {}, at its release", nsl::PAGE)));
    about.push(info("Remove", "Open it and pick Uninstall for one launcher, or Start Fresh for everything it added. Its backups of your saves stay."));
    obj! {
        "crumb": s("Store"),
        "title": s(NAME),
        "intro": s(ABOUT),
        "sections": arr([section("", actions), section("About", about)]),
    }
}

/// In Game Mode: how to get to Desktop Mode, where its window shows.
fn desktop_guide(ui: &Ui, title: &str) -> Json {
    let mut items = vec![
        ("1", "Press the Steam button, then Power, then Switch to Desktop."),
        ("2", "Open Turbine Hub from the application menu, bottom left."),
        ("3", "Store, Launchers, NonSteamLaunchers. Press the same button there."),
        ("4", "When you're done, Return to Gaming Mode is on the desktop."),
    ];
    let st = state(ui);
    if decky_installed(ui) && st.plugin.is_none() {
        items.push(("Or", "Get its Decky plugin on that page too. Then you pick launchers from Steam's quick menu, right here in Game Mode."));
    } else if st.plugin.is_some() && st.installed {
        items.insert(0, ("Here", "Its Decky plugin is installed: Steam's quick menu, the plug icon, NonSteamLaunchers."));
    }
    guide(title, "It shows its own windows, and only Desktop Mode has windows. Your launchers and their games start from Game Mode afterwards.", &items)
}

fn no_tag() -> Json {
    toast("Couldn't reach GitHub to find NonSteamLaunchers' newest release. Try again in a moment.")
}

/// The plan for Install, Open or Update: fetch the script at the newest release tag and start it.
pub(crate) fn plan(ui: &Ui, what: &str) -> Result<Plan, Json> {
    let st = state(ui);
    let tag = ui.nsl_tag.clone().filter(|t| nsl::tag_ok(t)).ok_or_else(no_tag)?;
    let (title, confirm) = match (what, st.installed) {
        ("update", true) => ("Update NonSteamLaunchers?", "Update"),
        (_, true) => ("Open NonSteamLaunchers?", "Open"),
        _ => ("Install NonSteamLaunchers?", "Install"),
    };
    let text = if st.installed { "Turbine Hub gets its official script and starts it. Its window opens: tick launchers to add them, or pick Uninstall to take one away." } else { "Turbine Hub gets its official script and starts it. Its window opens: tick the launchers you want and press OK. It installs them and adds them and their games to your Steam library." };
    let mut p = Plan::new(title, text, confirm);
    p.items.push(("Get its script".into(), format!("{}, release {tag}, from {}", nsl::SCRIPT, nsl::PAGE)));
    p.items.push(("Start it".into(), "Its own window opens on this desktop. It needs no password.".into()));
    if !st.installed {
        p.items.push(("What it adds".into(), "One set of Windows files for its launchers, GE-Proton, their entries in your library, and a game scanner that keeps adding new games.".into()));
        p.items.push(("Steam restarts".into(), "Once, the first time, so the new entries show.".into()));
    }
    p.steps.push(Step::Nsl { tag, plugin: false });
    p.done = "NonSteamLaunchers is opening in its own window.".into();
    Ok(p)
}

/// The plan for its Decky plugin: its official installer at the same release tag.
pub(crate) fn plugin_plan(ui: &Ui) -> Result<Plan, Json> {
    let tag = ui.nsl_tag.clone().filter(|t| nsl::tag_ok(t)).ok_or_else(no_tag)?;
    let mut p = Plan::new("Get the NonSteamLaunchers Decky plugin?", "Turbine Hub gets the plugin's official installer and starts it. Then you can pick launchers from Steam's quick menu in Game Mode.", "Get plugin");
    p.items.push(("Get its installer".into(), format!("{}, release {tag}, from {}", nsl::PLUGIN_SCRIPT, nsl::PAGE)));
    p.items.push(("Its window asks".into(), "Install or update, then your SteamOS password, because Decky's plugin folder belongs to the system. Turbine Hub never sees that password.".into()));
    p.items.push(("The plugin".into(), format!("Its newest version, from {}", nsl::PLUGIN_PAGE)));
    p.steps.push(Step::Nsl { tag, plugin: true });
    p.done = "The plugin's installer is opening in its own window.".into();
    Ok(p)
}

pub(crate) fn act(ui: &mut Ui, action: &str, arg: &str, _value: &str) -> Option<Json> {
    match action {
        "nsl" => {
            let title = match arg {
                "update" => "Update NonSteamLaunchers in Desktop Mode",
                "open" => "Open NonSteamLaunchers in Desktop Mode",
                _ if state(ui).installed => "Open NonSteamLaunchers in Desktop Mode",
                _ => "Install NonSteamLaunchers in Desktop Mode",
            };
            if !ui.window {
                return Some(desktop_guide(ui, title));
            }
            Some(match plan(ui, arg) {
                Ok(p) => dialog(ui, p),
                Err(j) => j,
            })
        }
        "nsl-plugin" => {
            if !decky_installed(ui) {
                return Some(toast("Install Decky Loader first: Store, Decky plugins."));
            }
            if state(ui).plugin.is_some() {
                return Some(toast("Its Decky plugin is installed already."));
            }
            if !ui.window {
                return Some(desktop_guide(ui, "Get its Decky plugin in Desktop Mode"));
            }
            Some(match plugin_plan(ui) {
                Ok(p) => dialog(ui, p),
                Err(j) => j,
            })
        }
        _ => None,
    }
}
