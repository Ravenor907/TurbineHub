//! The storage saver's screen: the Storage section of My machine, its look that never runs on the page, the
//! checkup's line, the plan and its Undo on the made-up machine, the pictures drawn for blank tiles with their
//! Undo, and a game's real size on its page.
use super::*;
use crate::fake::Preset;
use crate::json::parse;

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-storage-ui-{tag}-{}", std::process::id()))
}

fn ui(tag: &str, preset: Preset, mode: Mode) -> Ui {
    Ui::sample(&dir(tag), preset, Some(mode))
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

fn list(j: &Json, key: &str) -> Vec<Json> {
    j.get(key).map(Json::as_array).unwrap_or(&[]).to_vec()
}

fn rows(j: &Json) -> Vec<Json> {
    list(j, "sections").iter().flat_map(|sec| list(sec, "rows")).collect()
}

fn titled(j: &Json, title: &str) -> Json {
    rows(j).into_iter().find(|r| r.str_at("title") == Some(title)).unwrap_or_else(|| panic!("no row {title} among {:?}", rows(j).iter().map(|r| r.str_at("title").unwrap_or("").to_string()).collect::<Vec<_>>()))
}

fn section_titles(j: &Json) -> Vec<String> {
    list(j, "sections").iter().map(|s| s.str_at("title").unwrap_or("").to_string()).collect()
}

fn page(u: &mut Ui, name: &str, arg: &str) -> Json {
    parse(&u.page(name, arg)).unwrap()
}

fn act(u: &mut Ui, action: &str, arg: &str, value: &str) -> Json {
    parse(&u.act(action, arg, value)).unwrap()
}

fn on(r: &Json) -> bool {
    r.get("on").and_then(Json::as_bool) == Some(true)
}

const WINDOWS: &str = "Windows files of Hades II";

#[test]
fn storage_is_a_section_of_my_machine_on_every_machine() {
    for (tag, preset, before) in [("sec-h", HANDHELD, "sleep"), ("sec-d", Preset::Desktop, "drives"), ("sec-t", Preset::TvPc, "drives")] {
        let mut u = ui(tag, preset, Mode::Guided);
        let j = page(&mut u, "machine", "");
        let chips: Vec<String> = list(&j, "chips").iter().map(|c| c.str_at("id").unwrap().to_string()).collect();
        let at = chips.iter().position(|c| c == "storage").unwrap_or_else(|| panic!("{chips:?}"));
        assert_eq!(chips[at - 1], before, "{chips:?}");
        // Looking at Overview (its card looks into the section for its Options) never takes a look.
        assert!(u.storage_checkup().is_none() && !rows(&page(&mut u, "home", "")).iter().any(|r| r.str_at("title") == Some("Looking")));
        let card = list(&j, "cards").into_iter().find(|c| c.str_at("id") == Some("storage")).unwrap();
        assert_eq!((card.str_at("title"), card.str_at("value"), card.str_at("arg")), (Some("Storage"), Some("Free up space"), Some("module:storage")));
        let s = page(&mut u, "module:storage", "");
        assert_eq!((s.str_at("section"), s.str_at("title")), (Some("storage"), Some("Storage")));
        assert!(section_titles(&s).contains(&"Drives".to_string()), "{:?}", section_titles(&s));
        assert!(list(&s, "options").iter().any(|o| o.str_at("title") == Some("Look again") && o.str_at("act") == Some("storage-look")));
        assert!(list(&s, "options").iter().any(|o| o.str_at("title") == Some("About Storage")));
        done(tag);
    }
}

#[test]
fn the_section_shows_drives_and_the_leftovers_as_the_engine_sorts_them() {
    let mut u = ui("groups", HANDHELD, Mode::Guided);
    u.sample_storage("full");
    let j = page(&mut u, "module:storage", "");
    assert_eq!(section_titles(&j), ["Drives", "Safe to clear \u{b7} 23.3 GB", "Check first \u{b7} 16.8 GB", "Biggest games"]);
    // Each drive: a meter with its room and what can be freed on it.
    let card = titled(&j, "SD card");
    assert_eq!((card.str_at("kind"), card.str_at("value"), card.str_at("badge"), card.str_at("tone")), (Some("meter"), Some("13.0 GB free of 238 GB"), Some("Nearly full"), Some("warn")));
    assert_eq!(card.str_at("line"), Some("12.0 GB can be freed, and up to 922 MB more after a look at what it is."));
    assert!((card.get("part").and_then(Json::as_f64).unwrap() - 0.94).abs() < 0.001);
    let inside = titled(&j, "Internal storage");
    assert_eq!((inside.str_at("value"), inside.str_at("badge")), (Some("214 GB free of 477 GB"), None));
    assert!(inside.str_at("line").unwrap().starts_with("11.3 GB can be freed, and up to "), "{:?}", inside.str_at("line"));
    // Safe to clear: ticked, each with its size and one plain line.
    let safe = list(&list(&j, "sections")[1], "rows");
    assert!(safe.iter().all(|r| r.str_at("kind") == Some("check") && on(r) && r.str_at("act") == Some("storage-tick") && !r.str_at("line").unwrap().is_empty()), "{safe:?}");
    let shader = titled(&j, "Shader cache of Hades II");
    assert_eq!((shader.str_at("value"), shader.str_at("badge")), (Some("3.4 GB"), None));
    // Check first: not ticked, each with the engine's reason and what happens to it.
    let check = list(&list(&j, "sections")[2], "rows");
    assert!(check.iter().all(|r| !on(r) && matches!(r.str_at("badge"), Some("Goes to the Recycle Bin" | "Erased for good"))), "{check:?}");
    let windows = titled(&j, WINDOWS);
    assert_eq!((windows.str_at("badge"), windows.str_at("value")), (Some("Goes to the Recycle Bin"), Some("1.3 GB")));
    assert!(windows.str_at("line").unwrap().contains("saves"));
    assert_eq!(titled(&j, "Recycle Bin").str_at("badge"), Some("Erased for good"));
    assert_eq!(titled(&j, "Unfinished update of Baldur's Gate 3").str_at("badge"), Some("Erased for good"));
    // What is in use is not listed at all.
    assert!(!rows(&j).iter().any(|r| r.str_at("title") == Some("Proton 11.0")));
    // The highlight starts on the first thing to tick, the main button frees the ticked set, and the line says
    // when the look was taken.
    assert_eq!(j.str_at("focus"), safe[0].str_at("id"));
    assert!(j.str_at("focus").unwrap().starts_with("storage-tick:download:"), "{:?}", j.str_at("focus"));
    let primary = j.get("primary").unwrap();
    assert_eq!((primary.str_at("label"), primary.str_at("act"), primary.str_at("k")), (Some("Free 23.3 GB"), Some("storage-free"), Some("start")));
    assert!(j.get("header").unwrap().str_at("line").unwrap().starts_with("Looked just now. "), "{:?}", j.get("header"));
    // The biggest games by their real size open the game.
    let games = list(list(&j, "sections").last().unwrap(), "rows");
    assert_eq!(games.len(), 5);
    assert_eq!((games[0].str_at("title"), games[0].str_at("value"), games[0].str_at("act"), games[0].str_at("arg")), (Some("Baldur's Gate 3"), Some("117 GB"), Some("open"), Some("games|steam:1086940")));
    assert!(games[0].str_at("line").unwrap().starts_with("Game 114 GB, shader cache 2.6 GB"), "{:?}", games[0].str_at("line"));
    // Veteran sees where each thing is.
    let mut v = ui("groups-v", HANDHELD, Mode::Veteran);
    v.sample_storage("full");
    assert!(titled(&page(&mut v, "module:storage", ""), "Shader cache of Hades II").str_at("line").unwrap().ends_with("/home/deck/.local/share/Steam/steamapps/shadercache/1145360"));
    done("groups");
    done("groups-v");
}

#[test]
fn ticks_change_the_button_and_start_opens_the_engines_plan() {
    let mut u = ui("plan", HANDHELD, Mode::Guided);
    u.sample_storage("full");
    let j = page(&mut u, "module:storage", "");
    let windows = titled(&j, WINDOWS);
    assert_eq!(act(&mut u, "storage-tick", windows.str_at("arg").unwrap(), "").get("refresh").and_then(Json::as_bool), Some(true));
    let j = page(&mut u, "module:storage", "");
    assert!(on(&titled(&j, WINDOWS)));
    assert_eq!(j.get("primary").unwrap().str_at("label"), Some("Free 24.6 GB"));
    // The plan: every thing by name, with what happens to it. Nothing that may hold saves is erased, so it is
    // not a dangerous dialog.
    let d = act(&mut u, "storage-free", "", "");
    let d = d.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.str_at("confirm"), d.get("danger").and_then(Json::as_bool)), (Some("Free up space"), Some("Free 24.6 GB"), Some(false)));
    let items = list(d, "items");
    assert!(items.iter().any(|i| i.str_at("title") == Some("Windows files of Hades II (1.3 GB)") && i.str_at("line").unwrap().starts_with("Goes to the Recycle Bin")), "{items:?}");
    assert!(items.iter().any(|i| i.str_at("title") == Some("Shader cache of Hades II (3.4 GB)") && i.str_at("line") == Some("Erased for good.")));
    assert!(d.str_at("text").unwrap().contains("SD card has 13.0 GB free now, and about 25.0 GB after."), "{:?}", d.str_at("text"));
    u.cancel();
    // Something that may hold saves erased for good (the Recycle Bin emptied): Cancel is the highlighted button.
    let bin = titled(&j, "Recycle Bin");
    act(&mut u, "storage-tick", bin.str_at("arg").unwrap(), "");
    assert_eq!(act(&mut u, "storage-free", "", "").get("dialog").unwrap().get("danger").and_then(Json::as_bool), Some(true));
    u.cancel();
    // Everything unticked: no main button, and the press says why.
    for r in rows(&page(&mut u, "module:storage", "")).iter().filter(|r| r.str_at("act") == Some("storage-tick") && on(r)) {
        act(&mut u, "storage-tick", r.str_at("arg").unwrap(), "");
    }
    let j = page(&mut u, "module:storage", "");
    assert!(j.get("primary").is_none_or(|p| *p == Json::Null));
    assert!(j.get("header").unwrap().str_at("line").unwrap().ends_with("Tick what should go."));
    assert_eq!(act(&mut u, "storage-free", "", "").str_at("toast"), Some("Nothing is ticked. Tick what should go first."));
    // A tick for something that isn't offered changes nothing.
    act(&mut u, "storage-tick", "proton:/nowhere", "");
    assert!(page(&mut u, "module:storage", "").get("primary").is_none_or(|p| *p == Json::Null));
    done("plan");
}

