//! A made-up machine with a home folder, games, ratings, a Windows drive and a few recorded changes, so the pictures
//! and the tests show every screen with believable content and never touch the real system.
use super::Ui;
use crate::checkup::Facts;
use crate::cli::Ctx;
use crate::fake::{self, Preset};
use crate::helper::{Change, Ledger, Setting};
use crate::sensors::Readings;
use crate::settings::{Mode, Settings};
use std::fs;
use std::path::Path;

/// NonSteamLaunchers' newest release on the made-up machine.
pub const NSL_TAG: &str = "v4.2.94";

/// The made-up handheld of the pictures and tests.
pub const HANDHELD: Preset = Preset::Handheld;

/// The pictures of a screenshot run: (name, arguments).
pub const SHOTS: &[(&str, &str)] = &[
    ("pick", "--page=pick --machine=handheld"),
    ("setup-machine", "--page=setup --arg=machine --machine=handheld --mode=guided"),
    ("setup-password", "--page=setup --arg=password --machine=handheld --mode=guided"),
    ("setup-password-new", "--page=setup --arg=password --machine=handheld --mode=guided --no-password"),
    ("dialog-password", "--page=setup --arg=password --act=password: --machine=handheld --mode=guided --no-password"),
    ("setup-games", "--page=setup --arg=games --machine=handheld --mode=guided"),
    ("setup-bring", "--page=setup --arg=bring --machine=handheld --mode=guided"),
    ("setup-picks", "--page=setup --arg=picks --machine=handheld --mode=guided"),
    ("quick-setup", "--page=quick --machine=handheld --mode=veteran"),
    ("home-guided", "--page=home --machine=handheld --mode=guided"),
    ("home-veteran", "--page=home --machine=handheld --mode=veteran"),
    ("games", "--page=games --machine=handheld --mode=guided"),
    ("games-locked", "--page=games --arg=steam:1245620 --machine=handheld --mode=guided"),
    ("games-switches", "--page=games --arg=steam:1086940 --machine=handheld --mode=guided"),
    ("games-mods", "--page=games --arg=steam:367520 --machine=handheld --mode=guided"),
    ("games-search", "--page=games --act=search:games --value=el --machine=handheld --mode=guided"),
    ("games-search-none", "--page=games --act=search:games --value=zelda --machine=handheld --mode=guided"),
    ("tool", "--page=tool --machine=handheld --mode=guided"),
    ("tool-new", "--page=tool --machine=desktop --mode=guided --no-tool"),
    ("dialog-tool", "--page=tool --act=tool:install --machine=desktop --mode=guided --no-tool"),
    ("dialog-tool-how", "--page=games --arg=steam:1091500 --act=tool-how:steam:1091500 --machine=handheld --mode=guided"),
    ("dialog-tool-remove", "--page=tool --act=tool:remove --machine=handheld --mode=guided"),
    ("proton", "--page=proton --arg=steam:1086940 --machine=handheld --mode=guided"),
    ("dialog-proton", "--page=proton --arg=steam:1086940 --act=proton-pick:steam:1086940|GE-Proton10-4 --machine=handheld --mode=guided"),
    ("saves", "--page=saves --arg=steam:1086940 --machine=handheld --mode=guided --installed=com.github.mtkennerly.ludusavi"),
    ("saves-empty", "--page=saves --arg=steam:2379780 --machine=handheld --mode=guided"),
    ("dialog-restore", "--page=saves --arg=steam:1086940 --keys=down,a --machine=handheld --mode=guided --installed=com.github.mtkennerly.ludusavi"),
    ("library", "--page=library --machine=handheld --mode=guided"),
    ("library-offline", "--page=library --machine=handheld --mode=guided --offline"),
    ("store", "--page=store --arg=apps --machine=handheld --mode=guided"),
    ("store-offline", "--page=store --arg=apps --machine=handheld --mode=guided --offline"),
    ("store-decky", "--page=store --arg=decky --machine=handheld --mode=guided"),
    ("store-tv", "--page=store --arg=tv --machine=tv --mode=guided"),
    ("store-search", "--page=store --act=search:store --value=stream --machine=handheld --mode=guided"),
    ("store-item", "--page=store-item --arg=com.discordapp.Discord --machine=handheld --mode=guided"),
    ("store-item-new", "--page=store-item --arg=com.spotify.Client --machine=handheld --mode=guided"),
    ("store-item-update", "--page=store-item --arg=com.heroicgameslauncher.hgl --machine=handheld --mode=guided --offline"),
    ("store-item-tile", "--page=store-item --arg=tile:netflix --machine=tv --mode=guided"),
    ("store-item-decky", "--page=store-item --arg=decky --machine=handheld --mode=guided"),
    ("store-launchers", "--page=store --arg=launchers --machine=handheld --mode=guided"),
    ("store-launchers-nsl", "--page=store --arg=launchers --machine=handheld --mode=guided --nsl"),
    ("store-item-nsl", "--page=store-item --arg=nsl --machine=handheld --mode=guided"),
    ("store-item-nsl-desktop", "--page=store-item --arg=nsl --machine=desktop --mode=guided --window"),
    ("store-item-nsl-installed", "--page=store-item --arg=nsl --machine=desktop --mode=guided --window --nsl"),
    ("dialog-nsl", "--page=store-item --arg=nsl --act=nsl:install --machine=desktop --mode=guided --window"),
    ("dialog-nsl-game-mode", "--page=store-item --arg=nsl --act=nsl:install --machine=handheld --mode=guided"),
    ("dialog-nsl-plugin", "--page=store-item --arg=nsl --act=nsl-plugin: --machine=desktop --mode=guided --window --nsl"),
    ("machine", "--page=machine --machine=handheld --mode=guided"),
    ("lighting", "--page=module:lighting --machine=handheld --mode=guided"),
    ("lighting-veteran", "--page=module:lighting --machine=handheld --mode=veteran"),
    ("lighting-preset", "--page=module:lighting --act=light:preset --value=10 --machine=handheld --mode=guided"),
    ("lighting-spiral", "--page=module:lighting --act=light:effect --value=8 --machine=handheld --mode=veteran"),
    ("lighting-scrolled", "--page=module:lighting --keys=down,down,down,down,down,down,down,down,a --machine=handheld --mode=guided"),
    ("lighting-cards", "--page=module:lighting --keys=down,right,right,down --machine=handheld --mode=guided"),
    ("dialog-lighting", "--page=module:lighting --act=light-test: --machine=handheld --mode=guided"),
    ("lighting-other-app", "--page=module:lighting --machine=handheld --mode=guided --lights=allyhub"),
    ("lighting-other-huesync", "--page=module:lighting --machine=handheld --mode=guided --lights=huesync"),
    ("dialog-lighting-other", "--page=module:lighting --act=light-rival:allyhub --machine=handheld --mode=guided --lights=allyhub"),
    ("lighting-left-to-other", "--page=module:lighting --act=light:allyhub --machine=handheld --mode=guided --lights=allyhub"),
    ("lighting-closed", "--page=module:lighting --machine=handheld --mode=veteran --lights=closed"),
    ("dialog-lighting-open", "--page=module:lighting --act=light-test: --machine=handheld --mode=veteran --lights=closed"),
    ("lighting-stuck", "--page=module:lighting --machine=handheld --mode=guided --lights=stuck"),
    ("lighting-test", "--page=module:lighting --act=light-test: --machine=handheld --mode=guided --lights=open"),
    ("lighting-test-veteran", "--page=module:lighting --act=light-test: --machine=handheld --mode=veteran --lights=open"),
    ("dialog-lighting-check", "--page=module:lighting --act=light-report: --machine=handheld --mode=guided"),
    ("home-lights-other", "--page=home --machine=handheld --mode=guided --lights=allyhub"),
    ("home-lights-stuck", "--page=home --machine=handheld --mode=guided --lights=stuck"),
    ("power", "--page=module:power --machine=handheld --mode=veteran"),
    ("tuneup", "--page=module:tuneup --machine=handheld --mode=guided"),
    ("sleep", "--page=module:sleep --machine=handheld --mode=guided"),
    ("battery", "--page=module:battery --machine=handheld --mode=guided"),
    ("dock", "--page=module:dock --machine=handheld --mode=guided"),
    ("dialog", "--page=machine --act=tuneup:all --machine=handheld --mode=guided"),
    ("dialog-guide", "--page=home --act=checkup:help:hdmi --machine=tv --mode=guided"),
    ("menu", "--page=home --act=menu: --machine=handheld --mode=guided"),
    ("settings", "--page=settings --machine=handheld --mode=guided"),
    ("settings-pc", "--page=settings --machine=desktop --mode=guided"),
    ("dialog-library", "--page=settings --act=self-tile:add --machine=handheld --mode=guided"),
    ("dialog-library-steam", "--page=settings --act=self-tile:add --machine=handheld --mode=guided --steam-running"),
    ("dialog-library-remove", "--page=settings --act=self-tile:remove --machine=desktop --mode=guided"),
    ("dialog-panel", "--page=store --arg=decky --act=panel-guide: --machine=handheld --mode=guided"),
    ("settings-look", "--page=settings --arg=look --machine=handheld --mode=guided"),
    ("settings-updates", "--page=settings --arg=updates --machine=handheld --mode=veteran --offline"),
    ("changes", "--page=changes --machine=handheld --mode=guided"),
    ("bring", "--page=bring --machine=handheld --mode=guided"),
    ("windows-drive", "--page=windrive --machine=handheld --mode=guided"),
    ("windows-apps", "--page=winapps --machine=handheld --mode=guided"),
    ("wheres-my", "--page=wheres --machine=handheld --mode=guided"),
    ("tv-home", "--page=home --machine=tv --mode=guided"),
    ("pc-home", "--page=home --machine=desktop --mode=guided"),
    ("pc-home-veteran", "--page=home --machine=desktop --mode=veteran"),
    ("pc-machine", "--page=machine --machine=desktop --mode=guided"),
    ("pc-drives", "--page=module:drives --machine=desktop --mode=guided"),
    ("pc-monitors", "--page=module:monitors --machine=desktop --mode=guided"),
    ("pc-sound", "--page=module:sound --machine=desktop --mode=guided"),
    ("pc-gpu", "--page=module:gpu --machine=desktop --mode=veteran"),
    ("home-size-150", "--page=home --machine=handheld --mode=guided --scale=150"),
    ("machine-size-150", "--page=machine --machine=handheld --mode=guided --scale=150"),
    ("settings-size-50", "--page=settings --machine=handheld --mode=guided --scale=50"),
    ("home-light", "--page=home --machine=handheld --mode=veteran --theme=light"),
    ("games-contrast", "--page=games --machine=handheld --mode=guided --theme=contrast"),
];

