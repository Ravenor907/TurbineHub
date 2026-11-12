use super::*;
use crate::fake;
use crate::files::Space;
use crate::files::testing::{self, NOW, Temp};
use crate::ui::Step;
use std::cell::RefCell;

const BG3: &str = "1086940";
const HADES: &str = "1145360";
const STEAM: &str = "/home/deck/.local/share/Steam";

/// The made-up handheld with the storage saver's leftovers on it.
struct Made {
    t: Temp,
    disk: Disk,
    places: Places,
    folder: PathBuf,
    data: PathBuf,
}

impl Made {
    fn new(tag: &str) -> Made {
        let (t, disk, _) = testing::machine(&format!("storage-{tag}"));
        fake::storage(&t.0, &t.at(fake::FILES_HOME));
        let (folder, data) = (t.at("/config/turbinehub"), t.at("/data/turbinehub"));
        // Turbine Hub remembers Hades II from when it was played.
        crate::history::add_session(&data, &crate::history::Session { game: format!("steam:{HADES}"), name: "Hades II".into(), start: 100, end: 200, asleep: 0, before: None, after: None, on_battery: false });
        let places = testing::places(&t, &disk);
        Made { t, disk, places, folder, data }
    }
    fn reread(&mut self) {
        self.places = testing::places(&self.t, &self.disk);
    }
    fn site(&self) -> Site<'_> {
        Site { fs: &self.disk, places: &self.places, folder: &self.folder, data: &self.data, when: NOW }
    }
    fn limits() -> Limits {
        Limits { downloads_bytes: 10_000, ..Limits::default() }
    }
    fn scan(&self) -> Report {
        scan(&self.site(), &Host::default(), &Made::limits())
    }
    fn run(&self, job: &Job, steam: Steam, games: &str) -> Result<String, String> {
        let said = RefCell::new(Vec::new());
        let mut ask = host(steam, games);
        run(job, &self.site(), &|w| said.borrow_mut().push(w), &mut ask)
    }
}

/// A host that answers the two questions a cleaning asks.
fn host(steam: Steam, games: &str) -> impl FnMut(&[String]) -> Option<(i32, String)> {
    let games = games.to_string();
    move |line: &[String]| {
        if *line == *Steam::question() {
            match steam {
                Steam::Closed => Some((1, "0\n".into())),
                Steam::Running => Some((0, "3\n".into())),
                Steam::Unknown => None,
            }
        } else if *line == *crate::agent::games_question() {
            Some((if games.is_empty() { 1 } else { 0 }, games.clone()))
        } else {
            None
        }
    }
}

fn closed() -> Host {
    Host { steam: Steam::Closed, running: Some(Vec::new()), ..Host::default() }
}

fn by<'a>(r: &'a Report, kind: Kind, part: &str) -> &'a Finding {
    r.findings.iter().find(|f| f.kind == kind && (f.title.contains(part) || f.id.contains(part))).unwrap_or_else(|| panic!("no {kind:?} with {part} in {:#?}", r.findings.iter().map(|f| (&f.id, &f.title)).collect::<Vec<_>>()))
}

fn ids(r: &Report, kinds: &[Kind]) -> Vec<String> {
    r.findings.iter().filter(|f| kinds.contains(&f.kind) && f.group != Group::Never).map(|f| f.id.clone()).collect()
}

fn clean_job(p: &crate::ui::Plan) -> Job {
    p.steps.iter().find_map(|x| if let Step::Storage(j) = x { Some(j.clone()) } else { None }).unwrap()
}

#[test]
fn every_leftover_lands_in_its_group_and_only_safe_ones_are_ticked() {
    let m = Made::new("groups");
    let r = m.scan();
    assert!(r.problems.is_empty(), "{:?}", r.problems);

    // Safe: what Steam builds again, and plain garbage. Erased for good, ticked.
    let shader = by(&r, Kind::ShaderCache, "Hades II");
    assert_eq!((shader.group, shader.how, shader.ticked, shader.size, shader.steam), (Group::Safe, How::Erase, true, Size::exact(6000), true));
    assert_eq!(shader.title, "Shader cache of Hades II");
    assert_eq!(by(&r, Kind::ShaderCache, "/570").title, "Shader cache of a removed game (app 570)");
    let card = by(&r, Kind::ShaderCache, "632470");
    assert_eq!((card.drive, r.drives[card.drive].label.as_str()), (1, "SD card"));
    let download = by(&r, Kind::Download, "Unfinished download of Hades II");
    assert_eq!((download.group, download.ticked, download.size), (Group::Safe, true, Size::exact(9000)));
    let logs = by(&r, Kind::SteamLogs, "old logs");
    assert_eq!((logs.group, logs.size, logs.paths.len(), logs.steam), (Group::Safe, Size::exact(2200), 2, false));
    assert!(logs.paths.iter().all(|p| !p.ends_with("console_log.txt")), "the log Steam writes now stays");
    assert_eq!(by(&r, Kind::SteamLogs, "crash dumps").paths, vec![PathBuf::from(format!("{STEAM}/dumps/crash_20260901.dmp"))]);

    // Check first: anything that may hold saves or settings. Never ticked, each with its reason.
    let windows = by(&r, Kind::Compatdata, "Hades II");
    assert_eq!((windows.group, windows.how, windows.ticked), (Group::Check, How::Bin, false));
    assert!(windows.why.contains("saves"), "{}", windows.why);
    let reset = by(&r, Kind::ResetBackup, "Cyberpunk 2077");
    assert_eq!((reset.group, reset.how, reset.ticked, reset.size), (Group::Check, How::Bin, false, Size::exact(800)));
    let update = by(&r, Kind::Download, "Unfinished update of Baldur's Gate 3");
    assert_eq!((update.group, update.ticked), (Group::Check, false));
    assert_eq!(by(&r, Kind::FlatpakData, "com.example.OldLauncher").group, Group::Check);
    let file = by(&r, Kind::DownloadsFile, "Old game installer.iso");
    assert_eq!((file.group, file.how, file.ticked, file.size), (Group::Check, How::Bin, false, Size::exact(20_000)));
    assert!(file.why.contains("200 days"), "{}", file.why);
    assert_eq!(by(&r, Kind::RecycleBin, "Recycle Bin").group, Group::Check);
    assert_eq!(by(&r, Kind::RecycleBin, "Recycle Bin on SD card").drive, 1);
    for f in r.group(Group::Check) {
        assert!(!f.ticked && !f.why.is_empty(), "{}", f.title);
    }

    // Protons: what something uses stays and says who; the rest wait for a look.
    let used = by(&r, Kind::Proton, "Proton 11.0");
    assert_eq!((used.group, used.why.as_str()), (Group::Never, "Steam starts games with it when nothing else is set."));
    assert_eq!(by(&r, Kind::Proton, "GE-Proton10-12").why, "Cyberpunk 2077 uses it.");
    let nine = by(&r, Kind::Proton, "Proton 9.0");
    assert_eq!((nine.group, nine.how, nine.steam, nine.paths.len()), (Group::Check, How::Bin, true, 2));
    assert!(nine.paths[1].ends_with("appmanifest_2805730.acf"));
    let ge = by(&r, Kind::Proton, "GE-Proton10-4");
    assert_eq!((ge.group, ge.steam, ge.paths.len()), (Group::Check, false, 1));

    // Nothing of an installed game is a leftover, and neither are small or fresh downloads.
    for f in &r.findings {
        assert!(!f.id.contains(&format!("shadercache/{BG3}")) && !f.id.ends_with(&format!("compatdata/{BG3}")) && !f.id.ends_with("compatdata/1091500"), "{}", f.id);
        assert!(!f.title.contains("Fresh installer") && !f.title.contains("TurbineHub.flatpak") && !f.title.contains("Discord"), "{}", f.title);
    }

    // The ticks before the player touches anything are the Safe group, and the order is Safe, Check, in use.
    let ticks = r.default_ticks();
    assert_eq!(ticks.len(), r.group(Group::Safe).count());
    assert!(r.findings.iter().all(|f| f.ticked == (f.group == Group::Safe)));
    assert!(r.findings.windows(2).all(|w| w[0].group <= w[1].group));
    assert_eq!(r.total(Group::Safe), Size::exact(6000 + 2500 + 4000 + 9000 + 2200 + 2200));
    assert_eq!(r.ticked_total(&ticks), r.total(Group::Safe));
    assert_eq!(r.free_words(&ticks), "Free 26 KB");
    assert_eq!(r.free_words(&[]), "Nothing ticked");
    // Ticking what is in use changes nothing.
    assert_eq!(r.ticked_total(std::slice::from_ref(&used.id)), Size::default());
    assert_eq!(r.ticked_to_bin(&[windows.id.clone(), shader.id.clone()]), windows.size);
}