#[test]
fn with_steam_running_the_leftovers_of_removed_games_still_go() {
    let mut u = ui("running", HANDHELD, Mode::Guided);
    u.steam_running = true;
    u.sample_storage("full");
    let j = page(&mut u, "module:storage", "");
    // What belongs to an installed game, or to Steam itself, says that it waits for Steam to be closed.
    assert!(titled(&j, "Proton 9.0").str_at("line").unwrap().ends_with("Needs Steam closed."));
    assert!(titled(&j, "Unfinished update of Baldur's Gate 3").str_at("line").unwrap().ends_with("Needs Steam closed."));
    assert!(!titled(&j, WINDOWS).str_at("line").unwrap().contains("Needs Steam closed") && !titled(&j, "Shader cache of Hades II").str_at("line").unwrap().contains("Steam closed"));
    for title in [WINDOWS, "Proton 9.0"] {
        act(&mut u, "storage-tick", titled(&j, title).str_at("arg").unwrap(), "");
    }
    let d = act(&mut u, "storage-free", "", "");
    let d = d.get("dialog").unwrap();
    let items = list(d, "items");
    assert!(items.iter().any(|i| i.str_at("title") == Some("Windows files of Hades II (1.3 GB)")), "{items:?}");
    assert!(items.iter().any(|i| i.str_at("title") == Some("Shader cache of Hades II (3.4 GB)")));
    assert!(items.iter().any(|i| i.str_at("title") == Some("Left alone: Proton 9.0") && i.str_at("line").unwrap().starts_with("Close Steam first")), "{items:?}");
    // It runs on the made-up machine as it would on a real one: the leftovers go, the Proton stays.
    let plan = u.confirm().unwrap();
    assert!(u.runs_covered(&plan));
    u.play_sample(&plan).unwrap();
    let steam = u.ctx.home.join(".local/share/Steam/steamapps");
    assert!(!steam.join("shadercache/1145360").exists() && !steam.join("compatdata/1145360").exists() && !steam.join("downloading/1145360").exists());
    assert!(steam.join("common/Proton 9.0/proton").exists() && steam.join("shadercache/1086940").exists());
    done("running");
}

