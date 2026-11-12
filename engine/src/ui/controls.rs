//! The controls every page shares, decided in one place. The scheme is Turbine Mod Manager's: A selects, B goes
//! back, X opens Options (a menu for what the highlight is on), Y is Find, LB and RB turn the tabs, View opens the
//! menu, Start presses the page's main button and plays a game when held, the right stick is a pointer and R3 its
//! click. A keyboard and a mouse have a way to do each of them.
//!
//! A page only says what it offers: `options` (its own actions), `primary` (its main button), `search` (it has a
//! list to find in) and `hold` (what holding Start does). `finish` turns that into the Options menu's rows and the
//! footer's hints, for every page alike; no page writes a hint or names a button itself.
use super::{Ui, arr, b, obj, s, with};
use crate::json::Json;

/// One line of the control scheme: what it is called, how it is done on a controller, with a keyboard and with a
/// mouse ("" where there is no way of its own), and what it does. Help shows these, and the README's and the design
/// document's tables are held to them by a test.
pub struct Control {
    pub name: &'static str,
    pub pad: &'static str,
    pub keys: &'static str,
    pub mouse: &'static str,
    pub does: &'static str,
}

pub const SCHEME: &[Control] = &[
    Control { name: "Move", pad: "D-pad or left stick", keys: "Arrows or W A S D", mouse: "Move the mouse", does: "Moves the highlight." },
    Control { name: "Select", pad: "A", keys: "Enter or Space", mouse: "Click", does: "Picks what the highlight is on: opens it, turns it on or off, or presses its button." },
    Control { name: "Back", pad: "B", keys: "Esc or Backspace", mouse: "Back side button", does: "Back to where you were. It also empties a search and ends typing." },
    Control { name: "Options", pad: "X", keys: "R", mouse: "Right-click", does: "A menu of everything the highlighted thing and this page can do." },
    Control { name: "Find", pad: "Y", keys: "F, Ctrl+F or /", mouse: "Click the search box", does: "Jumps to the search box of a page that has a list. Typing letters on a keyboard finds too." },
    Control { name: "Tabs", pad: "LB / RB", keys: "Q / E, or Page Up / Page Down", mouse: "Click a tab", does: "Home, Games, Store, My machine, Settings." },
    Control { name: "Menu", pad: "View", keys: "Tab", mouse: "Click Menu in the footer", does: "Help, Where's my...?, Bring it over, Review changes, Settings, Close." },
    Control { name: "Main button", pad: "Start", keys: "Ctrl+Enter", mouse: "Click the button", does: "Presses the page's main button (Continue, Apply, Copy). Without one it opens Settings." },
    Control { name: "Play", pad: "Hold Start", keys: "Hold Space", mouse: "Click Play", does: "Starts the highlighted game, from the Games tab." },
    Control { name: "Jump", pad: "LT / RT", keys: "Home / End", mouse: "Wheel", does: "LT and RT jump a screenful up and down; Home and End go to the first and the last; the wheel scrolls." },
    Control { name: "Pointer", pad: "Right stick", keys: "", mouse: "", does: "Moves a pointer drawn by Turbine Hub. It hides after a moment and when the D-pad is used." },
    Control { name: "Click", pad: "R3", keys: "", mouse: "Click", does: "Presses what is under the pointer. With the pointer hidden it is A." },
    Control { name: "Pointer options", pad: "Hold R3", keys: "", mouse: "Right-click", does: "Options for what is under the pointer. LT does the same while the pointer shows, and RT holds the button down to drag a slider." },
    Control { name: "Scroll", pad: "Sweep the left stick round", keys: "", mouse: "Wheel", does: "Circling the left stick turns a scroll wheel: clockwise scrolls down." },
    Control { name: "Help", pad: "X, then Help", keys: "F1", mouse: "Right-click, then Help", does: "What this page is and what its buttons do." },
    Control { name: "Size", pad: "", keys: "Ctrl+= / Ctrl+-", mouse: "Ctrl and the wheel", does: "Makes everything bigger or smaller." },
];

