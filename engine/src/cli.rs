//! `turbinehub --json <job>`: one JSON object on stdout for the Decky quick panel and for scripts. Read-only jobs
//! only: changes always go through the app, where the player sees and confirms them.

use crate::checkup::{self, Facts, Level};
use crate::hardware::Machine;
use crate::json::quote;
use crate::{glossary, helper, modules, sensors, settings::Settings, tuneup};
use std::path::PathBuf;

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

/// Runs one job. Returns the JSON text and the exit code.
pub fn run(job: &str, ctx: &Ctx) -> (String, i32) {
    let home_dev = crate::hardware::device_of(&ctx.home);
    let m = || Machine::read(&ctx.root, &ctx.efibootmgr, &home_dev);
    let body = match job {
        "version" => format!("\"version\":{}", quote(&ctx.version)),
        "machine" => format!("\"machine\":{}", machine_json(&m())),
        "readings" => {
            let r = sensors::read(&ctx.root);
            format!(
                "\"readings\":{{\"cpuC\":{},\"gpuC\":{},\"gpuW\":{},\"batteryPct\":{},\"batteryW\":{},\"charging\":{},\"fanRpm\":{}}}",
                opt(r.cpu_c.map(|v| v.round())),
                opt(r.gpu_c.map(|v| v.round())),
                opt(r.gpu_w.map(|v| (v * 10.0).round() / 10.0)),
                opt(r.battery_pct),
                opt(r.battery_w.map(|v| (v * 10.0).round() / 10.0)),
                r.charging,
                opt(r.fan_rpm)
            )
        }
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
        _ => return (format!("{{\"ok\":false,\"error\":{}}}", quote("Unknown job. Jobs: version, machine, readings, checkup, cards, tuneup, changes, glossary")), 2),
    };
    (format!("{{\"ok\":true,{body}}}"), 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{self, Preset};
    use crate::json::{Json, parse};

    #[test]
    fn every_job_answers_valid_json() {
        let root = std::env::temp_dir().join(format!("turbinehub-cli-{}", std::process::id()));
        fake::machine(&root, Preset::Desktop);
        let ctx = Ctx { root: root.clone(), home: root.join("home/deck"), config_home: root.join("config"), efibootmgr: fake::EFI_WITH_WINDOWS.into(), version: "0.1".into() };
        for job in ["version", "machine", "readings", "checkup", "cards", "tuneup", "changes", "glossary"] {
            let (text, code) = run(job, &ctx);
            let j = parse(&text).unwrap_or_else(|e| panic!("{job}: {e} in {text}"));
            assert_eq!((code, j.get("ok").and_then(Json::as_bool)), (0, Some(true)), "{job}");
        }
        let (text, _) = run("machine", &ctx);
        let j = parse(&text).unwrap();
        assert_eq!(j.get("machine").unwrap().str_at("name"), Some("Desktop PC"));
        assert_eq!(j.get("machine").unwrap().get("windows").and_then(Json::as_bool), Some(true));
        let (text, _) = run("cards", &ctx);
        assert_eq!(parse(&text).unwrap().get("cards").unwrap().as_array().len(), 8);
        let (text, code) = run("rm", &ctx);
        assert_eq!(code, 2);
        assert!(text.contains("\"ok\":false"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