#[test]
fn a_cleaning_on_the_made_up_machine_frees_the_room_and_undo_puts_back_what_went_to_the_bin() {
    let mut u = ui("clean", HANDHELD, Mode::Guided);
    u.sample_storage("full");
    let j = page(&mut u, "module:storage", "");
    assert!(!rows(&j).iter().any(|r| r.str_at("act") == Some("storage-undo")));
    act(&mut u, "storage-tick", titled(&j, WINDOWS).str_at("arg").unwrap(), "");
    assert!(act(&mut u, "storage-free", "", "").get("dialog").is_some());
    let plan = u.confirm().unwrap();
    u.play_sample(&plan).unwrap();
    // A new look was taken: the safe group is gone, the drives have the room, and the checkup has nothing to say.
    let j = page(&mut u, "module:storage", "");
    assert!(!section_titles(&j).iter().any(|t| t.starts_with("Safe to clear")), "{:?}", section_titles(&j));
    assert_eq!(titled(&j, "SD card").str_at("value"), Some("25.0 GB free of 238 GB"));
    assert_eq!(titled(&j, "SD card").str_at("badge"), None);
    assert_eq!(u.facts_now().storage_full, None);
    assert!(!rows(&j).iter().any(|r| r.str_at("title") == Some(WINDOWS)));
    // The Windows files wait in the Recycle Bin: Undo is on the page and in Review changes.
    let undo = titled(&j, "Undo the last cleaning");
    assert_eq!((undo.str_at("act"), undo.str_at("button")), (Some("storage-undo"), Some("Undo")));
    assert!(undo.str_at("line").unwrap().starts_with("Puts back the 1 thing it moved"), "{:?}", undo.str_at("line"));
    let changes = page(&mut u, "changes", "");
    assert_eq!(titled(&changes, "The last storage cleaning").str_at("act"), Some("storage-undo"));
    let d = act(&mut u, "storage-undo", "", "");
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Put back what was freed"));
    let plan = u.confirm().unwrap();
    u.play_sample(&plan).unwrap();
    assert!(u.ctx.home.join(".local/share/Steam/steamapps/compatdata/1145360/pfx").is_dir());
    let j = page(&mut u, "module:storage", "");
    assert!(!on(&titled(&j, WINDOWS)), "what came back is not ticked");
    assert!(!rows(&j).iter().any(|r| r.str_at("act") == Some("storage-undo")));
    assert!(!rows(&page(&mut u, "changes", "")).iter().any(|r| r.str_at("act") == Some("storage-undo")));
    assert_eq!(act(&mut u, "storage-undo", "", "").str_at("toast"), Some("There is nothing left in the Recycle Bin to put back."));
    done("clean");
}