/// One entry of an Options menu: its words and the press it makes.
pub(crate) fn option(title: &str, act: &str, arg: &str) -> Json {
    obj! {"title": s(title), "act": s(act), "arg": s(arg)}
}

/// A page action that the page also draws as a button, so it is one press away with a mouse or a finger too.
pub(crate) fn page_button(title: &str, act: &str, arg: &str) -> Json {
    with(option(title, act, arg), "button", b(true))
}

/// An entry the window itself carries out: "find" (the search box), "reload" (the page again).
pub(crate) fn local(title: &str, what: &str, key: &str) -> Json {
    obj! {"title": s(title), "act": s(""), "arg": s(""), "local": s(what), "key": s(key)}
}

fn list(j: &Json, key: &str) -> Vec<Json> {
    j.get(key).map(Json::as_array).unwrap_or(&[]).to_vec()
}

fn same(a: &Json, b: &Json) -> bool {
    a.str_at("act") == b.str_at("act") && a.str_at("arg") == b.str_at("arg") && a.str_at("local") == b.str_at("local")
}

fn push(out: &mut Vec<Json>, o: Json) {
    if !out.iter().any(|x| same(x, &o)) {
        out.push(o);
    }
}

/// What a row offers as an Options entry: a button's press, a switch's turn. Sliders and choices need the row
/// itself (their value is picked on it), and words to read offer nothing.
fn row_option(r: &Json) -> Option<Json> {
    let act = r.str_at("act").unwrap_or("");
    if act.is_empty() || act == "none" {
        return None;
    }
    let title = r.str_at("title").unwrap_or("");
    let on = r.get("on").is_some_and(Json::truthy);
    let words = match r.str_at("kind").unwrap_or("") {
        "button" => {
            let button = r.str_at("button").unwrap_or("");
            let said = button.is_empty() || button == "Open" || title.to_lowercase().contains(&button.to_lowercase());
            if said { title.to_string() } else { format!("{title}: {button}") }
        }
        "toggle" => format!("{title}: turn {}", if on { "off" } else { "on" }),
        "check" => format!("{title}: {}", if on { "untick" } else { "tick" }),
        _ => return None,
    };
    Some(option(&words, act, r.str_at("arg").unwrap_or("")))
}

/// Everything a page offers, as Options entries: its rows' presses in the order they show, then its own actions
/// and its main button. This is what an entry that opens the page lists (a game, a card, a Store row).
pub(crate) fn actions_of(page: &Json) -> Vec<Json> {
    let mut out = Vec::new();
    for sec in list(page, "sections") {
        for r in list(&sec, "rows") {
            if let Some(o) = row_option(&r) {
                push(&mut out, o);
            }
        }
    }
    for o in list(page, "options") {
        push(&mut out, with(o, "button", b(false)));
    }
    if let Some(p) = page.get("primary").filter(|p| p.str_at("act").is_some()) {
        push(&mut out, option(p.str_at("label").unwrap_or(""), p.str_at("act").unwrap_or(""), p.str_at("arg").unwrap_or("")));
    }
    out.truncate(MOST);
    out
}

/// An Options menu stays a menu: at most this many entries come from the page an entry opens.
const MOST: usize = 9;

/// The pages whose entries are worth looking into for an Options menu: one part of the machine, one thing from the
/// Store. Both are built from what is already known, without asking the host anything.
fn looks_into(target: &str) -> Option<(&str, &str)> {
    if target.starts_with("module:") {
        return Some((target, ""));
    }
    target.strip_prefix("store-item|").map(|id| ("store-item", id))
}