#[test]
fn games_and_drives_show_what_they_really_take() {
    let m = Made::new("sizes");
    let r = m.scan();
    let bg3 = r.game(&format!("steam:{BG3}")).unwrap();
    assert_eq!((bg3.files, bg3.shaders, bg3.proton, bg3.total(), bg3.drive), (Size::exact(4), Size::exact(7000), Size::exact(6), Size::exact(7010), 0));
    let valheim = r.game("steam:892970").unwrap();
    assert_eq!((valheim.drive, valheim.shaders, valheim.proton), (1, Size::default(), Size::default()));
    assert_eq!(r.games[0].id, format!("steam:{BG3}"), "biggest first");
    assert_eq!(r.drives.iter().map(|d| d.label.as_str()).collect::<Vec<_>>(), ["Internal storage", "SD card"]);
    assert_eq!((r.drives[0].libraries.len(), r.drives[0].libraries[0].games, r.drives[1].libraries[0].games), (1, 2, 1));
    assert_eq!(r.drives[0].percent_full(), Some(55));
    assert_eq!(r.can_free(1).0, Size::exact(4000));

    // One game alone, for the Games tab.
    let games = crate::scan_games(&m.t.at(fake::FILES_HOME)).games;
    let one = game_size(&m.disk, &m.places, games.iter().find(|g| g.store_id == BG3).unwrap(), &Limits::default());
    assert_eq!(one.total(), Size::exact(7010));

    // Cut short by the number of entries or by the clock: the size says "at least", never less than was counted.
    let few = scan(&m.site(), &Host::default(), &Limits { entries: 2, ..Made::limits() });
    let cut = by(&few, Kind::Compatdata, "Hades II");
    assert!(cut.size.at_least && cut.size.bytes <= 1209, "{:?}", cut.size);
    assert!(few.total(Group::Check).at_least);
    let none = scan(&m.site(), &Host::default(), &Limits { time: Duration::ZERO, ..Made::limits() });
    assert_eq!(by(&none, Kind::ShaderCache, "Hades II").size, Size { bytes: 0, at_least: true });
    assert_eq!(Size { bytes: 0, at_least: true }.words(), "size not known");
    assert_eq!(Size { bytes: 3 << 30, at_least: true }.words(), "at least 3.0 GB");
    assert!(none.free_words(&none.default_ticks()).starts_with("Free"));
}