#[test]
fn the_look_never_runs_on_the_page_and_the_last_one_is_kept() {
    let mut u = ui("worker", HANDHELD, Mode::Guided);
    // As on a real machine: the page asks for a look and shows that it is looking.
    u.sample = false;
    assert!(!u.storage_take_wanted(), "nothing asked for a look yet");
    // Home, Overview and a card's Options look into the section without asking for a look.
    page(&mut u, "home", "");
    page(&mut u, "machine", "");
    assert!(!u.storage_take_wanted());
    let j = page(&mut u, "module:storage", "");
    let looking = titled(&j, "Looking");
    assert_eq!((looking.str_at("kind"), looking.str_at("badge")), (Some("info"), Some("Working")));
    assert!(j.get("primary").is_none_or(|p| *p == Json::Null));
    assert!(u.storage_take_wanted(), "the page asked for it");
    assert!(!u.storage_take_wanted(), "once");
    // The page again while the look runs: still looking, and no second look.
    page(&mut u, "module:storage", "");
    assert!(!u.storage_take_wanted());
    assert_eq!(act(&mut u, "storage-look", "", "").str_at("toast"), Some("Still looking. It shows here when it is done."));
    assert_eq!(act(&mut u, "storage-free", "", "").str_at("toast"), Some("Still looking. Try again when the list is here."));
    // The worker hands the report back: it is shown, with when it was taken, however often the page is drawn.
    u.sample = true;
    let mut other = ui("worker-b", HANDHELD, Mode::Guided);
    other.sample_storage("full");
    let (report, host) = other.storage_last().unwrap();
    u.sample = false;
    u.storage_found(report, host);
    for _ in 0..3 {
        let j = page(&mut u, "module:storage", "");
        assert!(j.get("header").unwrap().str_at("line").unwrap().starts_with("Looked just now."));
        assert!(!u.storage_take_wanted(), "a page reload never looks again");
    }
    // The checkup reads the kept report and never starts a look.
    assert_eq!(u.facts_now().storage_full, Some(("Your SD card is nearly full".to_string(), "12.0 GB can be freed.".to_string())));
    page(&mut u, "home", "");
    assert!(!u.storage_take_wanted());
    // Look again asks for one; the list from before stays until the new one is here, ticks and all.
    let windows = titled(&page(&mut u, "module:storage", ""), WINDOWS);
    act(&mut u, "storage-tick", windows.str_at("arg").unwrap(), "");
    assert_eq!(act(&mut u, "storage-look", "", "").get("refresh").and_then(Json::as_bool), Some(true));
    assert!(u.storage_take_wanted());
    let j = page(&mut u, "module:storage", "");
    assert!(j.get("header").unwrap().str_at("line").unwrap().starts_with("Looking again"), "{:?}", j.get("header"));
    let (report, host) = other.storage_last().unwrap();
    u.storage_found(report, host);
    let j = page(&mut u, "module:storage", "");
    assert!(on(&titled(&j, WINDOWS)) && on(&titled(&j, "Shader cache of Hades II")));
    // A cleaning that ended makes the kept report wrong: it is dropped and a new look is asked for.
    let mut p = Plan::new("Free up space", "", "Free");
    p.steps.push(Step::Storage(crate::storage::Job::Clean(Vec::new())));
    u.finish(&p).unwrap();
    assert!(u.storage_take_wanted());
    assert_eq!(titled(&page(&mut u, "module:storage", ""), "Looking").str_at("kind"), Some("info"));
    // A second cleaning ends while that look still runs: what the look brings is from before it, so it is not
    // shown, and the look asked for since starts.
    u.finish(&p).unwrap();
    assert!(!u.storage_take_wanted(), "one look at a time");
    let (report, host) = other.storage_last().unwrap();
    u.storage_found(report, host);
    assert_eq!(titled(&page(&mut u, "module:storage", ""), "Looking").str_at("kind"), Some("info"));
    assert!(u.storage_take_wanted());
    let (report, host) = other.storage_last().unwrap();
    u.storage_found(report, host);
    assert!(page(&mut u, "module:storage", "").get("header").unwrap().str_at("line").unwrap().starts_with("Looked just now."));
    done("worker");
    done("worker-b");
}

