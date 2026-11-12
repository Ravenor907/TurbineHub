//! The one-press things on the made-up machine: the newest GE-Proton on a game's Proton page, pictures for blank
//! library entries from the checkup, "Check the game's files", and what players say beside Valve's rating.
use super::plan::Step;
use super::*;
use crate::json::parse;
use crate::shortcuts::{NewShortcut, Steam};
use crate::{proton_get, protondb, tile_art};

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-onepress-{tag}-{}", std::process::id()))
}

fn ui(tag: &str) -> Ui {
    Ui::sample(&dir(tag), HANDHELD, Some(Mode::Guided))
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

fn sections(j: &Json) -> Vec<Vec<Json>> {
    j.get("sections").map(Json::as_array).unwrap_or(&[]).iter().map(|sec| sec.get("rows").map(Json::as_array).unwrap_or(&[]).to_vec()).collect()
}

fn titles(list: &[Json]) -> Vec<&str> {
    list.iter().filter_map(|r| r.str_at("title")).collect()
}

fn dialog_of(answer: &str) -> Json {
    parse(answer).unwrap().get("dialog").cloned().unwrap_or_else(|| panic!("no dialog in {answer}"))
}

const BG3: &str = "steam:1086940";
const ELDEN: &str = "steam:1245620";

fn release(u: &Ui, tag: &str) {
    std::fs::create_dir_all(&u.cache).unwrap();
    std::fs::write(u.cache.join(proton_get::RELEASE.name), format!("{{\"tag_name\":\"{tag}\",\"assets\":[{{\"name\":\"{name}.tar.gz\",\"size\":503316480,\"browser_download_url\":\"https://github.com/GloriousEggroll/proton-ge-custom/releases/download/{tag}/{name}.tar.gz\"}}]}}", name = proton_get::names(tag)[0])).unwrap();
}

#[test]
fn the_proton_page_offers_the_newest_ge_proton_in_one_press() {
    let mut u = ui("get");
    let client = crate::net::Client::new("0.1", &dir("get").join("tmp"));
    let last = |u: &mut Ui| sections(&parse(&u.page("proton", BG3)).unwrap()).last().cloned().unwrap();
    // Before GitHub was ever asked: the press finds out which one is the newest.
    let rows = last(&mut u);
    assert_eq!(titles(&rows), vec!["Get the newest GE-Proton", "More versions"]);
    assert_eq!((rows[0].str_at("act"), rows[0].str_at("arg"), rows[0].str_at("button")), (Some("proton-get"), Some(""), Some("Get")));
    let d = dialog_of(&u.act("proton-get", "", ""));
    assert_eq!(d.str_at("title"), Some("Get the newest GE-Proton?"));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![Step::ProtonGet { tag: String::new() }]);
    assert!(plan.needs_internet() && plan.ask == super::plan::Ask::None && plan.helper_jobs() == 0);
    // The newest one is known and isn't here: the row and the dialog name it, with its size.
    release(&u, "GE-Proton10-99");
    let rows = last(&mut u);
    assert_eq!((rows[0].str_at("title"), rows[0].str_at("arg")), (Some("Get the newest GE-Proton"), Some("GE-Proton10-99")));
    assert!(rows[0].str_at("line").unwrap().starts_with("GE-Proton10-99: "));
    let d = dialog_of(&u.act("proton-get", "GE-Proton10-99", ""));
    assert_eq!(d.str_at("title"), Some("Get GE-Proton10-99?"));
    let items: Vec<(&str, &str)> = d.get("items").unwrap().as_array().iter().map(|i| (i.str_at("title").unwrap(), i.str_at("line").unwrap())).collect();
    assert_eq!(items[0], ("GE-Proton10-99", "From github.com/GloriousEggroll/proton-ge-custom, 480.0 MB to download"));
    assert_eq!(items[3].1, "It shows in this list at once. Steam lists it after its next start.");
    let plan = u.confirm().unwrap();
    assert_eq!((plan.steps.clone(), plan.done.as_str()), (vec![Step::ProtonGet { tag: "GE-Proton10-99".into() }], "GE-Proton10-99 is installed. Steam lists it after its next start."));
    // While it is fetched its row says so and A stops it; told to stop, it says that until the job has cleared up.
    u.enqueue(plan.clone());
    let (id, _) = u.queue_next().unwrap();
    let rows = last(&mut u);
    assert_eq!((rows[0].str_at("title"), rows[0].str_at("badge"), rows[0].str_at("button"), rows[0].str_at("act")), (Some("GE-Proton10-99"), Some("Working"), Some("Stop"), Some("work-stop")));
    assert!(parse(&u.act("proton-get", "GE-Proton10-99", "")).unwrap().str_at("toast").unwrap().starts_with("GE-Proton10-99 is being worked on"));
    u.sample = false;
    assert_eq!(parse(&u.act("work-stop", "", "")).unwrap().str_at("toast"), Some("Stopping GE-Proton10-99..."));
    u.sample = true;
    let rows = last(&mut u);
    assert_eq!((rows[0].str_at("badge"), rows[0].str_at("kind")), (Some("Stopping"), Some("info")));
    assert_eq!(u.queue_done(id, Err(crate::STOPPED.into())).unwrap().str_at("toast"), Some("GE-Proton10-99: stopped. Nothing was left behind."));
    assert_eq!(last(&mut u)[0].str_at("button"), Some("Get"));
    // Offline, nothing is asked: it needs the internet.
    u.offline = true;
    assert!(parse(&u.act("proton-get", "GE-Proton10-99", "")).unwrap().str_at("toast").unwrap().starts_with("You're offline"));
    u.offline = false;
    // A name that isn't a release's falls back to the newest known one.
    dialog_of(&u.act("proton-get", "../../etc", ""));
    assert_eq!(u.confirm().unwrap().steps, vec![Step::ProtonGet { tag: "GE-Proton10-99".into() }]);
    // The newest one is the one installed: "Up to date", nothing to press.
    release(&u, "GE-Proton10-4");
    let rows = last(&mut u);
    assert_eq!((rows[0].str_at("title"), rows[0].str_at("badge"), rows[0].str_at("kind")), (Some("GE-Proton10-4"), Some("Up to date"), Some("info")));
    assert_eq!(parse(&u.act("proton-get", "GE-Proton10-4", "")).unwrap().str_at("toast"), Some("GE-Proton10-4 is installed already."));

    // Remove is only for a build Turbine Hub fetched: the one that came some other way has no row.
    assert_eq!(parse(&u.act("proton-remove", "GE-Proton10-4", "")).unwrap().str_at("toast"), Some("Turbine Hub only removes the Proton builds it fetched itself."));
    std::fs::create_dir_all(u.folder()).unwrap();
    std::fs::write(proton_get::record_file(&u.folder()), "GE-Proton10-4\n").unwrap();
    assert_eq!(titles(&last(&mut u)), vec!["GE-Proton10-4", "Remove GE-Proton10-4", "More versions"]);
    // While a game is set to use it, the dialog only says which, and the step itself refuses too.
    let steam = u.ctx.home.join(".local/share/Steam");
    crate::compat_tool::set_proton_pick(&u.folder(), "1086940", Some(&proton_get::build_dir(&steam, "GE-Proton10-4"))).unwrap();
    let d = dialog_of(&u.act("proton-remove", "GE-Proton10-4", ""));
    assert_eq!((d.str_at("title"), d.get("guide").and_then(Json::as_bool)), (Some("Switch these to another Proton first"), Some(true)));
    assert_eq!(d.get("items").unwrap().as_array()[0].str_at("title"), Some("Baldur's Gate 3"));
    assert!(u.confirm().is_none());
    let step = Step::ProtonRemove { name: "GE-Proton10-4".into() };
    assert!(onepress::run_step(&step, &u.ctx.home, &u.folder(), &u.cache, &client, Steam::Closed, &|_, _| true).unwrap().unwrap_err().starts_with("1 game is set to use it"));
    crate::compat_tool::set_proton_pick(&u.folder(), "1086940", None).unwrap();
    let d = dialog_of(&u.act("proton-remove", "GE-Proton10-4", ""));
    assert_eq!((d.str_at("title"), d.get("danger").and_then(Json::as_bool)), (Some("Remove GE-Proton10-4?"), Some(true)));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![step.clone()]);
    onepress::run_step(&step, &u.ctx.home, &u.folder(), &u.cache, &client, Steam::Closed, &|_, _| true).unwrap().unwrap();
    assert!(!steam.join("compatibilitytools.d/GE-Proton10-4").exists());
    assert_eq!(titles(&last(&mut u)), vec!["Get the newest GE-Proton", "More versions"]);
    done("get");
}

