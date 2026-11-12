//! Bring it over: reading a Windows drive (read only) and the checkup's rules.

mod support;

use turbinehub_engine::checkup::{self, Facts, Level};
use turbinehub_engine::fake::{self, Preset};
use turbinehub_engine::hardware::Machine;
use turbinehub_engine::bring;

fn snapshot(dir: &std::path::Path) -> Vec<(String, u64)> {
    let mut v = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let m = e.metadata().unwrap();
            if m.is_dir() {
                stack.push(e.path());
            } else {
                v.push((e.path().to_string_lossy().into_owned(), m.len()));
            }
        }
    }
    v.sort();
    v
}

#[test]
fn reads_steam_games_and_apps_from_a_windows_drive_without_changing_it() {
    let mount = support::scratch("windrive");
    fake::windows_drive(&mount);
    let before = snapshot(&mount);
    let games = bring::steam_games(&mount);
    let names: Vec<&str> = games.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["Baldur's Gate 3", "Disco Elysium", "Hades II"], "half-downloaded and escaping folders are left out");
    assert_eq!(games[0].bytes, 120_000_000_000);
    let plan = bring::copy_plan(&games[0], std::path::Path::new("/home/deck/.local/share/Steam/steamapps"));
    assert_eq!(plan[0].1, std::path::PathBuf::from("/home/deck/.local/share/Steam/steamapps/common/Baldurs Gate 3"));
    assert_eq!(plan[1].1, std::path::PathBuf::from("/home/deck/.local/share/Steam/steamapps/appmanifest_1086940.acf"));
    assert_eq!(bring::windows_users(&mount), vec![mount.join("Users/Sam")]);
    assert_eq!(bring::apps_found(&mount), ["discord", "afterburner", "spotify", "battlenet"]);
    assert_eq!(snapshot(&mount), before);
    let _ = std::fs::remove_dir_all(&mount);
}

fn read(preset: Preset, tag: &str) -> Machine {
    let root = support::scratch(&format!("chk-{tag}"));
    fake::machine(&root, preset);
    let m = Machine::read(&root, "", fake::HOME_DEV);
    let _ = std::fs::remove_dir_all(&root);
    m
}

#[test]
fn the_checkup_lists_problems_first_with_their_fix() {
    let pc = read(Preset::Desktop, "pc");
    let f = Facts { current_hz: Some(60), updates: vec!["Discord".into(), "Heroic".into(), "OBS Studio".into()], windows_not_brought: true, ..Facts::default() };
    let items = checkup::items(&pc, &f);
    let ids: Vec<&str> = items.iter().map(|i| i.id).collect();
    assert_eq!(ids, ["refresh", "drive", "updates", "windows"]);
    assert_eq!(items[0].title, "Your screen runs at 60 Hz");
    assert_eq!(items[0].line, "It supports 240 Hz. Games feel smoother at 240 Hz.");
    assert_eq!(items[1].line, "Samsung SSD 990 PRO 2TB, empty. Add it to Steam as a game library?");
    assert_eq!(items[2].title, "3 updates ready");
    assert_eq!(checkup::headline(&items).0, "Three things could use a look");
    assert!(checkup::items(&pc, &Facts { current_hz: Some(240), ..Facts::default() }).iter().all(|i| i.id != "refresh"));

    let tv = read(Preset::TvPc, "tv");
    let ids: Vec<&str> = checkup::items(&tv, &Facts::default()).iter().map(|i| i.id).collect();
    assert!(ids.contains(&"hdmi") && ids.contains(&"wake"), "{ids:?}");

    let ally = read(Preset::AllyX, "ally");
    let f = Facts { last_sleep: Some((6.0, vec!["A USB dongle".into()])), has_password: Some(false), ..Facts::default() };
    let items = checkup::items(&ally, &f);
    assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), ["password", "sleep", "charge"]);
    assert_eq!(items[1].line, "A USB dongle woke the machine. Stop it from waking it?");
    assert_eq!(items[2].level, Level::Tip);
    assert_eq!(checkup::headline(&[]).0, "Everything is running fine");
}