/// Gives every row or card that opens a part of the machine or a thing from the Store its `options`: what can be
/// done there, so X on it offers Install or Turn on without going in first.
fn with_item_options(ui: &mut Ui, j: Json) -> Json {
    let give = |ui: &mut Ui, item: Json| -> Json {
        if item.str_at("act") != Some("open") {
            return item;
        }
        let Some((page, arg)) = item.str_at("arg").and_then(looks_into) else { return item };
        let (page, arg) = (page.to_string(), arg.to_string());
        let inside = actions_of(&ui.raw(&page, &arg));
        if inside.is_empty() { item } else { with(item, "offers", arr(inside)) }
    };
    let mut j = j;
    if j.get("cards").is_some() {
        let cards: Vec<Json> = list(&j, "cards").into_iter().map(|c| give(ui, c)).collect();
        j = with(j, "cards", arr(cards));
    }
    if j.get("sections").is_some() {
        let sections: Vec<Json> = list(&j, "sections")
            .into_iter()
            .map(|sec| {
                let rows: Vec<Json> = list(&sec, "rows").into_iter().map(|r| give(ui, r)).collect();
                // The keys keep their order: the page reads the title first.
                match sec {
                    Json::Obj(e) => Json::Obj(e.into_iter().map(|(k, v)| if k == "rows" { (k, arr(rows.clone())) } else { (k, v) }).collect()),
                    other => other,
                }
            })
            .collect();
        j = with(j, "sections", arr(sections));
    }
    j
}

/// The first-run pages: no tabs and no menu until a mode is picked and setup is through.
pub(crate) fn first_run(name: &str) -> bool {
    ["pick", "setup", "quick"].contains(&name)
}