fn w(base: &Path, rel: &str, text: &str) {
    let f = base.join(rel.trim_start_matches('/'));
    if let Some(d) = f.parent() {
        let _ = fs::create_dir_all(d);
    }
    let _ = fs::write(f, text);
}

/// (app id, name, folder, Valve's category or 0 for none).
const GAMES: &[(&str, &str, &str, u32)] = &[
    ("1086940", "Baldur's Gate 3", "Baldurs Gate 3", 3),
    ("1245620", "Elden Ring", "ELDEN RING", 3),
    ("1091500", "Cyberpunk 2077", "Cyberpunk 2077", 3),
    ("553850", "Helldivers 2", "Helldivers 2", 2),
    ("252950", "Rocket League", "rocketleague", 2),
    ("1172470", "Apex Legends", "Apex Legends", 1),
    ("367520", "Hollow Knight", "Hollow Knight", 3),
    ("2379780", "Balatro", "Balatro", 0),
];

const ANTICHEAT: &str = r#"[{"name":"Elden Ring","status":"Supported","storeIds":{"steam":"1245620"}},{"name":"Helldivers 2","status":"Supported","storeIds":{"steam":"553850"}},{"name":"Rocket League","status":"Running","storeIds":{"steam":"252950"}},{"name":"Apex Legends","status":"Denied","storeIds":{"steam":"1172470"}}]"#;
const FIXES: &str = r#"{"1091500":{"kind":"switch","value":"fsr4","words":"One tap: turn on the FSR 4 upgrade for a sharper picture."}}"#;

