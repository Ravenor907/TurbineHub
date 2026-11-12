use super::*;
use crate::fake::Preset;
use crate::files::When;
use crate::json::parse;

fn dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("turbinehub-uifiles-{tag}-{}", std::process::id()))
}

fn ui(tag: &str, mode: Mode) -> Ui {
    Ui::sample(&dir(tag), Preset::Handheld, Some(mode))
}

fn done(tag: &str) {
    let _ = std::fs::remove_dir_all(dir(tag));
}

fn page(u: &mut Ui) -> Json {
    parse(&u.page("files", "")).unwrap()
}

fn act(u: &mut Ui, action: &str, arg: &str, value: &str) -> Json {
    parse(&u.act(action, arg, value)).unwrap()
}

/// The names of the list's lines, without the group headings.
fn names(j: &Json) -> Vec<String> {
    j.get("rows").unwrap().as_array().iter().filter_map(|r| r.str_at("name")).map(String::from).collect()
}

fn groups(j: &Json) -> Vec<String> {
    j.get("rows").unwrap().as_array().iter().filter_map(|r| r.str_at("group")).map(String::from).collect()
}

fn row<'a>(j: &'a Json, name: &str) -> &'a Json {
    j.get("rows").unwrap().as_array().iter().find(|r| r.str_at("name") == Some(name)).unwrap_or_else(|| panic!("no line named {name}"))
}

fn tiles(j: &Json) -> Vec<(String, Vec<String>)> {
    j.get("sections").unwrap().as_array().iter().map(|sec| (sec.str_at("title").unwrap_or("").to_string(), sec.get("tiles").unwrap().as_array().iter().filter_map(|t| t.str_at("name")).map(String::from).collect())).collect()
}

fn go(u: &mut Ui, key: &str) -> Json {
    act(u, "files-go", key, "");
    page(u)
}

fn toast(j: &Json) -> &str {
    j.str_at("toast").unwrap_or("")
}

fn list(paths: &[&str]) -> String {
    format!("[{}]", paths.iter().map(|p| crate::json::quote(p)).collect::<Vec<_>>().join(","))
}

fn strings(j: &Json, out: &mut Vec<String>) {
    match j {
        Json::Str(t) => out.push(t.clone()),
        Json::Arr(a) => a.iter().for_each(|x| strings(x, out)),
        Json::Obj(o) => o.iter().for_each(|(_, v)| strings(v, out)),
        _ => {}
    }
}

const HOME: &str = "/home/deck";
const DOWNLOADS: &str = "/home/deck/Downloads";
const PICTURES: &str = "/home/deck/Pictures";
const DOCUMENTS: &str = "/home/deck/Documents";

#[test]
fn guided_starts_windows_like_on_home_and_veteran_in_true_linux() {
    let mut u = ui("start", Mode::Guided);
    let j = page(&mut u);
    assert_eq!((j.str_at("view"), j.str_at("place"), j.str_at("kind")), (Some("windows"), Some("home"), Some("tiles")));
    assert_eq!(tiles(&j)[0], ("Quick access".to_string(), ["Desktop", "Downloads", "Documents", "Pictures", "Music", "Videos"].map(String::from).to_vec()));
    let nav: Vec<&str> = j.get("nav").unwrap().as_array().iter().filter_map(|n| n.str_at("label")).collect();
    assert_eq!(nav, ["Home", "Desktop", "Downloads", "Documents", "Pictures", "Music", "Videos", "This PC", "SteamOS (C:)", "SD card (D:)", "Games", "Apps", "Recycle Bin"]);
    // This PC: the folders, then the drives with their room and the bin with its count.
    let pc = go(&mut u, "pc");
    assert_eq!(tiles(&pc)[1], ("Devices and drives".to_string(), ["SteamOS (C:)", "SD card (D:)", "Recycle Bin"].map(String::from).to_vec()));
    let drive = &pc.get("sections").unwrap().as_array()[1].get("tiles").unwrap().as_array()[0];
    assert_eq!((drive.str_at("room"), drive.get("used").and_then(Json::as_f64)), (Some("214 GB free of 477 GB"), Some(55.0)));
    // The switch is always there, and it keeps the same folder.
    go(&mut u, DOWNLOADS);
    assert_eq!(act(&mut u, "files-view", "linux", "").get("refresh").and_then(Json::as_bool), Some(true));
    let j = page(&mut u);
    assert_eq!((j.str_at("view"), j.str_at("place"), j.str_at("typed")), (Some("linux"), Some(DOWNLOADS), Some(DOWNLOADS)));
    assert!(row(&j, "wallpaper-jade.png").str_at("rights").unwrap().starts_with("-rw"));
    assert_eq!(j.get("status").unwrap().str_at("words"), Some("Showing real names and paths"));
    done("start");

    let mut v = ui("start-veteran", Mode::Veteran);
    let j = page(&mut v);
    assert_eq!((j.str_at("view"), j.str_at("place"), j.str_at("kind")), (Some("linux"), Some(HOME), Some("list")));
    // Hidden files show in True Linux, and not in Windows-like until asked.
    assert!(names(&j).contains(&".config".to_string()));
    act(&mut v, "files-view", "windows", "");
    let j = page(&mut v);
    assert!(!names(&j).contains(&".config".to_string()) && names(&j).contains(&"AppData".to_string()));
    act(&mut v, "files-hidden", "", "");
    assert!(names(&page(&mut v)).contains(&".config".to_string()));
    done("start-veteran");
}