/// A page as the window gets it: its own `options` completed with its main button, Find and Help, the footer's
/// `hints` derived from them, and the options of the entries that open another page.
pub(crate) fn finish(ui: &mut Ui, name: &str, arg: &str, page: Json) -> Json {
    if !matches!(page, Json::Obj(_)) {
        return page;
    }
    let mut j = with_item_options(ui, page);
    let mut options: Vec<Json> = Vec::new();
    for o in list(&j, "options") {
        push(&mut options, o);
    }
    // The first-run steps' buttons are actions of the page too, and so is what the page inside a step offers.
    for bt in list(&j, "buttons") {
        let o = option(bt.str_at("label").unwrap_or(""), bt.str_at("act").unwrap_or(""), bt.str_at("arg").unwrap_or(""));
        push(&mut options, if bt.str_at("k") == Some("start") { with(o, "key", s("start")) } else { o });
    }
    if let Some(inner) = j.str_at("embed").filter(|e| !e.is_empty()).map(String::from) {
        let inside = ui.raw(&inner, "");
        for o in actions_of(&obj! {"options": arr(list(&inside, "options")), "primary": inside.get("primary").cloned().unwrap_or(Json::Null)}) {
            push(&mut options, o);
        }
    }
    // The main button is always Start's.
    let primary = j.get("primary").filter(|p| p.str_at("act").is_some()).cloned();
    if let Some(p) = &primary {
        j = with(j, "primary", with(p.clone(), "k", s("start")));
        push(&mut options, with(option(p.str_at("label").unwrap_or(""), p.str_at("act").unwrap_or(""), p.str_at("arg").unwrap_or("")), "key", s("start")));
    }
    let hold = j.get("hold").filter(|h| h.str_at("act").is_some()).cloned();
    if let Some(h) = &hold {
        let o = with(option(h.str_at("label").unwrap_or(""), h.str_at("act").unwrap_or(""), h.str_at("arg").unwrap_or("")), "key", s("play"));
        // Said once, with its key: the same press may be among the page's own already.
        options.retain(|x| !same(x, &o));
        options.insert(0, o);
    }
    let search = j.get("search").is_some_and(Json::truthy);
    if search {
        push(&mut options, local("Find", "find", "y"));
    }
    match name {
        "wheres" => {}
        "help" => push(&mut options, option("Where's my...?", "open", "wheres")),
        _ => push(&mut options, with(option("Help for this page", "open", &format!("help|{name}@{arg}")), "key", s("f1"))),
    }
    // What the highlighted entry of a two-sided page offers: the presses of what it holds.
    if j.get("side").is_some() && j.get("entry").is_none() && name == "games" {
        let inside = actions_of(&obj! {"sections": arr(list(&j, "sections"))});
        j = with(j, "entry", arr(inside));
    }
    // The footer, most important first (A and B are the window's: they follow the highlight).
    let mut hints: Vec<Json> = Vec::new();
    let start_button = list(&j, "buttons").into_iter().find(|bt| bt.str_at("k") == Some("start"));
    if let Some(p) = primary.as_ref().or(start_button.as_ref()) {
        hints.push(super::hint("start", p.str_at("label").unwrap_or("")));
    } else if let Some(h) = &hold {
        hints.push(with(super::hint("play", h.str_at("label").unwrap_or("")), "hold", b(true)));
    }
    if search {
        hints.push(super::hint("y", "Find"));
    }
    if !options.is_empty() {
        hints.push(super::hint("x", "Options"));
    }
    if !first_run(name) {
        hints.push(super::hint("view", "Menu"));
    }
    j = with(j, "options", arr(options));
    with(j, "hints", arr(hints))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_offers_its_press_in_plain_words() {
        let r = |kind: &str, title: &str, button: &str, on: bool| obj! {"kind": s(kind), "title": s(title), "button": s(button), "on": b(on), "act": s("do"), "arg": s("x")};
        let words = |j: Json| row_option(&j).and_then(|o| o.str_at("title").map(String::from));
        assert_eq!(words(r("button", "Play", "Play", false)).as_deref(), Some("Play"));
        assert_eq!(words(r("button", "Test lights", "Test", false)).as_deref(), Some("Test lights"));
        assert_eq!(words(r("button", "Heroic", "Remove", false)).as_deref(), Some("Heroic: Remove"));
        assert_eq!(words(r("button", "Saves and snapshots", "", false)).as_deref(), Some("Saves and snapshots"));
        assert_eq!(words(r("toggle", "Game Boost", "", false)).as_deref(), Some("Game Boost: turn on"));
        assert_eq!(words(r("toggle", "Game Boost", "", true)).as_deref(), Some("Game Boost: turn off"));
        assert_eq!(words(r("check", "Discord", "", true)).as_deref(), Some("Discord: untick"));
        assert_eq!(words(r("slider", "Brightness", "", false)), None);
        assert_eq!(words(obj! {"kind": s("info"), "title": s("Words"), "act": s("none"), "arg": s("")}), None);
    }

    #[test]
    fn the_scheme_names_every_way_once() {
        for (i, c) in SCHEME.iter().enumerate() {
            assert!(!c.name.is_empty() && !c.does.is_empty(), "{}", c.name);
            assert!(!(c.pad.is_empty() && c.keys.is_empty() && c.mouse.is_empty()), "{} has no way to do it", c.name);
            assert!(!SCHEME[..i].iter().any(|o| o.name == c.name), "{} is listed twice", c.name);
            // One button is one thing: no controller button stands for two entries of the scheme.
            assert!(c.pad.is_empty() || c.pad.starts_with("X, then") || !SCHEME[..i].iter().any(|o| o.pad == c.pad), "{} shares {} with another", c.name, c.pad);
        }
    }

    /// The README's and the design document's tables are the scheme, line for line.
    #[test]
    fn the_readme_and_the_design_show_the_scheme() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for file in ["README.md", "docs/DESIGN.md"] {
            let text = std::fs::read_to_string(root.join(file)).unwrap();
            for c in SCHEME {
                let line = format!("| {} | {} | {} | {} |", c.name, c.pad, c.keys, c.mouse);
                assert!(text.contains(&line), "{file} lacks the line: {line}");
            }
        }
    }
}