fn home(home: &Path) {
    let steam = home.join(".local/share/Steam");
    w(&steam, "steamapps/libraryfolders.vdf", &format!("\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n}}\n", steam.display()));
    for (id, name, dir, _) in GAMES {
        w(&steam, &format!("steamapps/appmanifest_{id}.acf"), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"{dir}\"\n\t\"buildid\"\t\t\"1\"\n}}\n"));
        w(&steam, &format!("steamapps/common/{dir}/game.exe"), "MZ");
        let _ = fs::create_dir_all(steam.join(format!("steamapps/compatdata/{id}/pfx")));
    }
    // Three Protons: two of Steam's own (one needs a runtime, which is installed) and one the player added.
    let manifest = |need: &str| format!("\"manifest\"\n{{\n  \"version\" \"2\"\n  \"commandline\" \"/proton %verb%\"\n{need}}}\n");
    for name in ["Proton 11.0", "Proton - Experimental"] {
        w(&steam, &format!("steamapps/common/{name}/proton"), "");
        w(&steam, &format!("steamapps/common/{name}/toolmanifest.vdf"), &manifest("  \"require_tool_appid\" \"4183110\"\n"));
    }
    w(&steam, "steamapps/appmanifest_4183110.acf", "\"AppState\"\n{\n\t\"appid\"\t\t\"4183110\"\n\t\"name\"\t\t\"Steam Linux Runtime 4.0\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"SteamLinuxRuntime_4\"\n}\n");
    w(&steam, "steamapps/common/SteamLinuxRuntime_4/toolmanifest.vdf", "\"manifest\"\n{\n  \"commandline\" \"/_v2-entry-point --verb=%verb% --\"\n}\n");
    w(&steam, "compatibilitytools.d/GE-Proton10-4/proton", "");
    w(&steam, "compatibilitytools.d/GE-Proton10-4/toolmanifest.vdf", &manifest(""));
    w(&steam, "compatibilitytools.d/GE-Proton10-4/compatibilitytool.vdf", "\"compatibilitytools\"\n{\n  \"compat_tools\"\n  {\n    \"GE-Proton10-4\"\n    {\n      \"install_path\" \".\"\n    }\n  }\n}\n");
    // Steam's own settings: its Proton for all games, one game on Turbine Hub's tool, one on Turbine Mod Manager's.
    w(&steam, "config/config.vdf", &format!("\"InstallConfigStore\"\n{{\n\"Software\"\n{{\n\"Valve\"\n{{\n\"Steam\"\n{{\n\"CompatToolMapping\"\n{{\n\"0\"\n{{\n\"name\" \"proton_11\"\n}}\n\"1086940\"\n{{\n\"name\" \"{}\"\n}}\n\"367520\"\n{{\n\"name\" \"{}\"\n}}\n}}\n}}\n}}\n}}\n}}\n", crate::compat_tool::TOOL_NAME, crate::compat_tool::MOD_TOOL_NAME));
    // A signed-in Steam account, with no non-Steam games yet.
    let _ = fs::create_dir_all(steam.join("userdata/1001/config"));
    w(home, "homebrew/services/PluginLoader", "");
    w(home, "homebrew/plugins/decky-css/plugin.json", "{\"name\":\"CSS Loader\"}");
    w(home, "homebrew/plugins/protondb/plugin.json", "{\"name\":\"ProtonDB Badges\"}");
}