#[test]
fn the_plan_names_every_thing_and_says_what_is_for_good() {
    let m = Made::new("plan");
    let r = m.scan();
    assert_eq!(plan(&r, &[], &closed()).unwrap_err(), "Nothing is ticked. Tick what should go first.");
    let used = by(&r, Kind::Proton, "Proton 11.0").id.clone();
    assert!(plan(&r, &[used], &closed()).is_err(), "what is in use is never in a plan");

    let p = plan(&r, &r.default_ticks(), &closed()).unwrap();
    assert_eq!((p.title.as_str(), p.confirm.as_str(), p.danger), ("Free up space", "Free 26 KB", false));
    assert!(p.text.starts_with("26 KB is erased for good."), "{}", p.text);
    assert!(p.text.contains("Internal storage has 214 GB free now, and about 214 GB after."), "{}", p.text);
    assert!(p.items.iter().any(|(t, l)| t == "Shader cache of Hades II (6 KB)" && l == "Erased for good."), "{:?}", p.items);
    let Job::Clean(items) = clean_job(&p) else { panic!() };
    assert_eq!((p.steps.len(), items.len()), (1, r.default_ticks().len()));
    assert!(items.iter().filter(|i| i.kind == Kind::ShaderCache).all(|i| i.orphan && i.steam));

    // Check first things go to the Recycle Bin and the plan says so; emptying a bin is for good and is marked.
    let windows = by(&r, Kind::Compatdata, "Hades II").id.clone();
    let p = plan(&r, std::slice::from_ref(&windows), &closed()).unwrap();
    assert!(!p.danger && p.text.contains("goes to the Recycle Bin, where Undo can put it back"), "{}", p.text);
    assert!(p.items[0].1.starts_with("Goes to the Recycle Bin"));
    let bin = by(&r, Kind::RecycleBin, "Recycle Bin on SD card").id.clone();
    assert!(plan(&r, std::slice::from_ref(&bin), &closed()).unwrap().danger);

    // Steam running: what a game that is still installed owns inside the libraries is refused (a reset backup,
    // an unfinished update, one of Steam's Protons); the rest still goes.
    let running = Host { steam: Steam::Running, running: Some(Vec::new()), ..Host::default() };
    let reset = by(&r, Kind::ResetBackup, "Cyberpunk").id.clone();
    let update = by(&r, Kind::Download, "Unfinished update of Baldur's Gate 3").id.clone();
    let nine = by(&r, Kind::Proton, "Proton 9.0").id.clone();
    for id in [&reset, &update, &nine] {
        assert_eq!(plan(&r, std::slice::from_ref(id), &running).unwrap_err(), "Close Steam first: Steam uses its library folders while it runs.", "{id}");
    }
    assert!(plan(&r, std::slice::from_ref(&windows), &Host::default()).unwrap_err().starts_with("Turbine Hub couldn't check whether Steam is running"));
    // What games that are gone left behind goes while Steam runs: in Game Mode it always runs.
    let p = plan(&r, std::slice::from_ref(&windows), &running).unwrap();
    assert!(p.items[0].0.starts_with("Windows files of Hades II") && !p.text.contains("left alone"), "{:?} {}", p.items, p.text);
    let mut ticked = r.default_ticks();
    ticked.extend([reset.clone(), update, nine]);
    let p = plan(&r, &ticked, &running).unwrap();
    let Job::Clean(items) = clean_job(&p) else { panic!() };
    assert_eq!(items.len(), r.default_ticks().len());
    assert_eq!(items.iter().filter(|i| i.steam).map(|i| (i.kind, i.orphan)).collect::<Vec<_>>(), [(Kind::Download, true), (Kind::ShaderCache, true), (Kind::ShaderCache, true), (Kind::ShaderCache, true)]);
    assert!(p.text.contains("3 things are left alone. Close Steam first"), "{}", p.text);
    assert!(p.items.iter().any(|(t, l)| t == "Left alone: Proton 9.0" && l.starts_with("Close Steam first")), "{:?}", p.items);
    // Which games run couldn't be asked: a leftover might belong to one of them, so it waits.
    let unasked = Host { steam: Steam::Running, running: None, ..Host::default() };
    assert_eq!(plan(&r, std::slice::from_ref(&windows), &unasked).unwrap_err(), "Turbine Hub couldn't check whether the game is running, so it left this alone.");
    // A game that runs keeps its own things, whatever else is known.
    let playing = Host { steam: Steam::Running, running: Some(vec!["1091500".into(), HADES.into()]), ..Host::default() };
    assert_eq!(plan(&r, &[reset], &playing).unwrap_err(), "The game is running. Close it first.");
    assert_eq!(plan(&r, &[windows], &playing).unwrap_err(), "The game is running. Close it first.");
}

#[test]
fn a_cleaning_erases_what_rebuilds_and_bins_the_rest_and_undo_brings_it_back() {
    let m = Made::new("clean");
    let r = m.scan();
    let mut ticked = r.default_ticks();
    ticked.extend(ids(&r, &[Kind::Compatdata, Kind::ResetBackup, Kind::Proton, Kind::FlatpakData, Kind::DownloadsFile]));
    let p = plan(&r, &ticked, &closed()).unwrap();
    assert_eq!(m.run(&clean_job(&p), Steam::Closed, "").unwrap(), format!("{} freed.", ticked.len()));

    for gone in ["steamapps/shadercache/1145360", "steamapps/shadercache/570", "steamapps/downloading/1145360", "logs/console_log.previous.txt", "logs/shader_log.txt", "dumps/crash_20260901.dmp", "steamapps/compatdata/1145360", "steamapps/compatdata/1091500/pfx.turbinehub-backup-1794000000", "steamapps/common/Proton 9.0", "steamapps/appmanifest_2805730.acf", "compatibilitytools.d/GE-Proton10-4"] {
        assert!(!m.t.has(&format!("{STEAM}/{gone}")), "{gone}");
    }
    assert!(!m.t.has("/run/media/deck/sdcard/steamapps/shadercache/632470"));
    assert!(!m.t.has("/home/deck/.var/app/com.example.OldLauncher") && !m.t.has("/home/deck/Downloads/Old game installer.iso"));
    // What is in use, what a game needs and what wasn't offered is all still there.
    for kept in ["steamapps/shadercache/1086940", "steamapps/compatdata/1086940/pfx", "steamapps/compatdata/1091500/pfx", "steamapps/downloading/1086940", "steamapps/common/Proton 11.0/proton", "compatibilitytools.d/GE-Proton10-12/proton", "logs/console_log.txt", "dumps/crash_now.dmp", "steamapps/shadercache", "steamapps/compatdata", "steamapps/downloading"] {
        assert!(m.t.has(&format!("{STEAM}/{kept}")), "{kept}");
    }
    // The Check first things are in the Recycle Bin, with the save still in them.
    assert!(m.t.has("/home/deck/.local/share/Trash/files/1145360/pfx/drive_c/users/steamuser/Saved Games/Hades II/Profile1.sav"));
    assert!(m.t.read("/home/deck/.local/share/Trash/info/1145360.trashinfo").contains("steamapps/compatdata/1145360"));

    // The note tells the room each drive really had and has; here the made-up disk reports a little more after.
    let after = Disk { room: Some(Space { free: 229_780_000_000 + (5 << 30), total: 512_000_000_000 }), ..m.disk.clone() };
    let note = done_words(&p.steps, &m.folder, &after).unwrap();
    assert!(note.starts_with("Freed 10.0 GB. Internal storage had 214 GB free and has 219 GB now."), "{note}");
    assert!(note.contains("7 things wait in the Recycle Bin: Undo puts them back"), "{note}");
    assert_eq!(done_words(&[Step::SaveSetup], &m.folder, &after), None);
    assert!(done_words(&p.steps, &m.folder, &m.disk).unwrap().starts_with("Moved to the Recycle Bin."));

    // A second look finds none of it again, and the bin's finding has grown.
    let again = m.scan();
    assert_eq!(again.total(Group::Safe), Size::default());
    assert!(again.findings.iter().all(|f| f.kind != Kind::Compatdata && f.kind != Kind::ResetBackup));

    // Undo: asked like every change inside Steam's folders, then everything is back where it was.
    let undo = undo_plan(&m.disk, &m.folder).unwrap();
    assert_eq!((undo.steps.clone(), undo.confirm.as_str()), (vec![Step::Storage(Job::Undo)], "Put back"));
    assert!(undo.items.iter().any(|(t, l)| t == "Windows files of Hades II" && l.ends_with("steamapps/compatdata/1145360")), "{:?}", undo.items);
    // With Steam running, what a removed game left behind goes back (the game is still gone); what belongs to an
    // installed game or to Steam itself waits for Steam to be closed.
    assert_eq!(m.run(&Job::Undo, Steam::Running, "").unwrap(), "4 put back. 3 still in the Recycle Bin: Close Steam first: Steam uses its library folders while it runs.");
    assert!(m.t.has(&format!("{STEAM}/steamapps/compatdata/1145360/pfx")) && m.t.has(&format!("{STEAM}/compatibilitytools.d/GE-Proton10-4/proton")));
    assert!(!m.t.has(&format!("{STEAM}/steamapps/common/Proton 9.0")) && !m.t.has(&format!("{STEAM}/steamapps/compatdata/1091500/pfx.turbinehub-backup-1794000000")));
    assert_eq!(m.run(&Job::Undo, Steam::Closed, "").unwrap(), "3 put back.");
    for back in ["steamapps/compatdata/1145360/pfx/drive_c/users/steamuser/Saved Games/Hades II/Profile1.sav", "steamapps/compatdata/1091500/pfx.turbinehub-backup-1794000000", "steamapps/common/Proton 9.0/proton", "steamapps/appmanifest_2805730.acf", "compatibilitytools.d/GE-Proton10-4/proton"] {
        assert!(m.t.has(&format!("{STEAM}/{back}")), "{back}");
    }
    assert!(m.t.has("/home/deck/.var/app/com.example.OldLauncher/data/games/save.dat") && m.t.has("/home/deck/Downloads/Old game installer.iso"));
    assert!(undo_plan(&m.disk, &m.folder).is_none());
    assert_eq!(m.run(&Job::Undo, Steam::Closed, "").unwrap_err(), "There is nothing to put back.");
}

