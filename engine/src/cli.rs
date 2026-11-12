//! `turbinehub --json <job>`: one JSON object on stdout for the Decky quick panel and for scripts. The jobs only
//! read: changes always go through the app, where the player sees and confirms them. `open` is how the panel sends
//! the player there: it leaves a note saying which page to show and changes nothing else.

use crate::checkup::{self, Facts, Level};
use crate::hardware::Machine;
use crate::json::quote;
use crate::switches::{GameSwitches, Switch};
use crate::{glossary, helper, library, modules, sensors, settings::Settings, shortcuts, tuneup};
use std::path::{Path, PathBuf};

/// Where to read from: "/" and the player's folders for real, fake ones for tests and screenshots.
pub struct Ctx {
    pub root: PathBuf,
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub efibootmgr: String,
    pub version: String,
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "null".into())
}

fn arr(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(","))
}

fn machine_json(m: &Machine) -> String {
    format!(
        "{{\"name\":{},\"kind\":{},\"onTv\":{},\"cpu\":{},\"gpu\":{},\"memoryGb\":{},\"steamos\":{},\"windows\":{}}}",
        quote(&m.name()),
        quote(m.kind.key()),
        m.on_tv(),
        quote(&m.cpu),
        quote(&m.main_gpu().map(|g| g.pci_id.clone()).unwrap_or_default()),
        m.memory_gb,
        quote(&m.steamos),
        m.windows_boot.is_some()
    )
}

fn readings_json(root: &Path) -> String {
    let r = sensors::read(root);
    format!(
        "{{\"cpuC\":{},\"gpuC\":{},\"gpuW\":{},\"batteryPct\":{},\"batteryW\":{},\"charging\":{},\"fanRpm\":{}}}",
        opt(r.cpu_c.map(|v| v.round())),
        opt(r.gpu_c.map(|v| v.round())),
        opt(r.gpu_w.map(|v| (v * 10.0).round() / 10.0)),
        opt(r.battery_pct),
        opt(r.battery_w.map(|v| (v * 10.0).round() / 10.0)),
        r.charging,
        opt(r.fan_rpm)
    )
}

/// The ring lights' brightness as a percent of the brightest the kernel allows. None without ring lights.
fn lighting_percent(root: &Path, m: &Machine) -> Option<u32> {
    let led = root.join("sys/class/leds").join(m.rgb_leds.first()?);
    let num = |f: &str| std::fs::read_to_string(led.join(f)).ok()?.trim().parse::<u64>().ok();
    let max = num("max_brightness").filter(|v| *v > 0).unwrap_or(255);
    Some((num("brightness")?.min(max) * 100 / max) as u32)
}

/// Where the app keeps the anti-cheat list it last fetched, for jobs that run without the network.
pub fn anticheat_cache(config_home: &Path) -> PathBuf {
    config_home.join("turbinehub/cache/anticheat.json")
}

/// A game as the quick panel names it: a Steam app id ("1245620") or Turbine Hub's own id ("epic:Fortnite").
/// None for anything that can't be one.
fn game_id(arg: &str) -> Option<String> {
    let ok = !arg.is_empty() && arg.len() <= 200 && !arg.starts_with('-') && arg.chars().all(|c| !c.is_control());
    ok.then(|| if arg.bytes().all(|b| b.is_ascii_digit()) { format!("steam:{arg}") } else { arg.to_string() })
}

fn game_json(arg: &str, ctx: &Ctx) -> Option<String> {
    let id = game_id(arg)?;
    let scan = crate::scan_games(&ctx.home);
    let Some(g) = scan.games.iter().find(|g| g.id() == id) else { return Some("\"found\":false".into()) };
    let list = std::fs::read_to_string(anticheat_cache(&ctx.config_home)).map(|t| library::parse_anticheat(&t)).unwrap_or_default();
    let locked = g.store == crate::Store::Steam && list.iter().any(|(app, status)| *app == g.store_id && matches!(status, library::AntiCheat::Supported | library::AntiCheat::Running));
    let on = GameSwitches::load(&ctx.config_home.join("turbinehub"));
    let switches = arr(Switch::ALL.iter().map(|s| format!("{{\"id\":{},\"title\":{},\"line\":{},\"on\":{}}}", quote(s.key()), quote(s.title()), quote(s.line()), !locked && on.is_on(&id, *s))));
    Some(format!("\"found\":true,\"game\":{{\"id\":{},\"name\":{},\"store\":{}}},\"locked\":{locked},\"switches\":{switches}", quote(&id), quote(&g.name), quote(g.store.label())))
}

/// The pages the quick panel can send the player to.
pub const PAGES: &[&str] = &["home", "games", "saves", "boost", "lighting", "battery"];
/// A page asked for longer ago than this is forgotten.
pub const OPEN_REQUEST_SECS: u64 = 120;