#[test]
fn a_list_is_explorers_with_downloads_under_its_dates() {
    let mut u = ui("list", Mode::Guided);
    let j = go(&mut u, DOWNLOADS);
    assert_eq!((j.str_at("sort"), j.get("desc").and_then(Json::as_bool), j.str_at("group")), (Some("date"), Some(true), Some("date")));
    assert_eq!(groups(&j).first().map(String::as_str), Some("Today"));
    assert_eq!(names(&j)[..3], ["TurbineHub.flatpak", "NonSteamLaunchers-install.log", "heroic-save-backup.zip"].map(String::from));
    let crumbs: Vec<&str> = j.get("crumbs").unwrap().as_array().iter().filter_map(|c| c.str_at("label")).collect();
    assert_eq!(crumbs, ["This PC", "SteamOS (C:)", "Users", "deck", "Downloads"]);
    assert_eq!((j.str_at("typed"), j.get("status").unwrap().str_at("real"), j.get("status").unwrap().str_at("count")), (Some("C:\\Users\\deck\\Downloads"), Some(DOWNLOADS), Some("9 items")));
    let f = row(&j, "TurbineHub.flatpak");
    assert_eq!((f.str_at("type"), f.str_at("size"), f.str_at("icon"), f.str_at("o")), (Some("App installer"), Some("3.2 MB"), Some("zip"), Some("xcrdapi")));
    assert!(f.str_at("date").unwrap().starts_with("Today "));
    // A click on a column sorts by it and ends the groups; again turns it round.
    act(&mut u, "files-sort", "name", "");
    let j = page(&mut u);
    assert_eq!((j.str_at("sort"), j.get("desc").and_then(Json::as_bool), j.str_at("group")), (Some("name"), Some(false), Some("none")));
    assert_eq!(names(&j)[0], "bg3-mod-fixer.zip");
    act(&mut u, "files-sort", "name", "");
    assert_eq!(names(&page(&mut u))[0], "wallpaper-jade.png");
    act(&mut u, "files-sort", "size", "");
    assert_eq!(names(&page(&mut u))[0], "trailer-capture.mp4");
    act(&mut u, "files-group", "type", "");
    assert!(groups(&page(&mut u)).contains(&"Zip archive".to_string()));
    for layout in ["icons", "list", "details"] {
        act(&mut u, "files-layout", layout, "");
        assert_eq!(page(&mut u).str_at("layout"), Some(layout));
    }
    // The made-up places carry their line of help and the real folders behind them.
    let j = go(&mut u, "games");
    assert!(j.str_at("note").unwrap().starts_with("Every installed game"));
    assert!(names(&j).contains(&"Baldur's Gate 3".to_string()) && names(&j).contains(&"Valheim".to_string()));
    assert!(j.get("status").unwrap().str_at("real").unwrap().contains("/run/media/deck/sdcard/steamapps/common"));
    let j = go(&mut u, "game:steam:1086940");
    let mut parts = names(&j);
    parts.sort();
    assert_eq!(parts, ["Game files", "Its Windows C: drive", "Save snapshots"]);
    assert_eq!(j.str_at("typed"), Some("C:\\Games\\Baldur's Gate 3"));
    let j = go(&mut u, "windows");
    assert!(!j.str_at("locked").unwrap().is_empty() && j.get("canAdd").and_then(Json::as_bool) == Some(false));
    assert!(names(&j).contains(&"bin".to_string()) && row(&j, "bin").get("locked").and_then(Json::as_bool) == Some(true));
    // A locked line can be copied and looked at, never cut, renamed or deleted.
    assert_eq!(row(&j, "bin").str_at("o"), Some("ctapqi"));
    done("list");
}