#[test]
fn steam_is_asked_again_right_before_anything_inside_its_folders_goes() {
    let m = Made::new("steam");
    let r = m.scan();
    // The plan was made while Steam was closed; it was started since. The Safe things, and three things of
    // installed games and of Steam itself.
    let mut ticked = r.default_ticks();
    ticked.extend(ids(&r, &[Kind::ResetBackup]));
    ticked.push(by(&r, Kind::Download, "Unfinished update").id.clone());
    ticked.push(by(&r, Kind::Proton, "Proton 9.0").id.clone());
    let p = plan(&r, &ticked, &closed()).unwrap();
    let kept = ["steamapps/downloading/1086940", "steamapps/common/Proton 9.0/proton", "steamapps/compatdata/1091500/pfx.turbinehub-backup-1794000000"];
    let orphans = ["steamapps/shadercache/1145360", "steamapps/shadercache/570", "steamapps/downloading/1145360"];
    // Whether Steam runs couldn't be asked: everything inside its libraries stays.
    assert_eq!(m.run(&clean_job(&p), Steam::Unknown, "").unwrap(), "2 freed, 7 left alone.");
    for there in kept.iter().chain(&orphans) {
        assert!(m.t.has(&format!("{STEAM}/{there}")), "{there}");
    }
    let note = done_words(&p.steps, &m.folder, &m.disk).unwrap_or_default();
    assert!(note.contains("Left alone: ") && note.contains("(and 6 more)") && note.contains("couldn't check whether Steam is running"), "{note}");
    fake::storage(&m.t.0, &m.t.at(fake::FILES_HOME));
    // Steam runs, and so does the game with app id 570: its leftover waits, the other removed games' leftovers
    // go, and what installed games and Steam itself own stays until Steam is closed.
    let playing = "123 /home/deck/.local/share/Steam/ubuntu12_32/reaper SteamLaunch AppId=570 -- game\n";
    assert_eq!(m.run(&clean_job(&p), Steam::Running, playing).unwrap(), "5 freed, 4 left alone.");
    assert!(m.t.has(&format!("{STEAM}/steamapps/shadercache/570")));
    for gone in ["steamapps/shadercache/1145360", "steamapps/downloading/1145360"] {
        assert!(!m.t.has(&format!("{STEAM}/{gone}")), "{gone}");
    }
    assert!(!m.t.has("/run/media/deck/sdcard/steamapps/shadercache/632470"));
    for there in kept {
        assert!(m.t.has(&format!("{STEAM}/{there}")), "{there}");
    }
    let record = m.t.read("/config/turbinehub/storage-last.json");
    assert!(record.contains("The game is running. Close it first") && record.contains("Close Steam first"), "{record}");
    // No game runs: the last leftover goes too, and nothing else does.
    assert_eq!(m.run(&clean_job(&p), Steam::Running, "").unwrap(), "1 freed, 8 left alone.");
    assert!(!m.t.has(&format!("{STEAM}/steamapps/shadercache/570")) && m.t.has(&format!("{STEAM}/steamapps/shadercache")));
    // Only things of installed games ticked: nothing done is an error with the reason.
    let only = plan(&r, &ids(&r, &[Kind::ResetBackup]), &closed()).unwrap();
    let why = m.run(&clean_job(&only), Steam::Running, "").unwrap_err();
    assert!(why.starts_with("Nothing was freed. ") && why.ends_with("Close Steam first: Steam uses its library folders while it runs."), "{why}");
    // The running games couldn't be listed: a leftover might be one of theirs, so it waits.
    fake::storage(&m.t.0, &m.t.at(fake::FILES_HOME));
    let shaders = plan(&r, &ids(&r, &[Kind::ShaderCache]), &closed()).unwrap();
    let mut no_list = |line: &[String]| if *line == *Steam::question() { Some((0, "3\n".to_string())) } else { None };
    let why = run(&clean_job(&shaders), &m.site(), &|_| {}, &mut no_list).unwrap_err();
    assert!(why.ends_with("Turbine Hub couldn't check whether the game is running, so it left this alone."), "{why}");
    assert!(m.t.has(&format!("{STEAM}/steamapps/shadercache/1145360")));

    // The rule itself. A game's own snapshots stay while that game runs.
    use super::clean::refusal;
    assert_eq!(refusal(false, false, "570", Steam::Running, Some(&["570".to_string()][..])), Some("The game is running. Close it first"));
    assert_eq!(refusal(false, false, "570", Steam::Running, Some(&[][..])), None);
    assert_eq!(refusal(false, false, "570", Steam::Unknown, None), Some("Turbine Hub couldn't check whether the game is running, so it left this alone"));
    assert_eq!(refusal(false, false, "", Steam::Unknown, None), None);
    // Inside a library: a leftover of a removed game goes while Steam runs and its id isn't running; anything
    // else, and anything at all when Steam couldn't be asked, waits.
    assert_eq!(refusal(true, true, "570", Steam::Running, Some(&[][..])), None);
    assert_eq!(refusal(true, true, "570", Steam::Running, Some(&["570".to_string()][..])), Some("The game is running. Close it first"));
    assert_eq!(refusal(true, true, "570", Steam::Running, None), Some("Turbine Hub couldn't check whether the game is running, so it left this alone"));
    assert_eq!(refusal(true, true, "570", Steam::Unknown, Some(&[][..])), Some("Turbine Hub couldn't check whether Steam is running, so it left Steam's folders alone. Close Steam, then try again"));
    assert_eq!(refusal(true, false, "570", Steam::Running, Some(&[][..])), Some("Close Steam first: Steam uses its library folders while it runs"));
    assert_eq!(refusal(true, true, "", Steam::Running, Some(&[][..])), Some("Close Steam first: Steam uses its library folders while it runs"));
    assert_eq!(refusal(true, false, "570", Steam::Closed, None), None);
}