fn entry(name: &str) -> NewShortcut {
    NewShortcut { name: name.into(), exe: format!("\"/home/deck/Games/{name}/start.sh\""), start_dir: "\"/home/deck/Games/\"".into(), launch_options: String::new(), flatpak_id: String::new(), icon: String::new() }
}

#[test]
fn blank_library_entries_get_pictures_from_the_checkup() {
    let mut u = ui("art");
    let client = crate::net::Client::new("0.1", &dir("art").join("tmp"));
    let run = |u: &Ui, step: &Step, steam: Steam| onepress::run_step(step, &u.ctx.home, &u.folder(), &u.cache, &client, steam, &|_, _| true).unwrap();
    // Nothing to fix: no card, and the press says so.
    assert_eq!(u.facts_now().tiles_without_art, 0);
    assert!(!crate::checkup::items(&u.machine(), &u.facts_now()).iter().any(|i| i.id == "tile-art"));
    assert_eq!(parse(&u.act("tile-art", "fix", "")).unwrap().str_at("toast"), Some("Every library entry has its pictures."));
    assert_eq!(parse(&u.act("tile-art", "undo", "")).unwrap().str_at("toast"), Some("There are no pictures of Turbine Hub's to take back."));
    // Two non-Steam games of the player's and Turbine Hub's own entry: only the player's count.
    let list = u.ctx.home.join(".local/share/Steam/userdata/1001/config/shortcuts.vdf");
    for s in [entry("Battle.net"), entry("Minecraft Launcher"), selftile::tile()] {
        crate::shortcuts::add_shortcut(&list, &s, selftile::MARKER, Steam::Closed).unwrap();
    }
    assert_eq!(u.facts_now().tiles_without_art, 2);
    let items = crate::checkup::items(&u.machine(), &u.facts_now());
    let card = items.iter().find(|i| i.id == "tile-art").unwrap();
    assert_eq!((card.title.as_str(), card.action, card.level), ("2 library entries have no pictures", ("Fix", "tile-art"), crate::checkup::Level::Tip));
    assert_eq!(onepress::checkup_title(1), "1 library entry has no pictures");
    // While Steam runs, or when that couldn't be asked, the press only explains.
    u.steam_running = true;
    let d = dialog_of(&u.act("checkup", "tile-art", ""));
    assert_eq!((d.str_at("title"), d.get("guide").and_then(Json::as_bool)), (Some("Steam has to be closed for this"), Some(true)));
    u.steam_unsure = true;
    assert!(dialog_of(&u.act("checkup", "tile-art", "")).str_at("text").unwrap().starts_with("Turbine Hub couldn't check whether Steam is running"));
    assert!(u.confirm().is_none());
    (u.steam_running, u.steam_unsure) = (false, false);
    // Steam closed: the dialog lists the entries it will fix.
    let d = dialog_of(&u.act("checkup", "tile-art", ""));
    assert_eq!(d.str_at("title"), Some("Draw pictures for 2 library entries?"));
    let items: Vec<(&str, &str)> = d.get("items").unwrap().as_array().iter().map(|i| (i.str_at("title").unwrap(), i.str_at("line").unwrap())).collect();
    assert_eq!(items[..2], [("Battle.net", "4 pictures"), ("Minecraft Launcher", "4 pictures")]);
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![Step::TileArt]);
    assert!(!plan.needs_internet() && plan.ask == super::plan::Ask::None);
    // Steam started meanwhile: the step asks again right before it writes, and writes nothing.
    assert!(run(&u, &Step::TileArt, Steam::Running).unwrap_err().starts_with("Close Steam first"));
    assert!(run(&u, &Step::TileArt, Steam::Unknown).is_err());
    assert_eq!(u.facts_now().tiles_without_art, 2);
    run(&u, &Step::TileArt, Steam::Closed).unwrap();
    assert_eq!(u.facts_now().tiles_without_art, 0);
    assert!(!crate::checkup::items(&u.machine(), &u.facts_now()).iter().any(|i| i.id == "tile-art"));
    assert_eq!(tile_art::undoable(&u.folder()), 8);
    // Turbine Hub's own entry got none of these: its own art is its own business.
    assert_eq!(selftile::state(&u.ctx.home), selftile::State::Added { ours: true, art_missing: 5 });
    // Undo takes exactly those eight files back.
    let d = dialog_of(&u.act("tile-art", "undo", ""));
    assert_eq!((d.str_at("title"), d.get("items").unwrap().as_array()[0].str_at("title")), (Some("Take the pictures back?"), Some("8 pictures")));
    assert_eq!(u.confirm().unwrap().steps, vec![Step::TileArtUndo]);
    run(&u, &Step::TileArtUndo, Steam::Closed).unwrap();
    assert_eq!((u.facts_now().tiles_without_art, tile_art::undoable(&u.folder())), (2, 0));
    assert!(std::fs::read_dir(list.with_file_name("grid")).unwrap().next().is_none());
    done("art");
}