fn open_request_file(config_home: &Path) -> PathBuf {
    config_home.join("turbinehub/open-request.json")
}

/// The page the quick panel asked for a moment ago, once: the note is removed as it is read. The app calls this
/// when it starts and when it comes to the front.
pub fn take_open_request(config_home: &Path, now: u64) -> Option<String> {
    let file = open_request_file(config_home);
    let text = std::fs::read_to_string(&file).ok()?;
    let _ = std::fs::remove_file(&file);
    let j = crate::json::parse(&text).ok()?;
    let at = j.get("at").and_then(crate::json::Json::as_f64)? as u64;
    let page = j.str_at("page").filter(|p| PAGES.contains(p))?;
    (at <= now && now - at <= OPEN_REQUEST_SECS).then(|| page.to_string())
}

pub const JOBS: &[&str] = &["version", "machine", "readings", "checkup", "cards", "tuneup", "changes", "glossary", "panel", "game", "open"];

/// Does this job's answer depend on the firmware's boot entries (is Windows there to restart into)? The caller
/// asks the host for them (`efibootmgr`) for these jobs only and puts them in `Ctx::efibootmgr`: without them the
/// answer says there is no Windows, whatever is on the drives.
pub fn reads_boot_entries(job: &str) -> bool {
    matches!(job.split('=').next(), Some("machine" | "cards"))
}

fn refuse(words: &str) -> (String, i32) {
    (format!("{{\"ok\":false,\"error\":{}}}", quote(words)), 2)
}