/// While Steam runs its lists can change during a cleaning: each leftover is looked up in them again right before
/// it goes. A game that came back in the meantime (a manifest, the library list, a non-Steam game) keeps its files.
#[test]
fn a_leftover_is_checked_against_steams_lists_right_before_it_goes_while_steam_runs() {
    let m = Made::new("orphans");
    let r = m.scan();
    let mut ticked = ids(&r, &[Kind::ShaderCache, Kind::Compatdata]);
    ticked.push(by(&r, Kind::Download, "Unfinished download").id.clone());
    let p = plan(&r, &ticked, &Host { steam: Steam::Running, running: Some(Vec::new()), ..Host::default() }).unwrap();
    assert!(!p.text.contains("left alone"), "{}", p.text);
    // Since the look, with Steam running all along: Hades II is being installed again (Steam wrote its manifest),
    // the library list names app 570 on a card that is out, and the card's leftover became a non-Steam game.
    m.t.write(&format!("{STEAM}/steamapps/appmanifest_{HADES}.acf"), format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{HADES}\"\n\t\"name\"\t\t\"Hades II\"\n\t\"StateFlags\"\t\t\"1026\"\n\t\"installdir\"\t\t\"Hades II\"\n}}\n").as_bytes());
    let list = m.t.read(&format!("{STEAM}/steamapps/libraryfolders.vdf"));
    let end = list.trim_end().strip_suffix('}').unwrap();
    m.t.write(&format!("{STEAM}/steamapps/libraryfolders.vdf"), format!("{end}\"2\"\n{{\n\"path\" \"/run/media/deck/other-card\"\n\"apps\"\n{{\n\"570\" \"1\"\n}}\n}}\n}}\n").as_bytes());
    let why = m.run(&clean_job(&p), Steam::Running, "");
    // The card's shader cache (app 632470) is the one thing that is still a leftover.
    assert_eq!(why.unwrap(), "1 freed, 4 left alone.");
    for kept in ["steamapps/shadercache/1145360", "steamapps/compatdata/1145360/pfx", "steamapps/downloading/1145360", "steamapps/shadercache/570"] {
        assert!(m.t.has(&format!("{STEAM}/{kept}")), "{kept}");
    }
    assert!(!m.t.has("/run/media/deck/sdcard/steamapps/shadercache/632470"));
    let record = m.t.read("/config/turbinehub/storage-last.json");
    assert_eq!(record.matches("The game is in Steam again, so its files stay").count(), 4, "{record}");
}