#[test]
fn back_forward_up_and_a_typed_path_lead_where_they_say() {
    let mut u = ui("nav", Mode::Guided);
    let j = go(&mut u, DOWNLOADS);
    assert_eq!((j.get("canBack").and_then(Json::as_bool), j.get("canFwd").and_then(Json::as_bool), j.get("canUp").and_then(Json::as_bool)), (Some(true), Some(false), Some(true)));
    // Up: the folder just left keeps the highlight.
    act(&mut u, "files-up", "", "");
    let j = page(&mut u);
    assert_eq!((j.str_at("place"), j.str_at("focus")), (Some(HOME), Some("row:/home/deck/Downloads")));
    assert_eq!(page(&mut u).str_at("focus"), Some(""), "the highlight is asked for once");
    act(&mut u, "files-back", "", "");
    assert_eq!(page(&mut u).str_at("place"), Some(DOWNLOADS));
    act(&mut u, "files-back", "", "");
    let j = page(&mut u);
    assert_eq!((j.str_at("place"), j.get("canFwd").and_then(Json::as_bool)), (Some("home"), Some(true)));
    act(&mut u, "files-fwd", "", "");
    assert_eq!(page(&mut u).str_at("place"), Some(DOWNLOADS));
    // The address takes both kinds of path, and a file's path opens its folder with the file highlighted.
    for (typed, place, focus) in [("c:\\users\\deck\\pictures", PICTURES, ""), ("/home/deck/Documents", DOCUMENTS, ""), ("C:\\Games", "games", ""), ("~/Downloads/wallpaper-jade.png", DOWNLOADS, "row:/home/deck/Downloads/wallpaper-jade.png"), ("D:\\", "/run/media/deck/sdcard", ""), ("Recycle Bin", "trash", "")] {
        assert_eq!(act(&mut u, "files-path", "", typed).get("refresh").and_then(Json::as_bool), Some(true), "{typed}");
        let j = page(&mut u);
        assert_eq!((j.str_at("place"), j.str_at("focus")), (Some(place), Some(focus)), "{typed}");
    }
    assert!(toast(&act(&mut u, "files-path", "", "C:\\Nowhere\\at all")).starts_with("Can't find"));
    assert_eq!(page(&mut u).str_at("place"), Some("trash"));
    // Tabs: each keeps its own place.
    act(&mut u, "files-go", PICTURES, "newtab");
    let j = page(&mut u);
    let titles: Vec<(&str, bool)> = j.get("tabs").unwrap().as_array().iter().map(|t| (t.str_at("title").unwrap(), t.get("on").unwrap().truthy())).collect();
    assert_eq!(titles, [("Recycle Bin", false), ("Pictures", true)]);
    act(&mut u, "files-tab", "prev", "");
    assert_eq!(page(&mut u).str_at("place"), Some("trash"));
    act(&mut u, "files-tab", "new", "");
    assert_eq!((page(&mut u).str_at("place"), page(&mut u).get("tabs").unwrap().as_array().len()), (Some("home"), 3));
    act(&mut u, "files-tab", "close:0", "");
    act(&mut u, "files-tab", "close", "");
    let j = page(&mut u);
    assert_eq!((j.get("tabs").unwrap().as_array().len(), j.str_at("place")), (1, Some(PICTURES)));
    assert_eq!(toast(&act(&mut u, "files-tab", "close", "")), "The last tab stays open.");
    // A link into Files from another page.
    assert_eq!(act(&mut u, "files-show", "pc", "").str_at("open"), Some("files"));
    assert_eq!(page(&mut u).str_at("place"), Some("pc"));
    done("nav");
}

