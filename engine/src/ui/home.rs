//! Home: Guided shows what needs a look first, then setup and a few readings; Veteran is a dashboard.
use super::{Ui, activity, arr, b, hint, kind_word, n, obj, s};
use crate::checkup::{self, Item, Level};
use crate::hardware::Machine;
use crate::json::Json;
use crate::sensors::Readings;
use crate::settings::{SETUP_STEPS, Settings};

/// How many checkup cards Guided shows before "Show all".
const SHOWN: usize = 3;

pub(crate) fn step_title(id: &str) -> &'static str {
    match id {
        "machine" => "Your machine",
        "password" => "Password",
        "games" => "Your games",
        "bring" => "Bring it over",
        _ => "Picks",
    }
}

fn item_json(i: &Item) -> Json {
    obj! {"id": s(i.id), "look": b(i.level == Level::Look), "icon": s(i.icon), "title": s(&i.title), "line": s(&i.line), "button": s(i.action.0), "opens": s(i.action.1)}
}

fn thousands(v: u32) -> String {
    if v >= 1000 { format!("{},{:03}", v / 1000, v % 1000) } else { v.to_string() }
}

/// The readings as tiles, only the ones this machine has: (id, big value, small words, icon).
pub(crate) fn tiles(m: &Machine, r: &Readings) -> Vec<(&'static str, String, String, &'static str)> {
    let mut out = Vec::new();
    if let Some(p) = r.battery_pct {
        out.push(("battery", format!("{p}%"), if r.charging { "Battery, charging" } else { "Battery" }.to_string(), "battery"));
    }
    if let Some(c) = r.cpu_c {
        out.push(("cpu", format!("{c:.0}\u{b0}C"), "CPU".into(), "temp"));
    }
    if let Some(c) = r.gpu_c
        && m.battery().is_none()
    {
        out.push(("gpu", format!("{c:.0}\u{b0}C"), "Graphics card".into(), "temp"));
    }
    if let Some(w) = r.battery_w.or(r.gpu_w) {
        out.push(("power", if w < 20.0 { format!("{w:.1} W") } else { format!("{w:.0} W") }, if r.battery_w.is_some() { "Power draw" } else { "Graphics power" }.to_string(), "bolt"));
    }
    if let Some(f) = r.fan_rpm
        && f > 0
    {
        out.push(("fan", format!("{} rpm", thousands(f)), "Fan".into(), "fan"));
    }
    if let Some(e) = m.connected_displays().find(|d| !d.internal).and_then(|d| d.edid.as_ref()) {
        let res = match e.best_mode.1 {
            2160 => "4K".to_string(),
            h => format!("{h}p"),
        };
        out.push(("screen", format!("{res} {} Hz", e.max_hz), if m.on_tv() { "TV" } else { "Monitor" }.to_string(), "display"));
    }
    out
}

fn setup_card(st: &Settings) -> Json {
    let left: Vec<&str> = SETUP_STEPS.iter().copied().filter(|x| !st.setup_done.iter().any(|d| d == x)).collect();
    obj! {
        "show": b(!left.is_empty()),
        "done": n((SETUP_STEPS.len() - left.len()) as f64),
        "total": n(SETUP_STEPS.len() as f64),
        "next": arr(left.iter().take(2).map(|x| s(step_title(x)))),
        "step": s(left.first().copied().unwrap_or("")),
    }
}

/// The eight quick switches of the dashboard: Game Boost and FSR 4 as real switches, then this machine's cards.
fn switches(ui: &Ui, m: &Machine, st: &Settings) -> Vec<Json> {
    let mut out = vec![obj! {"id": s("boost"), "title": s("Game Boost"), "value": s("While a game runs"), "toggle": b(true), "on": b(st.game_boost), "icon": s("bolt")}];
    out.push(obj! {"id": s("fsr4all"), "title": s("FSR 4, every game"), "value": s("Proton games"), "toggle": b(true), "on": b(super::games::Switches::load(&ui.folder()).fsr4_all), "icon": s("spark")});
    for c in crate::modules::cards(&ui.ctx.root, m, &ui.ledger(), st.game_boost) {
        if c.id == "cpu" {
            continue;
        }
        let value = if c.id == "lighting" { super::machine::Lighting::load(&ui.folder()).effect } else { c.value.clone() };
        out.push(obj! {"id": s(format!("module:{}", c.id)), "title": s(c.title), "value": s(value), "toggle": b(false), "on": b(false), "icon": s(c.icon)});
    }
    out.truncate(8);
    out
}

pub(crate) fn page(ui: &mut Ui) -> Json {
    let m = ui.machine();
    let st = ui.settings();
    let items = checkup::items(&m, &ui.facts);
    let (headline, sub) = checkup::headline(&items);
    let r = ui.readings();
    let tiles = arr(tiles(&m, &r).into_iter().map(|(id, v, l, icon)| obj! {"id": s(id), "value": s(v), "label": s(l), "icon": s(icon)}));
    if ui.veteran() {
        let now = crate::now();
        return obj! {
            "view": s("veteran"),
            "tiles": tiles,
            "switches": arr(switches(ui, &m, &st)),
            "items": arr(items.iter().map(item_json)),
            "fine": s(if items.iter().any(|i| i.level == Level::Look) { "" } else { "Nothing needs a look." }),
            "activity": arr(activity::read(&ui.folder(), 8).into_iter().map(|(at, w)| obj! {"when": s(activity::ago(now, at)), "text": s(w)})),
            "hints": arr([hint("y", "Refresh")]),
        };
    }
    let shown = if ui.home_all { items.len() } else { SHOWN.min(items.len()) };
    let more = items.len() - shown;
    let tip = if ui.window {
        ("Mouse and keyboard", "Everything works with a mouse too. Q and E switch tabs, Esc goes back.")
    } else {
        ("New here?", "Looking for Task Manager or the C: drive? Press View for \"Where's my...?\"")
    };
    obj! {
        "view": s("guided"),
        "headline": s(headline),
        "sub": s(sub),
        "setup": setup_card(&st),
        "items": arr(items.iter().take(shown).map(item_json)),
        "more": s(match more { 0 => String::new(), 1 => "1 more tip.".into(), n => format!("{n} more tips.") }),
        "all": b(ui.home_all),
        "tiles": tiles,
        "tip": obj! {"title": s(tip.0), "line": s(tip.1)},
        "kind": s(kind_word(&m)),
        "hints": arr(if more > 0 || ui.home_all { vec![hint("x", if ui.home_all { "Show fewer" } else { "Show all" })] } else { vec![] }),
    }
}