#[test]
fn every_thing_is_looked_at_again_before_it_goes() {
    let m = Made::new("again");
    let r = m.scan();
    let mut ticked = ids(&r, &[Kind::ShaderCache, Kind::Compatdata, Kind::Proton, Kind::Download]);
    ticked.retain(|id| !id.contains("632470") && !id.contains(BG3));
    let p = plan(&r, &ticked, &closed()).unwrap();

    // Since the look: Hades II was installed again, the other game's cache became a link out of Steam's folders,
    // and a game was set to the Proton nothing used.
    m.t.write(&format!("{STEAM}/steamapps/appmanifest_{HADES}.acf"), format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{HADES}\"\n\t\"name\"\t\t\"Hades II\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"Hades II\"\n}}\n").as_bytes());
    m.t.write("/home/deck/Documents/precious/save.dat", b"keep");
    std::fs::remove_dir_all(m.t.at(&format!("{STEAM}/steamapps/shadercache/570"))).unwrap();
    m.t.link("/home/deck/Documents/precious", &format!("{STEAM}/steamapps/shadercache/570"));
    let config = m.t.read(&format!("{STEAM}/config/config.vdf")).replace("GE-Proton10-12", "GE-Proton10-4");
    m.t.write(&format!("{STEAM}/config/config.vdf"), config.as_bytes());

    assert_eq!(m.run(&clean_job(&p), Steam::Closed, "").unwrap(), "1 freed, 5 left alone.");
    for kept in ["steamapps/shadercache/1145360", "steamapps/compatdata/1145360/pfx", "steamapps/downloading/1145360", "compatibilitytools.d/GE-Proton10-4/proton"] {
        assert!(m.t.has(&format!("{STEAM}/{kept}")), "{kept}");
    }
    assert_eq!(m.t.read("/home/deck/Documents/precious/save.dat"), "keep");
    assert!(m.t.has(&format!("{STEAM}/steamapps/shadercache/570")), "the link itself stays too");
    assert!(!m.t.has(&format!("{STEAM}/steamapps/common/Proton 9.0")), "the one thing that was still what it was offered as");
    let record = m.t.read("/config/turbinehub/storage-last.json");
    assert!(record.contains("The game is in Steam again, so its files stay") && record.contains("It is a link to another place") && record.contains("Something uses it now, so it stays"), "{record}");

    // A look never offers what sits behind a link: a shader cache folder moved to the card and linked back.
    let m = Made::new("linked");
    std::fs::rename(m.t.at(&format!("{STEAM}/steamapps/shadercache")), m.t.at("/run/media/deck/sdcard/shadercache-moved")).unwrap();
    m.t.link("/run/media/deck/sdcard/shadercache-moved", &format!("{STEAM}/steamapps/shadercache"));
    let r = m.scan();
    assert!(r.findings.iter().all(|f| !f.id.contains(".local/share/Steam/steamapps/shadercache")), "{:?}", ids(&r, &[Kind::ShaderCache]));
    assert!(r.problems.iter().any(|p| p.contains("is a link to another place")), "{:?}", r.problems);
    // A step made by hand for something that was never offered is refused too.
    let rogue = Item { id: "x".into(), kind: Kind::ShaderCache, how: How::Erase, title: "A game's files".into(), paths: vec![format!("{STEAM}/steamapps/common/Baldurs Gate 3").into()], inside: format!("{STEAM}/steamapps/common").into(), app_id: BG3.into(), steam: true, orphan: false };
    let home = Item { kind: Kind::DownloadsFile, how: How::Bin, paths: vec!["/home/deck/Documents".into()], inside: "/home/deck".into(), steam: false, app_id: String::new(), ..rogue.clone() };
    assert!(m.run(&Job::Clean(vec![rogue, home]), Steam::Closed, "").unwrap_err().starts_with("Nothing was freed."));
    assert!(m.t.has(&format!("{STEAM}/steamapps/common/Baldurs Gate 3/game.exe")) && m.t.has("/home/deck/Documents/My setup.json"));
}

#[test]
fn games_that_are_only_away_are_not_leftovers() {
    let mut m = Made::new("away");
    // A non-Steam game's Windows files belong to its entry in Steam's list.
    let tile = crate::shortcuts::coop_tile("Elden Ring", "steam:1245620", "io.github.ravenor907.TurbineHub");
    let file = m.t.at(&format!("{STEAM}/userdata/1001/config/shortcuts.vdf"));
    crate::shortcuts::add_shortcut(&file, &tile, &crate::shortcuts::coop_marker("steam:1245620"), Steam::Closed).unwrap();
    m.t.write(&format!("{STEAM}/steamapps/compatdata/{}/pfx/user.reg", tile.appid()), b"reg");
    // A game on a library that isn't connected: Steam's list still names it there.
    let list = format!("\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n\"1\"\n{{\n\"path\" \"/run/media/deck/away\"\n\"apps\"\n{{\n\"570\" \"123\"\n}}\n}}\n}}\n", m.t.at(STEAM).display());
    m.t.write(&format!("{STEAM}/steamapps/libraryfolders.vdf"), list.as_bytes());
    m.reread();
    let r = m.scan();
    assert!(r.findings.iter().all(|f| !f.id.contains(&tile.appid().to_string()) && !f.id.ends_with("/570")), "{:?}", r.findings.iter().map(|f| &f.id).collect::<Vec<_>>());
    assert!(r.problems.iter().any(|p| p.contains("/run/media/deck/away")), "{:?}", r.problems);
    assert!(by(&r, Kind::Compatdata, "Hades II").why.ends_with("A Steam library that isn't connected right now may still use it."));
}

fn snapshot(data: &Path, game: &str, name: &str, at: u64, kind: &str) -> PathBuf {
    let dir = crate::snapshots::game_dir(data, game).join(at.to_string());
    std::fs::create_dir_all(dir.join("drive-0")).unwrap();
    std::fs::write(dir.join("drive-0/save.dat"), [9u8; 500]).unwrap();
    std::fs::write(dir.join("snapshot.json"), format!("{{\"game\":\"{game}\",\"name\":\"{name}\",\"at\":{at},\"kind\":\"{kind}\",\"bytes\":500,\"files\":1}}\n")).unwrap();
    dir
}

#[test]
fn old_snapshots_can_go_but_the_newest_always_stays() {
    let m = Made::new("snapshots");
    // An installed game with more than the kept count, and a game that is gone.
    let bg3: Vec<PathBuf> = (1..=4).map(|i| snapshot(&m.data, &format!("steam:{BG3}"), "Baldur's Gate 3", 1000 + i, "launch")).collect();
    snapshot(&m.data, &format!("steam:{BG3}"), "Baldur's Gate 3", 900, "manual");
    let hades: Vec<PathBuf> = (1..=3).map(|i| snapshot(&m.data, &format!("steam:{HADES}"), "Hades II", 2000 + i, "launch")).collect();
    let r = scan(&m.site(), &Host::default(), &Limits { keep: 2, ..Made::limits() });
    let old = by(&r, Kind::Snapshots, "Baldur's Gate 3");
    assert_eq!((old.title.as_str(), old.group, old.how, old.ticked, old.size), ("2 older save snapshots of Baldur's Gate 3", Group::Check, How::Erase, false, Size::exact(1000)));
    assert_eq!(old.paths, vec![bg3[1].clone(), bg3[0].clone()]);
    let gone = by(&r, Kind::Snapshots, "Hades II");
    assert_eq!(gone.paths, vec![hades[1].clone(), hades[0].clone()]);
    assert!(gone.why.contains("the newest one stays"));

    let p = plan(&r, &ids(&r, &[Kind::Snapshots]), &closed()).unwrap();
    assert!(p.danger, "saves erased for good: Cancel is highlighted");
    // While Baldur's Gate 3 runs its snapshots stay; the other game's go.
    assert_eq!(m.run(&clean_job(&p), Steam::Running, &format!("123 /home/deck/.local/share/Steam/ubuntu12_32/reaper SteamLaunch AppId={BG3} -- game\n")).unwrap(), "1 freed, 1 left alone.");
    assert!(bg3.iter().all(|d| d.is_dir()) && hades[2].is_dir() && !hades[0].exists() && !hades[1].exists());
    assert_eq!(m.run(&clean_job(&p), Steam::Closed, "").unwrap(), "1 freed, 1 left alone.");
    assert!(bg3[3].is_dir() && bg3[2].is_dir() && !bg3[1].exists() && !bg3[0].exists());
    // A step that names the newest snapshot is refused.
    let newest = Item { id: "x".into(), kind: Kind::Snapshots, how: How::Erase, title: "Newest".into(), paths: vec![hades[2].clone()], inside: crate::snapshots::root(&m.data), app_id: HADES.into(), steam: false, orphan: false };
    assert!(m.run(&Job::Clean(vec![newest]), Steam::Closed, "").is_err());
    assert!(hades[2].join("drive-0/save.dat").is_file());
}

