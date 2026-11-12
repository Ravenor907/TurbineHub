//! Steam's shortcuts.vdf (binary VDF), finding the player's browser among non-Steam games, and the rungameid link.

use std::path::Path;
use turbinehub_engine::shortcuts::{self, Shortcut};
use turbinehub_engine::vdf::{self, Vdf};

/// Builds binary VDF the way Steam writes it.
#[derive(Default)]
struct Bin(Vec<u8>);

impl Bin {
    fn key(&mut self, t: u8, k: &str) -> &mut Self {
        self.0.push(t);
        self.0.extend_from_slice(k.as_bytes());
        self.0.push(0);
        self
    }
    fn open(&mut self, k: &str) -> &mut Self {
        self.key(0, k)
    }
    fn s(&mut self, k: &str, v: &str) -> &mut Self {
        self.key(1, k);
        self.0.extend_from_slice(v.as_bytes());
        self.0.push(0);
        self
    }
    fn int(&mut self, k: &str, v: u32) -> &mut Self {
        self.key(2, k);
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn close(&mut self) -> &mut Self {
        self.0.push(8);
        self
    }
}

/// One shortcut entry as Steam writes it (appid None leaves the field out, as very old files do).
fn entry(b: &mut Bin, index: &str, appid: Option<u32>, name: &str, exe: &str, opts: &str) {
    b.open(index);
    if let Some(a) = appid {
        b.int("appid", a);
    }
    b.s("AppName", name).s("Exe", exe).s("StartDir", "\"/usr/bin/\"").s("icon", "").s("ShortcutPath", "");
    b.s("LaunchOptions", opts).int("IsHidden", 0).int("AllowDesktopConfig", 1).int("LastPlayTime", 1_730_000_000);
    b.open("tags").s("0", "Browsers").close();
    b.close();
}

const CHROME_OPTS: &str = "run --branch=stable --arch=x86_64 --command=/app/bin/chrome --file-forwarding com.google.Chrome @@u @@";

fn sample() -> Vec<u8> {
    let mut b = Bin::default();
    b.open("shortcuts");
    entry(&mut b, "0", Some(0x8123_4567), "Heroic Games Launcher", "\"/usr/bin/flatpak\"", "run com.heroicgameslauncher.hgl");
    entry(&mut b, "1", Some(0xC0DE_0001), "Firefox", "\"/usr/bin/flatpak\"", "run org.mozilla.firefox @@u @@");
    entry(&mut b, "2", Some(0x9000_0042), "Google Chrome", "\"/usr/bin/flatpak\"", CHROME_OPTS);
    b.close().close();
    b.0
}

#[test]
fn reads_binary_vdf() {
    let v = vdf::parse_binary(&sample()).unwrap();
    let list = v.get("shortcuts").unwrap().entries();
    assert_eq!(list.len(), 3);
    assert_eq!(list[2].1.str_at(&["appname"]), Some("Google Chrome"));
    // 32-bit numbers read as unsigned, so the top bit doesn't make them negative.
    assert_eq!(list[2].1.str_at(&["appid"]), Some(&*0x9000_0042u32.to_string()));
    assert_eq!(list[0].1.str_at(&["tags", "0"]), Some("Browsers"));
    assert!(matches!(list[0].1.get("tags"), Some(Vdf::Obj(_))));
}

#[test]
fn reads_other_value_types() {
    let mut b = Bin::default();
    b.key(7, "big");
    b.0.extend_from_slice(&u64::MAX.to_le_bytes());
    b.key(3, "f");
    b.0.extend_from_slice(&1.5f32.to_le_bytes());
    b.key(5, "w");
    for u in "Hé".encode_utf16() {
        b.0.extend_from_slice(&u.to_le_bytes());
    }
    b.0.extend_from_slice(&[0, 0]);
    b.close();
    let v = vdf::parse_binary(&b.0).unwrap();
    assert_eq!(v.str_at(&["big"]), Some(&*u64::MAX.to_string()));
    assert_eq!(v.str_at(&["f"]), Some("1.5"));
    assert_eq!(v.str_at(&["w"]), Some("Hé"));
}

#[test]
fn broken_binary_files_are_errors_not_panics() {
    let good = sample();
    // Every cut of a good file either parses or reports an error; none panics.
    for n in 0..good.len() {
        let _ = vdf::parse_binary(&good[..n]);
    }
    assert!(vdf::parse_binary(&[0x00, b'a', 0]).is_err(), "object never closed");
    assert!(vdf::parse_binary(&[0x01, b'a', 0, b'x']).is_err(), "text never ended");
    assert!(vdf::parse_binary(&[0x02, b'a', 0, 1, 2]).is_err(), "number cut short");
    assert!(vdf::parse_binary(&[0x42, b'a', 0]).is_err(), "unknown type");
    // Deep nesting stops at the limit.
    let mut deep = Vec::new();
    for _ in 0..(vdf::MAX_DEPTH + 5) {
        deep.extend_from_slice(&[0x00, b'a', 0]);
    }
    assert!(vdf::parse_binary(&deep).is_err());
    assert_eq!(vdf::parse_binary(&[]).unwrap(), Vdf::Obj(vec![]));
}

#[test]
fn shortcut_ids_use_the_standard_crc32() {
    // Steam's shortcut ids use the same CRC-32 as zip: the standard check values, and an id worked out separately.
    assert_eq!(shortcuts::legacy_appid("\"C:\\Games\\game.exe\"", "Game"), 3_806_792_068);
    assert_eq!(shortcuts::legacy_appid("", "123456789"), 0xCBF4_3926 | 0x8000_0000);
    assert_eq!(turbinehub_engine::zip::crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(turbinehub_engine::zip::crc32(b""), 0);
    assert_eq!(turbinehub_engine::zip::crc32(b"The quick brown fox jumps over the lazy dog"), 0x414F_A339);
}

#[test]
fn shortcut_ids_and_the_rungameid_link() {
    // Steam's rule for old shortcuts: crc32(exe + name) with the top bit set.
    let exe = "\"/usr/bin/flatpak\"";
    let id = shortcuts::legacy_appid(exe, "Google Chrome");
    assert_eq!(id, turbinehub_engine::zip::crc32(b"\"/usr/bin/flatpak\"Google Chrome") | 0x8000_0000);
    assert!(id & 0x8000_0000 != 0);
    // The 64-bit id: appid in the top half, 0x02000000 in the bottom.
    assert_eq!(shortcuts::game_id(0x9000_0042), (0x9000_0042u64 << 32) | 0x0200_0000);
    assert_eq!(shortcuts::game_id(0x9000_0042), 10_376_293_824_963_018_752);
    assert_eq!(shortcuts::rungameid_url(0x9000_0042), "steam://rungameid/10376293824963018752");
    assert_eq!(shortcuts::game_id(u32::MAX), 0xFFFF_FFFF_0200_0000);
}

#[test]
fn reads_shortcuts_and_computes_missing_ids() {
    let list = shortcuts::parse(&sample()).unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[2], Shortcut { appid: 0x9000_0042, name: "Google Chrome".into(), exe: "\"/usr/bin/flatpak\"".into(), launch_options: CHROME_OPTS.into() });
    let mut b = Bin::default();
    b.open("shortcuts");
    entry(&mut b, "0", None, "Chrome", "\"/usr/bin/google-chrome-stable\"", "");
    entry(&mut b, "1", Some(0), "Brave", "\"/usr/bin/flatpak\"", "run com.brave.Browser");
    b.close().close();
    let list = shortcuts::parse(&b.0).unwrap();
    assert_eq!(list[0].appid, shortcuts::legacy_appid("\"/usr/bin/google-chrome-stable\"", "Chrome"));
    assert_eq!(list[1].appid, shortcuts::legacy_appid("\"/usr/bin/flatpak\"", "Brave"));
    assert!(shortcuts::parse(&[0x00, b's']).is_err());
}

fn sc(name: &str, exe: &str, opts: &str) -> Shortcut {
    Shortcut { appid: 0x8000_0001, name: name.into(), exe: exe.into(), launch_options: opts.into() }
}

#[test]
fn finds_chrome_family_browsers_by_program_not_name() {
    let fp = "\"/usr/bin/flatpak\"";
    assert_eq!(shortcuts::browser_of(&sc("x", fp, CHROME_OPTS)), Some("Google Chrome"));
    assert_eq!(shortcuts::browser_of(&sc("x", "\"/usr/bin/google-chrome-stable\"", "")), Some("Google Chrome"));
    assert_eq!(shortcuts::browser_of(&sc("x", "/opt/google/chrome/chrome", "")), Some("Google Chrome"));
    assert_eq!(shortcuts::browser_of(&sc("x", fp, "run com.brave.Browser")), Some("Brave"));
    assert_eq!(shortcuts::browser_of(&sc("x", fp, "run org.chromium.Chromium")), Some("Chromium"));
    assert_eq!(shortcuts::browser_of(&sc("x", "/usr/bin/chromium", "")), Some("Chromium"));
    assert_eq!(shortcuts::browser_of(&sc("x", fp, "run com.microsoft.Edge")), Some("Microsoft Edge"));
    assert_eq!(shortcuts::browser_of(&sc("x", fp, "run com.vivaldi.Vivaldi")), Some("Vivaldi"));
    // Not Chrome-family, or only named like one.
    assert_eq!(shortcuts::browser_of(&sc("Google Chrome", fp, "run org.mozilla.firefox")), None);
    assert_eq!(shortcuts::browser_of(&sc("Chrome", "\"/home/deck/game.sh\"", "")), None);
    // Chrome first, whatever the file order.
    let list = vec![sc("B", fp, "run com.brave.Browser"), sc("C", fp, CHROME_OPTS), sc("F", fp, "run org.mozilla.firefox")];
    let (b, s) = shortcuts::pick_browser(&list).unwrap();
    assert_eq!((b, s.name.as_str()), ("Google Chrome", "C"));
    assert!(shortcuts::pick_browser(&list[2..]).is_none());
}

fn write(p: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

#[test]
fn finds_the_browser_in_a_home_folder_read_only() {
    let home = std::env::temp_dir().join(format!("turbine-shortcuts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    assert!(shortcuts::find_browser(&home).is_none());
    let steam = home.join(".local/share/Steam");
    std::fs::create_dir_all(steam.join("steamapps")).unwrap();
    // A broken file for another account is skipped.
    write(&steam.join("userdata/111/config/shortcuts.vdf"), &[0x00, b's']);
    assert!(shortcuts::find_browser(&home).is_none());
    let f = steam.join("userdata/222/config/shortcuts.vdf");
    let bytes = sample();
    write(&f, &bytes);
    let found = shortcuts::find_browser(&home).unwrap();
    assert_eq!(found.browser, "Google Chrome");
    assert_eq!(found.appid, 0x9000_0042);
    assert_eq!(found.url(), "steam://rungameid/10376293824963018752");
    assert!(found.to_json().contains("\"gameid\":\"10376293824963018752\""));
    // Read only: the file is unchanged.
    assert_eq!(std::fs::read(&f).unwrap(), bytes);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn finds_its_own_shortcut_to_come_back_to() {
    let home = std::env::temp_dir().join(format!("turbinehub-shortcuts-self-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    assert!(shortcuts::find_self(&home).is_none());
    let mut b = Bin::default();
    b.open("shortcuts");
    // A shortcut merely named Turbine Hub isn't it; the Flatpak's id in the launch options is.
    entry(&mut b, "0", Some(0x8000_0001), "Turbine Hub", "\"/usr/bin/true\"", "");
    entry(&mut b, "1", Some(0x8765_4321), "Mods", "\"/usr/bin/flatpak\"", "run --branch=stable --arch=x86_64 --command=turbinehub io.github.ravenor907.TurbineHub");
    b.close().close();
    let f = home.join(".local/share/Steam/userdata/333/config/shortcuts.vdf");
    write(&f, &b.0);
    std::fs::create_dir_all(home.join(".local/share/Steam/steamapps")).unwrap();
    let (appid, name) = shortcuts::find_self(&home).unwrap();
    assert_eq!((appid, name.as_str()), (0x8765_4321, "Mods"));
    assert_eq!(shortcuts::rungameid_url(appid), format!("steam://rungameid/{}", shortcuts::game_id(appid)));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_game_id_steam_set_is_checked() {
    assert_eq!(shortcuts::rungameid_from_env("10376293824963018752").as_deref(), Some("steam://rungameid/10376293824963018752"));
    assert_eq!(shortcuts::rungameid_from_env(" 489830 ").as_deref(), Some("steam://rungameid/489830"));
    for bad in ["", "0", "12a", "steam://x", "123456789012345678901", "-1"] {
        assert_eq!(shortcuts::rungameid_from_env(bad), None, "{bad}");
    }
}