#[test]
fn new_rename_delete_and_undo_go_through_the_engine() {
    let mut u = ui("ops", Mode::Guided);
    go(&mut u, DOCUMENTS);
    // A new folder gets a free name and is named right away.
    let a = act(&mut u, "files-new", "folder", "");
    assert_eq!((a.str_at("rename"), toast(&a).is_empty()), (Some("/home/deck/Documents/New folder"), false));
    let j = page(&mut u);
    assert_eq!(j.str_at("focus"), Some("row:/home/deck/Documents/New folder"));
    assert!(u.ctx.home.join("Documents/New folder").is_dir());
    assert_eq!(act(&mut u, "files-new", "folder", "").str_at("rename"), Some("/home/deck/Documents/New folder (2)"));
    act(&mut u, "files-rename", "/home/deck/Documents/New folder", "Mods");
    let j = page(&mut u);
    assert!(names(&j).contains(&"Mods".to_string()) && !names(&j).contains(&"New folder".to_string()));
    assert_eq!(j.str_at("undo"), Some("Undo renaming New folder to Mods"));
    // A name that can't be: said in plain words, nothing changed.
    assert!(!toast(&act(&mut u, "files-rename", "/home/deck/Documents/Mods", "a/b")).is_empty());
    assert!(u.ctx.home.join("Documents/Mods").is_dir());
    // A text file and a shortcut.
    assert_eq!(act(&mut u, "files-new", "text", "").str_at("rename"), Some("/home/deck/Documents/New text file.txt"));
    assert_eq!(act(&mut u, "files-new", "shortcut", "").get("dialog").unwrap().get("input").unwrap().str_at("label"), Some("Where it leads"));
    act(&mut u, "files-new", "shortcut", "C:\\Games");
    assert!(std::fs::read_to_string(u.ctx.home.join("Documents/Shortcut to Games.desktop")).unwrap().contains("[Desktop Entry]"));
    // Delete goes to the Recycle Bin without a question, and Undo brings it back.
    let notes = "/home/deck/Documents/Controller layout notes.txt";
    let a = act(&mut u, "files-delete", "", &list(&[notes, "/home/deck/Documents/Mods"]));
    assert!(a.get("dialog").is_none() && a.get("refresh").is_some());
    let j = page(&mut u);
    assert!(!names(&j).contains(&"Mods".to_string()) && j.str_at("undo") == Some("Undo deleting 2 items"));
    let bin = go(&mut u, "trash");
    assert!(names(&bin).contains(&"Controller layout notes.txt".to_string()));
    assert_eq!((bin.str_at("kind"), row(&bin, "Mods").str_at("o"), row(&bin, "Mods").str_at("type")), (Some("bin"), Some("si"), Some("C:\\Users\\deck\\Documents")));
    assert!(bin.get("options").unwrap().as_array().iter().any(|o| o.str_at("title") == Some("Empty the Recycle Bin") && o.get("button").is_some_and(Json::truthy)));
    act(&mut u, "files-undo", "", "");
    assert!(u.ctx.home.join("Documents/Mods").is_dir() && u.ctx.home.join("Documents/Controller layout notes.txt").is_file());
    // Out of the bin by Restore, too.
    act(&mut u, "files-delete", notes, "");
    let bin = go(&mut u, "trash");
    let at = row(&bin, "Controller layout notes.txt").str_at("path").unwrap().to_string();
    act(&mut u, "files-restore", "", &list(&[&at]));
    assert!(u.ctx.home.join("Documents/Controller layout notes.txt").is_file());
    // What is in the bin isn't deleted from there, and emptying asks with Cancel first.
    assert_eq!(toast(&act(&mut u, "files-delete", "/home/deck/.local/share/Trash/files/old-screenshot.png", "")), crate::files::guard::BIN_WHY);
    let d = act(&mut u, "files-empty", "", "");
    let d = d.get("dialog").unwrap();
    assert_eq!((d.get("danger").and_then(Json::as_bool), d.str_at("confirm"), d.get("then").unwrap().str_at("act")), (Some(true), Some("Empty"), Some("files-run")));
    assert!(u.ctx.home.join(".local/share/Trash/files/old-screenshot.png").exists(), "nothing goes before the answer");
    act(&mut u, "files-run", "", "");
    assert!(names(&go(&mut u, "trash")).is_empty());
    assert_eq!(page(&mut u).str_at("empty"), Some("The Recycle Bin is empty."));
    // SteamOS itself and Steam's own folders are never changed: the engine's own words come back as a note.
    assert_eq!(toast(&act(&mut u, "files-delete", "/usr/bin/bash", "")), crate::files::guard::SYSTEM_WHY);
    assert_eq!(toast(&act(&mut u, "files-rename", "/home/deck/.local/share/Steam/steamapps", "x")), crate::files::guard::STEAM_WHY);
    go(&mut u, "games");
    assert!(toast(&act(&mut u, "files-new", "folder", "")).contains("Open one first"));
    done("ops");
}