#[test]
fn each_drives_recycle_bin_is_emptied_only_when_ticked() {
    let m = Made::new("bins");
    let r = m.scan();
    let card = by(&r, Kind::RecycleBin, "Recycle Bin on SD card");
    assert_eq!(card.size, Size::exact(4));
    let p = plan(&r, std::slice::from_ref(&card.id), &closed()).unwrap();
    assert!(p.items[0].1.starts_with("Erased for good"));
    assert_eq!(m.run(&clean_job(&p), Steam::Running, "").unwrap(), "1 freed.");
    let uid = m.places.who.uid;
    assert!(!m.t.has(&format!("/run/media/deck/sdcard/.Trash-{uid}/files/old-rom.zip")) && !m.t.has(&format!("/run/media/deck/sdcard/.Trash-{uid}/info/old-rom.zip.trashinfo")));
    assert!(m.t.has("/home/deck/.local/share/Trash/files/old-screenshot.png"), "the other bin wasn't ticked");
    assert!(m.scan().findings.iter().all(|f| f.kind != Kind::RecycleBin || f.drive == 0));
    assert!(undo_plan(&m.disk, &m.folder).is_none(), "an emptied bin has no Undo");
}

#[test]
fn flatpak_removes_its_own_unused_parts_as_a_step_of_the_plan() {
    let table = "\n        ID                                        Branch         Op\n 1.     org.freedesktop.Platform.GL.default       22.08          r\n 2.     org.kde.Platform                          5.15-22.08     r\n\nProceed with these changes to the system installation? [Y/n]: ";
    let unused = unused_from_answer(table);
    assert_eq!(unused, vec![("org.freedesktop.Platform.GL.default".to_string(), "22.08".to_string()), ("org.kde.Platform".to_string(), "5.15-22.08".to_string())]);
    assert_eq!(unused_from_answer("Uninstalling: runtime/org.gnome.Platform/x86_64/45\nUninstalling: org.kde.Platform.Locale/x86_64/5.15-22.08\n"), vec![("org.gnome.Platform".to_string(), "45".to_string()), ("org.kde.Platform.Locale".to_string(), "5.15-22.08".to_string())]);
    assert!(unused_from_answer("Nothing unused to uninstall\n").is_empty());
    let sizes = "org.freedesktop.Platform.GL.default\t22.08\t345.6\u{a0}MB\norg.freedesktop.Platform.GL.default\t23.08\t400.0 MB\norg.kde.Platform\t5.15-22.08\t1.2 GB\n";
    assert_eq!(unused_size(&unused, Some(sizes)), Size::exact(345_600_000 + 1_200_000_000));
    assert_eq!(unused_size(&unused, None), Size { bytes: 0, at_least: true });
    assert!(unused_size(&[("org.other.Runtime".into(), "1".into())], Some(sizes)).at_least);
    assert_eq!((unused_question().join(" "), unused_step().join(" ")), ("flatpak-spawn --host flatpak uninstall --unused".to_string(), "flatpak-spawn --host flatpak uninstall --unused --noninteractive -y".to_string()));

    let m = Made::new("flatpak");
    let host = Host { flatpak_unused: Some(table.into()), flatpak_sizes: Some(sizes.into()), ..closed() };
    let r = scan(&m.site(), &host, &Made::limits());
    let f = by(&r, Kind::FlatpakUnused, "2 app parts nothing uses");
    assert_eq!((f.group, f.how, f.ticked, f.size.bytes, f.paths.len()), (Group::Safe, How::Flatpak, true, 1_545_600_000, 0));
    assert!(f.why.contains("org.kde.Platform 5.15-22.08"), "{}", f.why);
    let p = plan(&r, std::slice::from_ref(&f.id), &host).unwrap();
    assert_eq!(p.steps, vec![Step::Storage(Job::Clean(Vec::new())), Step::Host(unused_step())]);
    assert!(p.text.contains("Flatpak removes the app parts nothing uses."), "{}", p.text);
    // Flatpak's own step counts as a slow one for the app, and needs no internet.
    assert!(!p.needs_internet());
    // Nothing unused, or a host that wasn't asked: no such finding.
    assert!(m.scan().findings.iter().all(|f| f.kind != Kind::FlatpakUnused));

    // What an app left behind is only told while the list of installed apps can be read, and never Turbine Hub's.
    m.t.write("/home/deck/.var/app/io.github.ravenor907.TurbineHub/config/turbinehub/settings.json", b"{}");
    let r = m.scan();
    assert_eq!(r.findings.iter().filter(|f| f.kind == Kind::FlatpakData).map(|f| f.title.as_str()).collect::<Vec<_>>(), ["Files left by com.example.OldLauncher"]);
    let mut blind = m.places.clone();
    blind.app_roots.clear();
    let site = Site { places: &blind, ..m.site() };
    assert!(scan(&site, &Host::default(), &Made::limits()).findings.iter().all(|f| f.kind != Kind::FlatpakData));
}