fn cache(cache: &Path, now: u64, offline: bool) {
    w(cache, "anticheat.json", ANTICHEAT);
    w(cache, "fixes.json", FIXES);
    for (id, cat) in GAMES.iter().filter(|g| g.3 > 0).map(|g| (g.0, g.3)).chain([("632470", 3), ("1145360", 3)]) {
        w(cache, &format!("valve/{id}.json"), &format!("{{\"success\":1,\"results\":{{\"appid\":{id},\"resolved_category\":{cat}}}}}"));
    }
    w(cache, "checked", &(now - if offline { 3 * 86_400 } else { 600 }).to_string());
    // What the host's flatpak said last about versions and sizes.
    super::store::remember_installed(cache, "com.heroicgameslauncher.hgl\t2.17.2\t412.6\u{a0}MB\ncom.vysp3r.ProtonPlus\t0.5.9\t9.8\u{a0}MB\ncom.discordapp.Discord\t0.0.98\t318.4\u{a0}MB\n");
    super::store::remember_remote(cache, "com.heroicgameslauncher.hgl\t2.18.0\t148.3\u{a0}MB\t415.0\u{a0}MB\ncom.spotify.Client\t1.2.60\t182.7\u{a0}MB\t511.9\u{a0}MB\ncom.discordapp.Discord\t0.0.98\t101.5\u{a0}MB\t318.4\u{a0}MB\n", now - if offline { 3 * 86_400 } else { 600 });
}