#[test]
fn copy_and_move_ask_about_a_name_that_is_taken() {
    let mut u = ui("clip", Mode::Guided);
    go(&mut u, DOWNLOADS);
    let wall = "/home/deck/Downloads/wallpaper-jade.png";
    let notes = "/home/deck/Downloads/ProtonGE-notes.txt";
    assert!(toast(&act(&mut u, "files-copy", "", &list(&[wall, notes]))).starts_with("Copied 2 items"));
    let j = page(&mut u);
    assert_eq!((j.get("clip").unwrap().get("count").and_then(Json::as_f64), j.get("clip").unwrap().get("cut").and_then(Json::as_bool)), (Some(2.0), Some(false)));
    assert!(j.get("options").unwrap().as_array().iter().any(|o| o.str_at("title") == Some("Paste 2 items") && o.str_at("key") == Some("ctrl+v")));
    // Pictures has a wallpaper-jade.png already: the dialog asks, and nothing is copied before the answer.
    go(&mut u, PICTURES);
    let d = act(&mut u, "files-paste", "", "");
    let d = d.get("dialog").unwrap();
    assert_eq!(d.str_at("title"), Some("wallpaper-jade.png is already there"));
    let choices: Vec<&str> = d.get("rows").unwrap().as_array().iter().filter_map(|r| r.str_at("arg")).collect();
    assert_eq!(choices, ["skip", "replace", "both"]);
    assert!(!u.ctx.home.join("Pictures/ProtonGE-notes.txt").exists());
    // Keep both: the new one gets a number, the other file comes along.
    act(&mut u, "files-answer", "both", "");
    let j = page(&mut u);
    assert!(names(&j).contains(&"wallpaper-jade (2).png".to_string()) && names(&j).contains(&"ProtonGE-notes.txt".to_string()));
    assert_eq!(j.str_at("undo"), Some("Undo copying 2 items"));
    act(&mut u, "files-undo", "", "");
    assert!(!u.ctx.home.join("Pictures/ProtonGE-notes.txt").exists() && !u.ctx.home.join("Pictures/wallpaper-jade (2).png").exists());
    // Two names taken: "for the others too" answers both with one press.
    std::fs::write(u.ctx.home.join("Pictures/ProtonGE-notes.txt"), "old").unwrap();
    let d = act(&mut u, "files-paste", "", "");
    assert!(d.get("dialog").unwrap().get("rows").unwrap().as_array().iter().any(|r| r.str_at("title") == Some("Do this for the other 1 too: no")));
    let d = act(&mut u, "files-answer", "all", "");
    assert!(d.get("dialog").unwrap().get("rows").unwrap().as_array().iter().any(|r| r.str_at("title") == Some("Do this for the other 1 too: yes")));
    assert!(act(&mut u, "files-answer", "replace", "").get("dialog").is_none());
    assert!(std::fs::read_to_string(u.ctx.home.join("Pictures/ProtonGE-notes.txt")).unwrap().starts_with("GE-Proton"));
    // What was replaced went to the Recycle Bin.
    assert!(names(&go(&mut u, "trash")).contains(&"ProtonGE-notes.txt".to_string()));
    // Skip leaves everything as it is.
    go(&mut u, PICTURES);
    act(&mut u, "files-copy", wall, "");
    act(&mut u, "files-paste", "", "");
    let a = act(&mut u, "files-answer", "skip", "");
    assert!(a.get("dialog").is_none() && !toast(&a).is_empty());
    // Cut and paste moves, and the cut is used up; a drop moves too, and with Ctrl it copies.
    act(&mut u, "files-cut", "/home/deck/Downloads/Handheld-manual.pdf", "");
    assert_eq!(row(&go(&mut u, DOWNLOADS), "Handheld-manual.pdf").get("dim").and_then(Json::as_bool), Some(true));
    go(&mut u, DOCUMENTS);
    act(&mut u, "files-paste", "", "");
    let j = page(&mut u);
    assert!(names(&j).contains(&"Handheld-manual.pdf".to_string()) && j.get("clip").unwrap().get("count").and_then(Json::as_f64) == Some(0.0));
    assert!(!u.ctx.home.join("Downloads/Handheld-manual.pdf").exists());
    act(&mut u, "files-drop", "move|/home/deck/Downloads", &list(&["/home/deck/Documents/Handheld-manual.pdf"]));
    assert!(u.ctx.home.join("Downloads/Handheld-manual.pdf").exists() && !u.ctx.home.join("Documents/Handheld-manual.pdf").exists());
    assert!(act(&mut u, "files-drop", "move|/home/deck/Downloads", &list(&["/home/deck/Downloads/Handheld-manual.pdf"])).entries().is_empty(), "dropped where it is");
    act(&mut u, "files-drop", "copy|/home/deck/Music", &list(&["/home/deck/Downloads/Handheld-manual.pdf"]));
    assert!(u.ctx.home.join("Downloads/Handheld-manual.pdf").exists() && u.ctx.home.join("Music/Handheld-manual.pdf").exists());
    // Nothing is ever dropped into SteamOS itself.
    assert_eq!(toast(&act(&mut u, "files-drop", "copy|/usr/bin", &list(&[wall]))), crate::files::guard::SYSTEM_WHY);
    done("clip");
}