#[test]
fn a_steam_game_offers_a_check_of_its_files() {
    let mut u = ui("validate");
    let page = parse(&u.page("games", BG3)).unwrap();
    let help = page.get("sections").unwrap().as_array().iter().find(|s| s.str_at("title") == Some("Won't start?")).unwrap().get("rows").unwrap().as_array().to_vec();
    assert_eq!((help[0].str_at("title"), help[0].str_at("act"), help[0].str_at("arg")), (Some("Check the game's files"), Some("game"), Some("validate|steam:1086940")));
    let d = dialog_of(&u.act("game", "validate|steam:1086940", ""));
    assert_eq!(d.str_at("title"), Some("Check the files of Baldur's Gate 3?"));
    let plan = u.confirm().unwrap();
    assert_eq!(plan.steps, vec![Step::Host(["flatpak-spawn", "--host", "xdg-open", "steam://validate/1086940"].iter().map(|x| x.to_string()).collect())]);
    assert!(plan.helper_jobs() == 0 && !plan.needs_internet());
    // A game Turbine Hub keeps its hands off has no "Won't start?" rows at all, and a game that isn't there
    // answers nothing.
    assert!(!u.page("games", ELDEN).contains("Check the game's files"));
    assert!(parse(&u.act("game", "validate|steam:1", "")).unwrap().entries().is_empty());
    done("validate");
}