fn sensors(root: &Path, preset: Preset) -> Readings {
    match preset {
        HANDHELD => Readings { cpu_c: Some(58.0), gpu_c: Some(55.0), gpu_w: Some(12.0), battery_pct: Some(78), battery_w: Some(14.2), battery_wh: Some(46.0), charging: false, fan_rpm: Some(3100) },
        Preset::Desktop => Readings { cpu_c: Some(41.0), gpu_c: Some(38.0), gpu_w: Some(34.0), battery_pct: None, battery_w: None, battery_wh: None, charging: false, fan_rpm: Some(880) },
        Preset::TvPc => {
            let _ = root;
            Readings { cpu_c: Some(44.0), gpu_c: Some(48.0), gpu_w: Some(62.0), battery_pct: None, battery_w: None, battery_wh: None, charging: false, fan_rpm: Some(940) }
        }
    }
}

fn ledger(root: &Path, preset: Preset, now: u64) {
    let mut l = Ledger::default();
    let set = |l: &mut Ledger, st: Setting, before: &str, ago: u64| {
        l.record(Change::Set(st), vec![("value".into(), before.into())], now - ago, "3.9.5");
    };
    set(&mut l, Setting::Sysctl { key: "vm.swappiness".into(), value: 100 }, "60", 86_400 * 2);
    set(&mut l, Setting::Sysctl { key: "vm.page-cluster".into(), value: 0 }, "3", 86_400 * 2);
    set(&mut l, Setting::Sysctl { key: "vm.max_map_count".into(), value: 2_147_483_642 }, "1048576", 86_400 * 2);
    if preset == HANDHELD {
        set(&mut l, Setting::Sysfs { path: "/sys/class/power_supply/BAT0/charge_control_end_threshold".into(), value: "80".into() }, "100", 3600 * 5);
        w(root, "sys/class/power_supply/BAT0/charge_control_end_threshold", "80\n");
    }
    w(root, crate::helper::LEDGER, &l.to_text());
}

/// What the background helper would have written on this machine over the last days: it is on and running, with
/// battery readings, sleeps and game sessions on the handheld.
fn agent_files(ui: &Ui, preset: Preset, now: u64) {
    use crate::history::{Sample, Session, Sleep, add_sample, add_session, add_sleep};
    let report = crate::agent::Report { at: now, version: ui.ctx.version.clone(), game: None, update: Some(String::new()), updates: ui.updates.clone(), plugins: Vec::new(), checked: now - 3 * 3600 };
    report.save(&ui.data);
    if preset != HANDHELD {
        return;
    }
    for i in (0..=30u64).rev() {
        add_sample(&ui.data, &Sample { at: now - i * 60, pct: 78 + (i / 8) as u32, watts: Some(13.6 + (i % 3) as f64 * 0.6), charging: false, game: String::new() });
    }
    add_sleep(&ui.data, &Sleep { at: now - 3 * 86_400, secs: 9 * 3600, before: 71, after: 69, charging: false, wakers: Vec::new() });
    add_sleep(&ui.data, &Sleep { at: now - 2 * 86_400, secs: 6 * 3600 + 1200, before: 40, after: 100, charging: true, wakers: Vec::new() });
    add_sleep(&ui.data, &Sleep { at: now - 9 * 3600, secs: 7 * 3600, before: 92, after: 50, charging: false, wakers: vec!["Built-in controller".into()] });
    let play = |game: &str, name: &str, ago: u64, mins: u64, before: u32, after: u32| add_session(&ui.data, &Session { game: game.into(), name: name.into(), start: now - ago, end: now - ago + mins * 60, asleep: 0, before: Some(before), after: Some(after), on_battery: true });
    play("steam:1091500", "Cyberpunk 2077", 4 * 86_400, 75, 96, 41);
    play("steam:1086940", "Baldur's Gate 3", 3 * 86_400 + 40_000, 110, 100, 38);
    play("steam:1086940", "Baldur's Gate 3", 2 * 86_400 + 30_000, 64, 88, 50);
    play("steam:367520", "Hollow Knight", 86_400, 95, 90, 62);
    play("steam:2379780", "Balatro", 20_000, 130, 97, 70);
}