#[test]
fn search_pins_paths_and_properties() {
    let mut u = ui("more", Mode::Guided);
    go(&mut u, DOWNLOADS);
    act(&mut u, "search", "files", "zip");
    let j = page(&mut u);
    assert_eq!(names(&j), ["heroic-save-backup.zip", "bg3-mod-fixer.zip"]);
    assert_eq!((j.get("search").unwrap().str_at("value"), j.get("search").unwrap().str_at("hint"), j.get("canDeep").and_then(Json::as_bool)), (Some("zip"), Some("Search Downloads"), Some(true)));
    act(&mut u, "search", "files", "zelda");
    assert_eq!(page(&mut u).str_at("empty"), Some("Nothing here has \"zelda\" in its name."));
    // Below the folder: every hit with where it is.
    go(&mut u, HOME);
    act(&mut u, "search", "files", "wallpaper");
    assert!(names(&page(&mut u)).is_empty());
    act(&mut u, "files-deep", "", "");
    let j = page(&mut u);
    let found: Vec<(&str, &str)> = j.get("rows").unwrap().as_array().iter().map(|r| (r.str_at("name").unwrap(), r.str_at("type").unwrap())).collect();
    assert_eq!(found, [("wallpaper-jade.png", "C:\\Users\\deck\\Downloads"), ("wallpaper-jade.png", "C:\\Users\\deck\\Pictures")]);
    assert_eq!((j.get("deep").unwrap().get("running").and_then(Json::as_bool), j.get("columns").unwrap().str_at("type")), (Some(false), Some("Folder")));
    act(&mut u, "search", "files", "");
    assert!(page(&mut u).get("deep") == Some(&Json::Null));
    // Pins: off, on again at the end, and up.
    let shots = "/home/deck/Pictures/Screenshots";
    assert!(toast(&act(&mut u, "files-pin", shots, "")).contains("pinned"));
    assert!(toast(&act(&mut u, "files-pin", shots, "")).contains("already"));
    act(&mut u, "files-unpin", "/home/deck/Music", "");
    act(&mut u, "files-pin-move", shots, "up");
    let j = page(&mut u);
    let pins: Vec<&str> = j.get("nav").unwrap().as_array().iter().filter(|n| n.get("pinned").is_some_and(Json::truthy)).filter_map(|n| n.str_at("label")).collect();
    assert_eq!(pins, ["Desktop", "Downloads", "Documents", "Pictures", "Screenshots", "Videos"]);
    assert!(u.folder().join(crate::files::quick::FILE).exists());
    assert_eq!(row(&go(&mut u, PICTURES), "Screenshots").str_at("o"), Some("xcrdtapui"));
    // Copy as path hands out both forms.
    let wall = "/home/deck/Downloads/wallpaper-jade.png";
    assert_eq!(act(&mut u, "files-copy-path", wall, "windows").str_at("copy"), Some("\"C:\\Users\\deck\\Downloads\\wallpaper-jade.png\""));
    assert_eq!(act(&mut u, "files-copy-path", wall, "real").str_at("copy"), Some(wall));
    // Properties and the details pane.
    let d = act(&mut u, "files-props", wall, "");
    let d = d.get("dialog").unwrap();
    let items: Vec<(&str, &str)> = d.get("items").unwrap().as_array().iter().map(|i| (i.str_at("title").unwrap(), i.str_at("line").unwrap())).collect();
    assert_eq!((d.str_at("title"), items[0], items[1], items[2]), (Some("wallpaper-jade.png"), ("Type", "Picture"), ("Where it is", "C:\\Users\\deck\\Downloads\\wallpaper-jade.png"), ("Real path", wall)));
    assert!(!items.iter().any(|(t, _)| *t == "Permissions"));
    let p = act(&mut u, "files-peek", wall, "");
    let p = p.get("peek").unwrap();
    assert_eq!((p.str_at("kind"), p.str_at("name")), (Some("picture"), Some("wallpaper-jade.png")));
    assert!(std::path::Path::new(p.str_at("load").unwrap()).is_file(), "the picture is loaded from where the app finds it");
    let p = act(&mut u, "files-peek", "/home/deck/Downloads/ProtonGE-notes.txt", "");
    assert!(p.get("peek").unwrap().str_at("text").unwrap().starts_with("GE-Proton10-4"));
    let p = act(&mut u, "files-peek", PICTURES, "");
    assert!(p.get("peek").unwrap().get("lines").unwrap().as_array().iter().any(|l| l.str_at("title") == Some("Holds") && l.str_at("line") == Some("3 files, 1 folder")));
    done("more");
}

#[test]
fn a_file_opens_inside_the_app_or_with_its_own_app_in_desktop_mode() {
    let mut u = ui("open", Mode::Guided);
    go(&mut u, DOWNLOADS);
    // A folder is gone into.
    act(&mut u, "files-open", PICTURES, PICTURES);
    assert_eq!(page(&mut u).str_at("place"), Some(PICTURES));
    // A text and a picture show inside the app, and land on Home under Recent.
    act(&mut u, "files-open", "/home/deck/Downloads/ProtonGE-notes.txt", "");
    let j = page(&mut u);
    let v = j.get("viewer").unwrap();
    assert_eq!((v.str_at("kind"), v.str_at("name")), (Some("text"), Some("ProtonGE-notes.txt")));
    assert!(v.str_at("text").unwrap().contains("Keep Proton 11"));
    act(&mut u, "files-close-view", "", "");
    assert!(page(&mut u).get("viewer") == Some(&Json::Null));
    act(&mut u, "files-open", "/home/deck/Downloads/wallpaper-jade.png", "");
    assert_eq!(page(&mut u).get("viewer").unwrap().str_at("kind"), Some("picture"));
    let home = go(&mut u, "home");
    assert_eq!(tiles(&home)[1], ("Recent".to_string(), vec!["wallpaper-jade.png".to_string(), "ProtonGE-notes.txt".to_string()]));
    // Anything else: in Game Mode the page says where it opens, with the real path to copy.
    let video = "/home/deck/Downloads/trailer-capture.mp4";
    let d = act(&mut u, "files-open", video, "");
    let d = d.get("dialog").unwrap();
    assert_eq!((d.str_at("title"), d.str_at("confirm")), (Some("trailer-capture.mp4 opens in Desktop Mode"), Some("Copy real path")));
    assert_eq!((d.get("then").unwrap().str_at("act"), d.get("then").unwrap().str_at("arg"), d.get("then").unwrap().str_at("value")), (Some("files-copy-path"), Some(video), Some("real")));
    assert!(toast(&act(&mut u, "files-open-with", video, "")).contains("Desktop Mode"));
    // In a Desktop Mode window it opens with its own app on the host, and Open with asks the system.
    u.window = true;
    let a = act(&mut u, "files-open", video, "");
    let line: Vec<&str> = a.get("run").unwrap().as_array().iter().filter_map(Json::as_str).collect();
    assert_eq!(&line[line.len() - 2..], ["xdg-open", video]);
    assert!(act(&mut u, "files-open-with", video, "").str_at("openWith").unwrap().ends_with("/home/deck/Downloads/trailer-capture.mp4"));
    assert!(row(&go(&mut u, DOWNLOADS), "trailer-capture.mp4").str_at("o").unwrap().contains('w'));
    // What is in the Recycle Bin opens only after it is put back.
    go(&mut u, "trash");
    assert_eq!(toast(&act(&mut u, "files-open", "/home/deck/.local/share/Trash/files/old-screenshot.png", "")), crate::files::guard::BIN_WHY);
    done("open");
}

