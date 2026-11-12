//! The build's stand-in data and fake home folder are read by the engine's own code here, so the checks that run
//! against them see what a player's machine would give.

mod support;

use std::path::Path;
use support::TempDir;
use turbinehub_engine::json::{self, Json};
use turbinehub_engine::library::{self, AntiCheat, ValveRating};
use turbinehub_engine::{Store, migrate, scan_games};

fn data(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/stand-in-data").join(name)).unwrap()
}

#[test]
fn the_stand_in_answers_in_the_shape_the_engine_reads() {
    let reports = json::parse(&data("valve.json")).unwrap();
    let rating = |id: &str| library::parse_valve_report(&reports.get(id).map(Json::to_text).unwrap_or_default());
    assert_eq!(rating("1245620"), ValveRating::Verified);
    assert_eq!(rating("1145360"), ValveRating::Verified);
    assert_eq!(rating("1172470"), ValveRating::Playable);
    assert_eq!(rating("3999903"), ValveRating::Unsupported);
    assert_eq!(rating("892970"), ValveRating::Unknown);

    let list = library::parse_anticheat(&data("anticheat.json"));
    assert_eq!(list, vec![("1172470".to_string(), AntiCheat::Supported), ("1245620".to_string(), AntiCheat::Running), ("3999903".to_string(), AntiCheat::Blocked)]);

    let fixes = library::parse_fixes(&data("fixes.json"));
    assert_eq!(fixes.iter().map(|(id, f)| (id.as_str(), f.kind.as_str())).collect::<Vec<_>>(), vec![("1245620", "switch"), ("1086940", "proton"), ("632470", "note")]);
    assert!(fixes.iter().all(|(_, f)| !f.words.is_empty()));
}

/// Runs tools/fake-home.py. None where there is no python3 (the test then has nothing to check).
fn fake_home(dir: &TempDir, allyhub: bool) -> Option<std::path::PathBuf> {
    let home = dir.path("home");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/fake-home.py");
    let mut cmd = std::process::Command::new("python3");
    cmd.arg(script).arg(&home);
    if allyhub {
        cmd.arg("--allyhub");
    }
    match cmd.output() {
        Ok(o) => {
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            Some(home)
        }
        Err(_) => None,
    }
}

#[test]
fn the_fake_home_is_a_home_the_engine_reads() {
    let dir = TempDir::new("fakehome");
    let Some(home) = fake_home(&dir, true) else { return };
    let scan = scan_games(&home);
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    let names: Vec<&str> = scan.games.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, vec!["Alan Wake 2", "Apex Legends", "Baldur's Gate 3", "Disco Elysium", "ELDEN RING", "Hades II", "Stardew Valley", "Valheim"]);
    assert_eq!(scan.games.iter().filter(|g| g.store == Store::Steam).count(), 6);

    let found = migrate::detect(&dir.path("no-root"), &home);
    assert!(found.app && found.plugin && found.config.is_some() && found.system_files.is_empty());
    let options = migrate::game_options(&home, &scan);
    assert_eq!(options.len(), 1);
    assert_eq!((options[0].name.as_str(), options[0].kept.as_str(), options[0].switches.len()), ("ELDEN RING", "", 2));
    let groups = migrate::groups(&found, &dir.path("no-root"), &options);
    assert_eq!(groups.iter().map(|g| g.id).collect::<Vec<_>>(), vec!["games", "panel"]);
    let steps = migrate::plan(&found, &groups, &[]);
    assert!(steps.contains(&migrate::Step::RemovePlugin));
    assert!(!steps.iter().any(|s| matches!(s, migrate::Step::RemoveUserPath(p) if p.contains("Backups"))));
}

#[test]
fn a_fake_home_without_allyhub_has_nothing_to_move() {
    let dir = TempDir::new("fakehome-plain");
    let Some(home) = fake_home(&dir, false) else { return };
    assert!(!migrate::detect(&dir.path("no-root"), &home).any());
}