#[test]
fn a_nearly_full_drive_is_a_checkup_card_that_opens_storage() {
    let mut u = ui("checkup", HANDHELD, Mode::Guided);
    assert_eq!(u.facts_now().storage_full, None);
    u.sample_storage("full");
    assert_eq!(u.facts_now().storage_full, Some(("Your SD card is nearly full".to_string(), "12.0 GB can be freed.".to_string())));
    let home = page(&mut u, "home", "");
    let item = list(&home, "items").into_iter().find(|i| i.str_at("id") == Some("storage")).unwrap();
    assert_eq!((item.str_at("title"), item.str_at("line"), item.str_at("button"), item.str_at("opens")), (Some("Your SD card is nearly full"), Some("12.0 GB can be freed."), Some("Free space"), Some("storage")));
    assert_eq!(act(&mut u, "checkup", "storage", "").str_at("open"), Some("module:storage"));
    // The Overview's card says the same in a few words.
    let card = list(&page(&mut u, "machine", ""), "cards").into_iter().find(|c| c.str_at("id") == Some("storage")).unwrap();
    assert_eq!(card.str_at("value"), Some("23.3 GB can be freed"));
    // Before any look, on a real machine: how full the drives are is enough for the card, and worth a look.
    let mut r = ui("checkup-b", HANDHELD, Mode::Guided);
    r.sample = false;
    r.storage_quick(None);
    assert!(r.facts_now().storage_full.is_none() && !r.storage_take_wanted());
    r.storage_quick(Some(("Your SD card is nearly full".into(), "Only 12 GB left. See what is using the room.".into())));
    assert_eq!(r.facts_now().storage_full.map(|x| x.1), Some("Only 12 GB left. See what is using the room.".to_string()));
    assert!(r.storage_take_wanted());
    // With nothing to clear the section says so.
    let mut e = ui("checkup-c", HANDHELD, Mode::Guided);
    e.sample_storage("empty");
    let j = page(&mut e, "module:storage", "");
    assert!(section_titles(&j).contains(&"Nothing to clear".to_string()), "{:?}", section_titles(&j));
    assert!(!rows(&j).iter().any(|r| r.str_at("act") == Some("storage-tick")));
    assert!(j.get("primary").is_none_or(|p| *p == Json::Null));
    for tag in ["checkup", "checkup-b", "checkup-c"] {
        done(tag);
    }
}