pub(crate) fn build(dir: &Path, preset: Preset, mode: Option<Mode>) -> Ui {
    let _ = fs::remove_dir_all(dir);
    let root = dir.join("root");
    fake::machine(&root, preset);
    if preset == HANDHELD {
        // The sample handheld is no particular device: its firmware just says what it is.
        w(&root, "sys/class/dmi/id/product_name", "Handheld\n");
        w(&root, "sys/bus/usb/devices/3-1/product", "Built-in controller\n");
    }
    let now = crate::now();
    let ctx = Ctx { root: root.clone(), home: dir.join("home"), config_home: dir.join("config"), efibootmgr: if preset == Preset::TvPc { String::new() } else { fake::EFI_WITH_WINDOWS.into() }, version: "0.1".into() };
    home(&ctx.home);
    ledger(&root, preset, now);
    let st = Settings { mode, setup_done: vec!["machine".into(), "password".into(), "games".into()], ..Settings::default() };
    let _ = st.save(&ctx.config_home);
    let mut ui = Ui::new(ctx, dir.join("cache"));
    cache(&ui.cache, now, false);
    ui.sample = true;
    ui.readings = Some(sensors(&root, preset));
    ui.installed = vec![super::store::HEROIC.into(), "com.vysp3r.ProtonPlus".into(), "com.discordapp.Discord".into()];
    // The launch tool is set up and Steam has started since; one game has a switch on and a few snapshots.
    let steam = ui.ctx.home.join(".local/share/Steam");
    let _ = crate::compat_tool::install(&steam, &ui.folder());
    ui.steam_started = Some(now + 60);
    let mut switches = super::Switches::default();
    switches.set("steam:1086940", crate::switches::Switch::Fsr4, true);
    switches.save(&ui.folder());
    for (ago, kind, bytes) in [(1800, "launch", 3_400_000), (86_400 * 2, "launch", 3_300_000), (86_400 * 9, "manual", 2_900_000)] {
        let d = crate::snapshots::game_dir(&ui.data, "steam:1086940").join((now - ago).to_string());
        let _ = fs::create_dir_all(&d);
        let _ = fs::write(d.join("snapshot.json"), format!("{{\"game\":\"steam:1086940\",\"name\":\"Baldur's Gate 3\",\"at\":{},\"kind\":\"{kind}\",\"bytes\":{bytes},\"files\":12}}\n", now - ago));
    }
    ui.updates = vec![super::store::HEROIC.into()];
    ui.decky_tag = Some("v3.1.10".into());
    ui.nsl_tag = Some(NSL_TAG.into());
    ui.free_bytes = Some(640_000_000_000);
    ui.sound = vec![("speakers".into(), "Speakers".into()), ("headset".into(), "Headset".into()), ("hdmi".into(), "TV over HDMI".into())];
    ui.sound_now = "headset".into();
    ui.facts = match preset {
        HANDHELD => Facts { updates: vec!["Heroic Games Launcher".into()], has_password: Some(true), windows_not_brought: true, ..Facts::default() },
        Preset::Desktop => Facts { current_hz: Some(60), screens: vec![crate::checkup::ScreenNow { name: "DP-4".into(), hz: Some(60), vrr: "automatic".into(), hdr: Some(false) }], updates: vec!["Heroic Games Launcher".into()], has_password: Some(true), windows_not_brought: true, ..Facts::default() },
        Preset::TvPc => Facts { updates: vec!["Heroic Games Launcher".into()], has_password: Some(true), ..Facts::default() },
    };
    if preset == Preset::Desktop {
        // On the PC Turbine Hub is in the Steam library already, added by itself; its pictures aren't there yet.
        let _ = crate::shortcuts::add_shortcut(&ui.ctx.home.join(".local/share/Steam/userdata/1001/config/shortcuts.vdf"), &super::selftile::tile(), super::selftile::MARKER, false);
    }
    if preset != Preset::TvPc {
        let mount = dir.join("windows");
        fake::windows_drive(&mount);
        ui.windows_mount = Some(mount);
    }
    ui.agent_on = Some(true);
    agent_files(&ui, preset, now);
    ui.absorb();
    for (ago, text) in [(86_400 * 2, "Tune-up: 3 settings turned on"), (3600 * 5, if preset == HANDHELD { "Charge limit set to 80%" } else { "Sound goes to Headset" }), (3600 * 2, "Heroic Games Launcher installed"), (1500, "Checked 10 games against the ratings"), (240, "Xbox Cloud added to the library")] {
        super::note_activity(&ui.folder(), now - ago, text);
    }
    ui
}