#[test]
fn what_players_say_is_one_more_chip_and_decides_nothing() {
    let mut u = ui("players");
    let chips = |u: &mut Ui, id: &str| -> Vec<String> { parse(&u.page("games", id)).unwrap().get("header").unwrap().get("chips").unwrap().as_array().iter().map(|c| c.str_at("text").unwrap().to_string()).collect() };
    let before = (chips(&mut u, BG3), chips(&mut u, ELDEN));
    assert!(before.0.iter().chain(&before.1).all(|c| !c.starts_with("Players say")));
    let state = |u: &Ui| -> Vec<(String, bool, bool, String)> { games::listed(u).iter().map(|l| (games::game_id(&l.game), l.locked(), l.takes_switches(), l.verdict.label.words().to_string())).collect() };
    let was = state(&u);
    let panel = u.page("games", BG3);
    // The worst word for a game that works, the best for the locked one: each is shown, after Valve's.
    for (id, tier) in [("1086940", "borked"), ("1245620", "platinum")] {
        let f = protondb::file(&u.cache, id);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, format!("{{\"tier\":\"{tier}\",\"total\":9}}")).unwrap();
    }
    let after = (chips(&mut u, BG3), chips(&mut u, ELDEN));
    assert_eq!(after.0, before.0.iter().cloned().chain(["Players say: Borked".to_string()]).collect::<Vec<_>>());
    assert_eq!(after.1, before.1.iter().cloned().chain(["Players say: Platinum".to_string()]).collect::<Vec<_>>());
    // Nothing else moved: the labels, the locks, the switches and the game's rows are what they were.
    assert_eq!(state(&u), was);
    let strip = |page: &str| parse(page).unwrap().get("sections").map(Json::to_text).unwrap_or_default();
    assert_eq!(strip(&u.page("games", BG3)), strip(&panel));
    let counts = |u: &mut Ui| parse(&u.page("library", "")).unwrap().get("tiles").map(Json::to_text);
    let with = counts(&mut u);
    let _ = std::fs::remove_dir_all(u.cache.join("protondb"));
    assert_eq!(with, counts(&mut u));
    done("players");
}