#[test]
fn the_page_offers_its_options_through_the_shared_controls() {
    let mut u = ui("options", Mode::Guided);
    let j = go(&mut u, DOWNLOADS);
    let titles: Vec<&str> = j.get("options").unwrap().as_array().iter().filter_map(|o| o.str_at("title")).collect();
    assert_eq!(titles, ["New", "Select all", "Sort and view", "Type a path", "New tab", "Refresh", "Select", "Find", "Help for this page"]);
    let hints: Vec<(&str, &str)> = j.get("hints").unwrap().as_array().iter().map(|h| (h.str_at("k").unwrap(), h.str_at("label").unwrap())).collect();
    assert_eq!(hints, [("start", "Select"), ("y", "Find"), ("x", "Options"), ("view", "Menu")]);
    // The item menu: the four icons first, as on Windows 11, then the rest. No Share.
    let io = j.get("itemOptions").unwrap();
    let strip: Vec<&str> = io.entries().iter().filter(|(_, o)| o.get("strip").is_some_and(Json::truthy)).filter_map(|(_, o)| o.str_at("title")).collect();
    assert_eq!(strip, ["Cut", "Copy", "Rename", "Delete"]);
    assert!(!j.to_text().contains("Share"));
    for letter in "xcrdwtapquUDsi".chars() {
        assert!(io.get(&letter.to_string()).is_some(), "{letter}");
    }
    // The two small menus.
    let new: Vec<String> = act(&mut u, "files-menu", "new", "").get("dialog").unwrap().get("rows").unwrap().as_array().iter().filter_map(|r| r.str_at("title")).map(String::from).collect();
    assert_eq!(new, ["Folder", "Text file", "Shortcut"]);
    let view = act(&mut u, "files-menu", "view", "");
    let view: Vec<&str> = view.get("dialog").unwrap().get("rows").unwrap().as_array().iter().filter_map(|r| r.str_at("title")).collect();
    assert!(view.contains(&"Sort by date (now)") && view.contains(&"Group by date (now)") && view.contains(&"Details (now)") && view.contains(&"Hidden files: show") && view.contains(&"Switch to True Linux"));
    // Help is about Files, and nothing in Guided's own words is jargon or has a long dash.
    let help = parse(&u.page("help", "files@")).unwrap();
    assert!(help.str_at("intro").unwrap().contains("Windows"));
    for place in ["home", "pc", "/", "games", "game:steam:1086940", "programs", "appdata", "windows", "trash"] {
        let mut j = go(&mut u, place);
        // (File names are the player's own; the page's words are the ones held to the rules.)
        j = with(with(j, "rows", arr([])), "sections", arr([]));
        let mut all = Vec::new();
        strings(&j, &mut all);
        for t in all.iter().filter(|t| !t.contains('/') && !t.contains("com.")) {
            assert!(!t.contains(['\u{2013}', '\u{2014}']), "{place}: dash in {t:?}");
            assert!(crate::glossary::jargon(t).is_empty(), "{place}: jargon in {t:?}");
        }
    }
    done("options");
}