impl Ui {
    /// A sample run on a machine where the launch tool was never set up.
    pub fn sample_without_tool(&mut self) {
        let _ = fs::remove_dir_all(crate::compat_tool::tool_dir(&self.ctx.home.join(".local/share/Steam")));
    }
    /// A sample run where NonSteamLaunchers was started by Turbine Hub at the newest release, with Battle.net in its
    /// shared Windows files and the EA app in its own (the places NonSteamLaunchers.sh looks).
    pub fn sample_with_nsl(&mut self) {
        let cd = self.ctx.home.join(".local/share/Steam/steamapps/compatdata");
        w(&cd, "NonSteamLaunchers/pfx/drive_c/Program Files (x86)/Battle.net/Battle.net Launcher.exe", "MZ");
        let _ = fs::create_dir_all(cd.join("TheEAappLauncher/pfx/drive_c/Program Files/Electronic Arts/EA Desktop/13.1"));
        let _ = crate::nsl::remember(&self.folder(), NSL_TAG);
    }
    /// A sample run with the lights in one of their states: "allyhub" (Ally Hub's agent is setting them too),
    /// "huesync" (the plugin is installed and Decky Loader runs), "closed" (set up before, closed again), "stuck"
    /// (opening them again didn't hold), "open" (set up and open, ready for the light test).
    pub fn sample_lights(&mut self, state: &str) {
        use super::lighting::{AllyHub, HostLights, Lighting};
        use std::os::unix::fs::PermissionsExt;
        let m = self.machine();
        let folder = self.folder();
        Lighting::load(&folder).save(&folder);
        let rule = turbinehub_helper::policy::Access::Lighting.rule().to_string();
        let mut host = HostLights { asked: true, rule: Some(rule), reapply: "enabled".into(), ..HostLights::default() };
        let setup = |ui: &Ui| {
            let mut l = ui.ledger();
            l.record(Change::Set(crate::modules::lighting_access()), Vec::new(), crate::now() - 3600, "3.9.5");
            w(&ui.ctx.root, crate::helper::LEDGER, &l.to_text());
        };
        let files = |ui: &Ui, mode: u32| {
            for led in &m.rgb_leds {
                for f in ["multi_intensity", "brightness"] {
                    let _ = fs::set_permissions(ui.ctx.root.join("sys/class/leds").join(led).join(f), fs::Permissions::from_mode(mode));
                }
            }
        };
        match state {
            "allyhub" => {
                host.allyhub = Some(AllyHub { controller: "allyhub".into(), hid_method: "m6".into(), encoding: "hid".into() });
                host.allyhub_agent = true;
                setup(self);
                files(self, 0o666);
            }
            "huesync" => {
                w(&self.ctx.home, "homebrew/plugins/HueSync/plugin.json", "{\"name\":\"HueSync\"}");
                host.decky_running = true;
                setup(self);
                files(self, 0o666);
            }
            "closed" | "stuck" => {
                setup(self);
                files(self, 0o644);
                if state == "stuck" {
                    let need: Vec<String> = crate::lights::driven(&self.ctx.root, &m.rgb_leds, "").iter().flat_map(crate::lights::needs).collect();
                    let _ = super::lighting::check_open(&self.ctx.root, &folder, &need, false);
                }
            }
            "open" => {
                setup(self);
                files(self, 0o666);
            }
            _ => {}
        }
        self.host_lights = host;
    }
    /// A sample run as if the internet were gone: the lists were fetched three days ago.
    pub fn sample_offline(&mut self) {
        self.offline = true;
        cache(&self.cache, crate::now(), true);
    }
}