#[test]
fn a_nearly_full_drive_gets_a_line_in_the_checkup() {
    let mut m = Made::new("checkup");
    assert_eq!(m.scan().checkup(), None);
    m.disk.room = Some(Space { free: 12_000_000_000, total: 512_000_000_000 });
    let r = m.scan();
    assert!(r.drives.iter().all(|d| d.nearly_full() && d.percent_full() == Some(97)));
    let (title, line) = r.checkup().unwrap();
    assert_eq!((title.as_str(), line.as_str()), ("Your internal storage is nearly full", "22 KB can be freed."));
    // The card alone: its own words, and what could go after a look when nothing is plainly safe.
    let mut card = r.clone();
    card.drives[0].free = 300_000_000_000;
    assert_eq!(card.checkup().unwrap(), ("Your SD card is nearly full".to_string(), "4 KB can be freed.".to_string()));
    card.findings.retain(|f| f.group != Group::Safe);
    assert_eq!(card.checkup().unwrap().1, "Up to 1 KB could be freed, after a look at what it is.");
    card.findings.clear();
    assert_eq!(card.checkup().unwrap().1, "Only 11.2 GB left. See what is using the room.");

    let facts = crate::checkup::Facts { storage_full: r.checkup(), ..crate::checkup::Facts::default() };
    let machine = crate::hardware::Machine::read(&m.t.0, "", fake::HOME_DEV);
    let item = crate::checkup::items(&machine, &facts).into_iter().find(|i| i.id == "storage").unwrap();
    assert_eq!((item.title.as_str(), item.line.as_str(), item.action, item.level), ("Your internal storage is nearly full", "22 KB can be freed.", ("Free space", "storage"), crate::checkup::Level::Look));
    assert!(crate::checkup::items(&machine, &crate::checkup::Facts::default()).iter().all(|i| i.id != "storage"));
}

#[test]
fn the_page_gets_everything_as_json_and_scripts_get_the_same() {
    let m = Made::new("json");
    let r = m.scan();
    let j = crate::json::parse(&r.to_json(&r.default_ticks()).to_text()).unwrap();
    let groups = j.get("groups").unwrap().as_array();
    assert_eq!(groups.iter().map(|g| g.str_at("key").unwrap()).collect::<Vec<_>>(), ["safe", "check", "never"]);
    assert_eq!(groups[0].str_at("title"), Some("Safe to clear"));
    let first = &groups[0].get("items").unwrap().as_array()[0];
    assert_eq!((first.str_at("title"), first.str_at("how"), first.get("ticked").and_then(Json::as_bool), first.get("needsSteamClosed").and_then(Json::as_bool)), (Some("Unfinished download of Hades II"), Some("erase"), Some(true), Some(false)));
    assert_eq!(first.get("size").unwrap().str_at("words"), Some("9 KB"));
    assert!(groups[1].get("items").unwrap().as_array().iter().all(|i| i.get("ticked").and_then(Json::as_bool) == Some(false)));
    assert!(groups[2].get("items").unwrap().as_array().iter().all(|i| i.get("offered").and_then(Json::as_bool) == Some(false)));
    assert_eq!(j.str_at("free"), Some("Free 26 KB"));
    let drive = &j.get("drives").unwrap().as_array()[0];
    assert_eq!((drive.str_at("label"), drive.str_at("freeWords"), drive.get("nearlyFull").and_then(Json::as_bool)), (Some("Internal storage"), Some("214 GB"), Some(false)));
    assert_eq!(j.get("games").unwrap().as_array()[0].get("total").unwrap().str_at("words"), Some("7 KB"));
    assert_eq!(j.get("checkup"), Some(&Json::Null));

    // `turbinehub --json storage` on the same machine: only a look, nothing changed.
    let ctx = crate::cli::Ctx { root: m.t.0.clone(), home: m.t.at(fake::FILES_HOME), config_home: m.t.at("/config"), efibootmgr: String::new(), version: "0.1".into() };
    let (text, code) = crate::cli::run("storage", &ctx);
    let j = crate::json::parse(&text).unwrap();
    assert_eq!((code, j.get("ok").and_then(Json::as_bool)), (0, Some(true)));
    let s = j.get("storage").unwrap();
    assert_eq!(s.get("drives").unwrap().as_array().len(), 2);
    assert!(s.get("groups").unwrap().as_array()[0].get("items").unwrap().as_array().iter().any(|i| i.str_at("title") == Some("Shader cache of a removed game (app 570)")));
    assert!(m.t.has(&format!("{STEAM}/steamapps/shadercache/570")));
}

/// What a look asks the host: Steam, the running games, and Flatpak's own list of what is unused, which is only
/// ever answered with no (a look reads, it never removes).
#[test]
fn a_look_asks_the_host_and_only_ever_answers_no_to_flatpak() {
    let asked: RefCell<Vec<(Vec<String>, Option<String>)>> = RefCell::new(Vec::new());
    let mut ask = |line: &[String], typed: Option<&str>| {
        asked.borrow_mut().push((line.to_vec(), typed.map(String::from)));
        if *line == *Steam::question() {
            Some((0, "2\n".to_string()))
        } else if *line == *crate::agent::games_question() {
            Some((0, "9 /home/deck/.local/share/Steam/ubuntu12_32/reaper SteamLaunch AppId=570 -- game\n".to_string()))
        } else if *line == *unused_question() {
            Some((1, " 1. org.kde.Platform 6.7\n".to_string()))
        } else if *line == *sizes_question() {
            Some((0, "org.kde.Platform\t6.7\t1.2 GB\n".to_string()))
        } else {
            None
        }
    };
    let host = ask_host(&mut ask);
    assert_eq!((host.steam, host.running.clone()), (Steam::Running, Some(vec!["570".to_string()])));
    assert!(host.flatpak_unused.is_some() && host.flatpak_sizes.is_some());
    assert_eq!(asked.borrow().len(), 4);
    for (line, typed) in asked.borrow().iter() {
        assert!(!line.iter().any(|a| a == "-y" || a == "--noninteractive"), "{line:?}");
        if *line == unused_question() {
            assert!(typed.as_deref().is_some_and(|t| t.lines().count() >= 2 && t.lines().all(|l| l == "n")), "{typed:?}");
        } else {
            assert_eq!(*typed, None, "{line:?}");
        }
    }
    // Steam closed: no game runs, and that isn't asked. Steam not known: the running games aren't known either.
    let mut closed = |line: &[String], _: Option<&str>| if *line == *Steam::question() { Some((1, "0\n".to_string())) } else { None };
    let host = ask_host(&mut closed);
    assert_eq!((host.steam, host.running, host.flatpak_unused), (Steam::Closed, Some(Vec::new()), None));
    let mut silent = |_: &[String], _: Option<&str>| None;
    let host = ask_host(&mut silent);
    assert_eq!((host.steam, host.running), (Steam::Unknown, None));
}