/// Runs one job. Returns the JSON text and the exit code. A job about one thing takes it after an equals sign:
/// `game=1245620`.
pub fn run(job: &str, ctx: &Ctx) -> (String, i32) {
    let (job, arg) = job.split_once('=').unwrap_or((job, ""));
    let home_dev = crate::hardware::device_of(&ctx.home);
    let m = || Machine::read(&ctx.root, &ctx.efibootmgr, &home_dev);
    let body = match job {
        "version" => format!("\"version\":{}", quote(&ctx.version)),
        "machine" => format!("\"machine\":{}", machine_json(&m())),
        "readings" => format!("\"readings\":{}", readings_json(&ctx.root)),
        "checkup" => {
            let items = checkup::items(&m(), &Facts::default());
            let (title, sub) = checkup::headline(&items);
            format!(
                "\"headline\":{},\"sub\":{},\"items\":{}",
                quote(&title),
                quote(&sub),
                arr(items.iter().map(|i| format!("{{\"id\":{},\"look\":{},\"icon\":{},\"title\":{},\"line\":{},\"button\":{},\"opens\":{}}}", quote(i.id), i.level == Level::Look, quote(i.icon), quote(&i.title), quote(&i.line), quote(i.action.0), quote(i.action.1))))
            )
        }
        "cards" => {
            let s = Settings::load(&ctx.config_home);
            let cards = modules::cards(&ctx.root, &m(), &helper::ledger(&ctx.root), s.game_boost);
            format!("\"cards\":{}", arr(cards.iter().map(|c| format!("{{\"id\":{},\"title\":{},\"icon\":{},\"value\":{},\"line\":{},\"experimental\":{}}}", quote(c.id), quote(c.title), quote(c.icon), quote(&c.value), quote(c.line), c.experimental))))
        }
        "tuneup" => {
            let plan = tuneup::plan(&ctx.root, &helper::ledger(&ctx.root));
            format!("\"items\":{}", arr(plan.iter().map(|i| format!("{{\"id\":{},\"title\":{},\"why\":{},\"state\":{},\"restart\":{}}}", quote(i.id), quote(i.title), quote(i.why), quote(i.state.key()), i.restart))))
        }
        "changes" => {
            let l = helper::ledger(&ctx.root);
            format!("\"helper\":{},\"changes\":{}", helper::installed(&ctx.root), arr(l.entries.iter().rev().map(|e| format!("{{\"id\":{},\"words\":{},\"applied\":{}}}", e.id, quote(&e.change.words()), e.applied))))
        }
        "glossary" => format!("\"terms\":{}", arr(glossary::TERMS.iter().map(|t| format!("{{\"term\":{},\"means\":{},\"windows\":{}}}", quote(t.term), quote(t.means), quote(t.windows))))),
        // Everything the quick panel shows that isn't about one game, in one answer.
        "panel" => {
            let m = m();
            let s = Settings::load(&ctx.config_home);
            let hub = shortcuts::find_self(&ctx.home).map(|(id, _)| shortcuts::rungameid_url(id)).unwrap_or_default();
            format!(
                "\"version\":{},\"machine\":{{\"name\":{},\"kind\":{}}},\"readings\":{},\"boost\":{{\"available\":{},\"on\":{}}},\"lighting\":{{\"available\":{},\"percent\":{}}},\"helper\":{},\"hubUrl\":{}",
                quote(&ctx.version),
                quote(&m.name()),
                quote(m.kind.key()),
                readings_json(&ctx.root),
                !m.cpu_epp.is_empty(),
                s.game_boost,
                !m.rgb_leds.is_empty(),
                opt(lighting_percent(&ctx.root, &m)),
                helper::installed(&ctx.root),
                quote(&hub)
            )
        }
        "game" => match game_json(arg, ctx) {
            Some(body) => body,
            None => return refuse("Name the game: game=<Steam app id>"),
        },
        "open" => {
            if !PAGES.contains(&arg) {
                return refuse(&format!("Name the page: open=<{}>", PAGES.join("|")));
            }
            let hub = shortcuts::find_self(&ctx.home).map(|(id, _)| shortcuts::rungameid_url(id)).unwrap_or_default();
            let noted = crate::write_atomic(&open_request_file(&ctx.config_home), format!("{{\"page\":{},\"at\":{}}}\n", quote(arg), crate::now()).as_bytes()).is_ok();
            format!("\"page\":{},\"noted\":{noted},\"hubUrl\":{}", quote(arg), quote(&hub))
        }
        _ => return refuse(&format!("Unknown job. Jobs: {}", JOBS.join(", "))),
    };
    (format!("{{\"ok\":true,{body}}}"), 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{self, Preset};
    use crate::json::{Json, parse};

    fn ctx(tag: &str, preset: Preset) -> Ctx {
        let root = std::env::temp_dir().join(format!("turbinehub-cli-{tag}-{}", std::process::id()));
        fake::machine(&root, preset);
        Ctx { root: root.clone(), home: root.join("home/deck"), config_home: root.join("config"), efibootmgr: fake::EFI_WITH_WINDOWS.into(), version: "0.1".into() }
    }

    fn w(p: PathBuf, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn steam_game(home: &Path, id: &str, name: &str) {
        let steam = home.join(".local/share/Steam");
        w(steam.join("steamapps/libraryfolders.vdf"), &format!("\"libraryfolders\"\n{{\n\"0\"\n{{\n\"path\" \"{}\"\n}}\n}}\n", steam.display()));
        w(steam.join(format!("steamapps/appmanifest_{id}.acf")), &format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"{name}\"\n}}\n"));
        w(steam.join(format!("steamapps/common/{name}/game.exe")), "MZ");
    }

    #[test]
    fn every_job_answers_valid_json() {
        let c = ctx("all", Preset::Desktop);
        for job in JOBS.iter().copied().filter(|j| !["game", "open"].contains(j)).chain(["game=1245620", "open=home"]) {
            let (text, code) = run(job, &c);
            let j = parse(&text).unwrap_or_else(|e| panic!("{job}: {e} in {text}"));
            assert_eq!((code, j.get("ok").and_then(Json::as_bool)), (0, Some(true)), "{job}");
        }
        let (text, _) = run("machine", &c);
        let j = parse(&text).unwrap();
        assert_eq!(j.get("machine").unwrap().str_at("name"), Some("Desktop PC"));
        assert_eq!(j.get("machine").unwrap().get("windows").and_then(Json::as_bool), Some(true));
        let (text, _) = run("cards", &c);
        assert_eq!(parse(&text).unwrap().get("cards").unwrap().as_array().len(), 8);
        let (text, code) = run("rm", &c);
        assert_eq!(code, 2);
        assert!(text.contains("\"ok\":false"));
        let _ = std::fs::remove_dir_all(&c.root);
    }

    #[test]
    fn jobs_that_show_windows_are_the_ones_given_the_boot_entries() {
        let with = ctx("boot", Preset::Desktop);
        let without = Ctx { root: with.root.clone(), home: with.home.clone(), config_home: with.config_home.clone(), efibootmgr: String::new(), version: with.version.clone() };
        for job in JOBS.iter().copied().filter(|j| !["game", "open", "readings"].contains(j)) {
            assert_eq!(run(job, &with).0 != run(job, &without).0, reads_boot_entries(job), "{job}");
        }
        // Without them a machine with Windows on a drive reads as one without.
        let j = parse(&run("machine", &without).0).unwrap();
        assert_eq!(j.get("machine").unwrap().get("windows").and_then(Json::as_bool), Some(false));
        assert!(!reads_boot_entries("game=1245620") && !reads_boot_entries("panel") && reads_boot_entries("machine"));
        let _ = std::fs::remove_dir_all(&with.root);
    }

    #[test]
    fn the_panel_job_has_what_the_quick_panel_shows() {
        let c = ctx("panel", Preset::Handheld);
        Settings { game_boost: true, ..Settings::default() }.save(&c.config_home).unwrap();
        let (text, code) = run("panel", &c);
        let j = parse(&text).unwrap();
        assert_eq!(code, 0);
        assert_eq!(j.get("machine").unwrap().str_at("kind"), Some("handheld"));
        assert_eq!(j.get("readings").unwrap().get("batteryPct").and_then(Json::as_f64), Some(78.0));
        let boost = j.get("boost").unwrap();
        assert_eq!((boost.get("available").and_then(Json::as_bool), boost.get("on").and_then(Json::as_bool)), (Some(true), Some(true)));
        let light = j.get("lighting").unwrap();
        assert_eq!((light.get("available").and_then(Json::as_bool), light.get("percent").and_then(Json::as_f64)), (Some(true), Some(50.0)));
        assert_eq!((j.get("helper").and_then(Json::as_bool), j.str_at("hubUrl")), (Some(false), Some("")));
        let _ = std::fs::remove_dir_all(&c.root);

        let c = ctx("panel-pc", Preset::Desktop);
        let j = parse(&run("panel", &c).0).unwrap();
        let light = j.get("lighting").unwrap();
        assert_eq!((light.get("available").and_then(Json::as_bool), light.get("percent")), (Some(false), Some(&Json::Null)));
        assert_eq!(j.get("boost").unwrap().get("on").and_then(Json::as_bool), Some(false));
        let _ = std::fs::remove_dir_all(&c.root);
    }

    #[test]
    fn the_game_job_lists_the_switches_and_locks_online_anti_cheat() {
        let c = ctx("game", Preset::Handheld);
        steam_game(&c.home, "1245620", "ELDEN RING");
        steam_game(&c.home, "1145360", "Hades II");
        let mut on = GameSwitches::default();
        on.set("steam:1145360", Switch::Handheld, true);
        on.set("steam:1245620", Switch::Fsr4, true);
        on.save(&c.config_home.join("turbinehub"));
        w(anticheat_cache(&c.config_home), r#"[{"status":"Running","storeIds":{"steam":"1245620"}},{"status":"Denied","storeIds":{"steam":"1145360"}}]"#);

        let j = parse(&run("game=1145360", &c).0).unwrap();
        assert_eq!(j.get("found").and_then(Json::as_bool), Some(true));
        assert_eq!((j.get("game").unwrap().str_at("id"), j.get("game").unwrap().str_at("name")), (Some("steam:1145360"), Some("Hades II")));
        assert_eq!(j.get("locked").and_then(Json::as_bool), Some(false));
        let switches = j.get("switches").unwrap().as_array();
        assert_eq!(switches.len(), Switch::ALL.len());
        let on_ids: Vec<&str> = switches.iter().filter(|s| s.get("on").and_then(Json::as_bool) == Some(true)).filter_map(|s| s.str_at("id")).collect();
        assert_eq!(on_ids, vec!["handheld"]);

        let j = parse(&run("game=steam:1245620", &c).0).unwrap();
        assert_eq!(j.get("locked").and_then(Json::as_bool), Some(true));
        assert!(j.get("switches").unwrap().as_array().iter().all(|s| s.get("on").and_then(Json::as_bool) == Some(false)));

        assert_eq!(parse(&run("game=7", &c).0).unwrap().get("found").and_then(Json::as_bool), Some(false));
        for bad in ["game", "game=", "game=--json", "game=a\nb"] {
            let (text, code) = run(bad, &c);
            assert_eq!(code, 2, "{bad}");
            assert!(text.contains("\"ok\":false"));
        }
        let _ = std::fs::remove_dir_all(&c.root);
    }

    #[test]
    fn the_open_job_leaves_a_note_the_app_reads_once() {
        let c = ctx("open", Preset::Handheld);
        assert_eq!(take_open_request(&c.config_home, crate::now()), None);
        let j = parse(&run("open=lighting", &c).0).unwrap();
        assert_eq!((j.str_at("page"), j.get("noted").and_then(Json::as_bool), j.str_at("hubUrl")), (Some("lighting"), Some(true), Some("")));
        assert_eq!(take_open_request(&c.config_home, crate::now()).as_deref(), Some("lighting"));
        assert_eq!(take_open_request(&c.config_home, crate::now()), None);

        run("open=saves", &c);
        assert_eq!(take_open_request(&c.config_home, crate::now() + OPEN_REQUEST_SECS + 5), None);
        assert_eq!(take_open_request(&c.config_home, crate::now()), None);
        w(open_request_file(&c.config_home), &format!("{{\"page\":\"../etc\",\"at\":{}}}", crate::now()));
        assert_eq!(take_open_request(&c.config_home, crate::now()), None);

        for bad in ["open", "open=", "open=settings/../x", "open=--help"] {
            assert_eq!(run(bad, &c).1, 2, "{bad}");
        }
        assert!(!open_request_file(&c.config_home).exists());
        let _ = std::fs::remove_dir_all(&c.root);
    }
}