#[test]
fn a_long_copy_runs_in_the_background_and_reports_to_the_status_bar() {
    let mut u = ui("job", Mode::Guided);
    u.files.always_background = true;
    go(&mut u, DOCUMENTS);
    act(&mut u, "files-copy", "/home/deck/Downloads/ProtonGE-notes.txt", "");
    let a = act(&mut u, "files-paste", "", "");
    assert!(toast(&a).starts_with("Copying in the background"));
    // While it runs the page shows it, offers to stop it, and a second copy waits its turn.
    let j = page(&mut u);
    if j.get("job") != Some(&Json::Null) {
        assert!(j.get("job").unwrap().str_at("words").unwrap().starts_with("Copying"));
        assert!(j.get("options").unwrap().as_array().iter().any(|o| o.str_at("act") == Some("files-job-stop")));
    }
    // The window asks until the end comes: one note, then nothing more to say.
    let mut end = None;
    for _ in 0..2000 {
        match u.files_tick() {
            Some(news) if news.contains("\"done\"") => {
                end = Some(parse(&news).unwrap());
                break;
            }
            _ => std::thread::sleep(std::time::Duration::from_millis(2)),
        }
    }
    let end = end.expect("the copy never ended");
    assert!(!toast(&end).is_empty());
    assert!(u.files_tick().is_none());
    let j = page(&mut u);
    assert!(j.get("job") == Some(&Json::Null) && names(&j).contains(&"ProtonGE-notes.txt".to_string()));
    assert_eq!((j.str_at("undo"), j.str_at("focus")), (Some("Undo copying 1 item"), Some("row:/home/deck/Documents/ProtonGE-notes.txt")));
    act(&mut u, "files-undo", "", "");
    assert!(!u.ctx.home.join("Documents/ProtonGE-notes.txt").exists());
    done("job");
}

#[test]
fn turbine_files_is_a_library_entry_of_its_own() {
    use super::plan::Step;
    use super::selftile::{self, State};
    use crate::shortcuts::Steam;
    let mut u = ui("tile", Mode::Guided);
    let (home, folder) = (u.ctx.home.clone(), u.folder());
    // Art to place: the set the bundle carries.
    let share = dir("tile").join("share");
    for part in ["capsule", "wide", "hero", "logo", "icon"] {
        for set in ["steam-art", "steam-art-files"] {
            std::fs::create_dir_all(share.join(set)).unwrap();
            std::fs::write(share.join(set).join(format!("{part}.png")), b"\x89PNG\r\n\x1a\n").unwrap();
        }
    }
    // Settings offers it; the dialog says what is added; while Steam runs the player gets the steps instead.
    let settings = parse(&u.page("settings", "")).unwrap().to_text();
    assert!(settings.contains(selftile::FILES_TITLE));
    let d = act(&mut u, "files-tile", "add", "");
    assert_eq!(d.get("dialog").unwrap().str_at("title"), Some("Add Turbine Files to your library?"));
    assert_eq!(u.confirm().unwrap().steps, vec![Step::FilesTile { add: true }]);
    u.steam_running = true;
    assert_eq!(act(&mut u, "files-tile", "add", "").get("dialog").unwrap().str_at("title"), Some("Steam has to be closed for this"));
    u.steam_running = false;
    // Added, it is never taken for Turbine Hub's own entry, and each is removed by itself.
    assert!(!selftile::files_added(&home));
    selftile::run_step(&Step::FilesTile { add: true }, &home, &folder, &share, Steam::Closed).unwrap().unwrap();
    assert!(selftile::files_added(&home));
    assert_eq!(selftile::state(&home), State::Missing);
    assert!(crate::shortcuts::find_self(&home).is_none());
    assert!(selftile::files_tile().launch_options.ends_with("io.github.ravenor907.TurbineHub --page=files"));
    assert!(toast(&act(&mut u, "files-tile", "add", "")).contains("already"));
    selftile::run_step(&Step::SelfTile { add: true }, &home, &folder, &share, Steam::Closed).unwrap().unwrap();
    assert!(matches!(selftile::state(&home), State::Added { ours: true, .. }));
    selftile::run_step(&Step::FilesTile { add: false }, &home, &folder, &share, Steam::Closed).unwrap().unwrap();
    assert!(!selftile::files_added(&home));
    assert!(matches!(selftile::state(&home), State::Added { ours: true, .. }));
    selftile::run_step(&Step::FilesTile { add: true }, &home, &folder, &share, Steam::Closed).unwrap().unwrap();
    selftile::run_step(&Step::SelfTile { add: false }, &home, &folder, &share, Steam::Closed).unwrap().unwrap();
    assert!(selftile::files_added(&home) && selftile::state(&home) == State::Missing);
    done("tile");
}

#[test]
fn dates_read_the_way_the_list_shows_them() {
    // Thursday 12 Nov 2026, 10:31 on a clock one hour ahead of UTC.
    let now = When { secs: 1_794_475_865, offset: 3600 };
    let day = 86_400;
    assert_eq!(files::date_words(now, now.secs - 3600), "Today 09:31");
    assert_eq!(files::date_words(now, now.secs - day), "Yesterday 10:31");
    assert_eq!(files::date_words(now, now.secs - 4 * day), "Sun 8 Nov");
    assert_eq!(files::date_words(now, now.secs - 15 * day), "28 Oct");
    assert_eq!(files::date_words(now, now.secs - 400 * day), "8 Oct 2025");
    // Just after midnight, a minute ago is yesterday.
    let late = When { secs: 1_794_438_030, offset: 3600 };
    assert_eq!(files::date_words(late, late.secs - 60), "Yesterday 23:59");
}
