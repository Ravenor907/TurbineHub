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
    ("library", "--page=library --machine=handheld --mode=guided"),
    ("library-offline", "--page=library --machine=handheld --mode=guided --offline"),
    ("store", "--page=store --arg=apps --machine=handheld --mode=guided"),
    ("store-offline", "--page=store --arg=apps --machine=handheld --mode=guided --offline"),
    ("store-decky", "--page=store --arg=decky --machine=handheld --mode=guided"),
    ("store-tv", "--page=store --arg=tv --machine=tv --mode=guided"),
    ("machine", "--page=machine --machine=handheld --mode=guided"),
    ("lighting", "--page=module:lighting --machine=handheld --mode=guided"),
    ("power", "--page=module:power --machine=handheld --mode=veteran"),
    ("tuneup", "--page=module:tuneup --machine=handheld --mode=guided"),
    ("sleep", "--page=module:sleep --machine=handheld --mode=guided"),
    ("dialog", "--page=machine --act=tuneup:all --machine=handheld --mode=guided"),
    ("dialog-guide", "--page=home --act=checkup:help:hdmi --machine=tv --mode=guided"),
    ("menu", "--page=home --act=menu: --machine=handheld --mode=guided"),
    ("settings", "--page=settings --machine=handheld --mode=guided"),
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
}

fn sensors(root: &Path, preset: Preset) -> Readings {
    match preset {
        HANDHELD => Readings { cpu_c: Some(58.0), gpu_c: Some(55.0), gpu_w: Some(12.0), battery_pct: Some(78), battery_w: Some(14.2), charging: false, fan_rpm: Some(3100) },
        Preset::Desktop => Readings { cpu_c: Some(41.0), gpu_c: Some(38.0), gpu_w: Some(34.0), battery_pct: None, battery_w: None, charging: false, fan_rpm: Some(880) },
        Preset::TvPc => {
            let _ = root;
            Readings { cpu_c: Some(44.0), gpu_c: Some(48.0), gpu_w: Some(62.0), battery_pct: None, battery_w: None, charging: false, fan_rpm: Some(940) }
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
    ui.updates = vec![super::store::HEROIC.into()];
    ui.decky_tag = Some("v3.1.10".into());
    ui.free_bytes = Some(640_000_000_000);
    ui.sound = vec![("speakers".into(), "Speakers".into()), ("headset".into(), "Headset".into()), ("hdmi".into(), "TV over HDMI".into())];
    ui.sound_now = "headset".into();
    ui.facts = match preset {
        HANDHELD => Facts { updates: vec!["Heroic Games Launcher".into()], last_sleep: Some((6.0, vec!["A USB device".into()])), has_password: Some(true), windows_not_brought: true, ..Facts::default() },
        Preset::Desktop => Facts { current_hz: Some(60), updates: vec!["Heroic Games Launcher".into()], has_password: Some(true), windows_not_brought: true, ..Facts::default() },
        Preset::TvPc => Facts { updates: vec!["Heroic Games Launcher".into()], has_password: Some(true), ..Facts::default() },
    };
    if preset != Preset::TvPc {
        let mount = dir.join("windows");
        fake::windows_drive(&mount);
        ui.windows_mount = Some(mount);
    }
    for (ago, text) in [(86_400 * 2, "Tune-up: 3 settings turned on"), (3600 * 5, if preset == HANDHELD { "Charge limit set to 80%" } else { "Sound goes to Headset" }), (3600 * 2, "Heroic Games Launcher installed"), (1500, "Checked 10 games against the ratings"), (240, "Xbox Cloud added to the library")] {
        super::note_activity(&ui.folder(), now - ago, text);
    }
    ui
}

impl Ui {
    /// A sample run as if the internet were gone: the lists were fetched three days ago.
    pub fn sample_offline(&mut self) {
        self.offline = true;
        cache(&self.cache, crate::now(), true);
    }
}