#[test]
fn drawn_pictures_have_an_undo_where_the_fix_was_and_in_review_changes() {
    let mut u = ui("art", Preset::Desktop, Mode::Guided);
    u.sample_tile_art("fix");
    let facts = u.facts_now();
    assert!(facts.tiles_without_art >= 2 && facts.tiles_drawn == 0, "{} {}", facts.tiles_without_art, facts.tiles_drawn);
    assert!(!rows(&page(&mut u, "changes", "")).iter().any(|r| r.str_at("act") == Some("tile-art")));
    // The fix runs on the made-up machine: the pictures are drawn into its Steam folder.
    assert!(act(&mut u, "checkup", "tile-art", "").get("dialog").is_some());
    let plan = u.confirm().unwrap();
    u.play_sample(&plan).unwrap();
    let facts = u.facts_now();
    assert!(facts.tiles_without_art == 0 && facts.tiles_drawn >= 6, "{} {}", facts.tiles_without_art, facts.tiles_drawn);
    // Where the fix was offered there is now its Undo, and Review changes lists it among the player's own folders.
    u.home_all = true;
    let home = page(&mut u, "home", "");
    let item = list(&home, "items").into_iter().find(|i| i.str_at("id") == Some("tile-art-undo")).unwrap_or_else(|| panic!("{:?}", list(&home, "items")));
    assert_eq!((item.str_at("button"), item.str_at("opens"), item.get("look").and_then(Json::as_bool)), (Some("Undo"), Some("tile-art-undo"), Some(false)));
    assert!(item.str_at("title").unwrap().starts_with("Turbine Hub drew ") && item.str_at("title").unwrap().ends_with(" pictures for your library"));
    let changes = page(&mut u, "changes", "");
    let row = rows(&changes).into_iter().find(|r| r.str_at("act") == Some("tile-art") && r.str_at("arg") == Some("undo")).unwrap();
    assert_eq!(row.str_at("button"), Some("Undo"));
    assert!(section_titles(&changes).contains(&"In your own folders, no password".to_string()));
    let d = act(&mut u, "checkup", "tile-art-undo", "");
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Take the pictures back?"));
    let plan = u.confirm().unwrap();
    u.play_sample(&plan).unwrap();
    let facts = u.facts_now();
    assert!(facts.tiles_drawn == 0 && facts.tiles_without_art >= 2);
    assert!(!rows(&page(&mut u, "changes", "")).iter().any(|r| r.str_at("act") == Some("tile-art")));
    done("art");
}

#[test]
fn a_games_page_says_what_the_game_really_takes() {
    let mut u = ui("size", HANDHELD, Mode::Guided);
    let line = |u: &mut Ui| page(u, "games", "steam:1086940").get("header").unwrap().str_at("line").unwrap().to_string();
    assert!(line(&mut u).ends_with("\u{b7} Takes 114 GB"), "{}", line(&mut u));
    // Veteran sees what it is made of, once there is more than the game's own files.
    let mut v = ui("size-v", HANDHELD, Mode::Veteran);
    v.sample_storage("full");
    assert!(line(&mut v).contains("\u{b7} Takes 117 GB (game 114 GB, shader cache 2.6 GB, Windows files "), "{}", line(&mut v));
    // On a real machine the page never counts: it asks once, shows the line without the size, and has it when
    // the worker hands it back.
    let mut r = ui("size-r", HANDHELD, Mode::Guided);
    r.sample = false;
    assert!(!line(&mut r).contains("Takes"));
    assert_eq!(r.storage_take_size_wanted().as_deref(), Some("steam:1086940"));
    assert_eq!(r.storage_take_size_wanted(), None);
    let found = crate::storage::GameSize { id: "steam:1086940".into(), name: "Baldur's Gate 3".into(), store: crate::Store::Steam, drive: 0, library: PathBuf::new(), files: crate::storage::Size::exact(40_000_000_000), shaders: crate::storage::Size::exact(2_000_000_000), proton: crate::storage::Size::default() };
    r.storage_game_size("steam:1086940", Some(found));
    assert!(line(&mut r).ends_with("\u{b7} Takes 39.1 GB"), "{}", line(&mut r));
    assert_eq!(r.storage_take_size_wanted(), None, "what is known is never asked for again");
    // A game that couldn't be counted is asked for once, not at every reload.
    line(&mut r);
    page(&mut r, "games", "steam:1245620");
    assert_eq!(r.storage_take_size_wanted().as_deref(), Some("steam:1245620"));
    r.storage_game_size("steam:1245620", None);
    page(&mut r, "games", "steam:1245620");
    assert_eq!(r.storage_take_size_wanted(), None);
    for tag in ["size", "size-v", "size-r"] {
        done(tag);
    }
}
