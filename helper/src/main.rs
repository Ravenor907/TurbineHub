//! turbinehub-helper <job> [json]. Run by the app through pkexec (allowed without a password by Turbine Hub's
//! polkit rule, only for this file and only for its owner), or through sudo with the player's password to install.
//! Prints one JSON line: {"ok":true,...} or {"ok":false,"error":"..."}.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use turbinehub_helper::json::{self, Json, quote};
use turbinehub_helper::ops::{self, Host, Owner, Sys};
use turbinehub_helper::policy::Setting;

const MAX_ARG: usize = 64 * 1024;

fn uid_env(name: &str) -> Option<u32> {
    std::env::var(name).ok()?.parse().ok()
}

/// The helper's own file and folders must be root's and writable by nobody else, or a player's program could swap
/// what the polkit rule lets run.
fn safe_install() -> Result<(), String> {
    for p in [ops::STATE_DIR, "/var/lib/turbinehub/bin", ops::BIN] {
        let m = fs::metadata(p).map_err(|_| format!("{p} is missing"))?;
        if m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err(format!("{p} isn't protected, so the helper won't run"));
        }
    }
    Ok(())
}

fn arg_json(arg: Option<&String>) -> Result<Json, String> {
    let text = arg.ok_or("This job needs its details")?;
    if text.len() > MAX_ARG {
        return Err("Too much input".into());
    }
    json::parse(text).map_err(|_| "The details aren't valid JSON".to_string())
}

fn run(verb: &str, arg: Option<&String>, sys: &mut Sys<Host>) -> Result<String, String> {
    if euid() != 0 {
        return Err("The helper must run as root".into());
    }
    if verb == "install" {
        let uid = uid_env("SUDO_UID").ok_or("Install runs through sudo")?;
        let owner = sys.user(uid).ok_or("Unknown user")?;
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        sys.install(&me, &owner)?;
        return Ok(String::new());
    }
    if verb == "reapply" && std::env::var_os("PKEXEC_UID").is_none() && std::env::var_os("SUDO_UID").is_none() {
        // Started by the boot unit.
        safe_install()?;
        return sys.reapply().map(|f| format!(",\"fixed\":{}", list(&f)));
    }
    if verb == "hid-access" {
        // Started by the lighting access rule's udev line, as root. Only ever adds the write bit to an ASUS
        // lighting interface, and only once the player asked for the rule (see `Sys::hid_access`).
        safe_install()?;
        let name = arg.ok_or("Which device?")?;
        return sys.hid_access(name).map(|opened| format!(",\"opened\":{opened}"));
    }
    safe_install()?;
    let owner: Owner = sys.owner().ok_or("Turbine Hub's helper isn't set up")?;
    let caller = uid_env("PKEXEC_UID").or_else(|| uid_env("SUDO_UID")).ok_or("Unknown caller")?;
    if caller != owner.uid {
        return Err("This helper belongs to another user".into());
    }
    match verb {
        "status" => sys.status().map(|s| format!(",\"status\":{s}")),
        "apply" => sys.apply(Setting::from_json(&arg_json(arg)?)?).map(|id| format!(",\"id\":{id}")),
        "undo" => sys.undo(ops::change_id(&arg_json(arg)?)?).map(|w| format!(",\"undone\":{}", quote(&w))),
        "undo-all" => sys.undo_all().map(|_| String::new()),
        "verify" => sys.verify().map(|d| format!(",\"drift\":{}", list(&d))),
        "reapply" => sys.reapply().map(|f| format!(",\"fixed\":{}", list(&f))),
        "set-now" => {
            // One value ({"path","value"}) or several at once ({"values":[{"path","value"},...]}), so Game Boost on a
            // many-core processor is one call.
            let j = arg_json(arg)?;
            let pairs: Vec<(String, String)> = match j.get("values") {
                Some(list) => list.as_array().iter().map(|v| Some((v.str_at("path")?.to_string(), v.str_at("value")?.to_string()))).collect::<Option<_>>().ok_or("A value is missing its path or value")?,
                None => vec![(j.str_at("path").ok_or("path is missing")?.to_string(), j.str_at("value").ok_or("value is missing")?.to_string())],
            };
            if pairs.is_empty() || pairs.len() > 512 {
                return Err("Between 1 and 512 values".into());
            }
            for (p, v) in &pairs {
                sys.set_now(p, v)?;
            }
            Ok(String::new())
        }
        "decky-install" => {
            let j = arg_json(arg)?;
            sys.decky_install(j.str_at("tag").ok_or("tag is missing")?, &owner).map(|id| format!(",\"id\":{id}"))
        }
        "boot-next" => sys.boot_next(arg_json(arg)?.str_at("entry").ok_or("entry is missing")?).map(|_| String::new()),
        "uninstall" => sys.uninstall().map(|_| String::new()),
        _ => Err("Unknown job".into()),
    }
}

fn list(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|s| quote(s)).collect::<Vec<_>>().join(","))
}

fn euid() -> u32 {
    // The effective user, read from /proc so the helper needs no libc bindings.
    fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find_map(|l| l.strip_prefix("Uid:").and_then(|r| r.split_whitespace().nth(1)?.parse().ok()))).unwrap_or(u32::MAX)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let verb = args.get(1).map(String::as_str).unwrap_or("");
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut sys = Sys { root: PathBuf::from("/"), run: Host, now };
    // One job at a time. The lighting rule's own start (hid-access, run by udev while another job may be telling
    // udev to look again) never waits: it only reads the record and opens one device. No job waits for ever: when
    // the lock stays taken, it says the helper is busy and ends.
    let lock = if verb != "hid-access" && euid() == 0 && Path::new(ops::STATE_DIR).is_dir() { ops::take_lock(Path::new(ops::STATE_DIR), ops::LOCK_WAIT) } else { Ok(None) };
    let result = match &lock {
        Ok(_) => run(verb, args.get(2), &mut sys),
        Err(busy) => Err(busy.clone()),
    };
    let caller = std::env::var("PKEXEC_UID").or_else(|_| std::env::var("SUDO_UID")).unwrap_or_else(|_| "boot".into());
    match &result {
        Ok(_) => sys.log(&format!("{caller} {verb} ok")),
        Err(e) => sys.log(&format!("{caller} {verb} refused: {e}")),
    }
    match result {
        Ok(extra) => println!("{{\"ok\":true{extra}}}"),
        Err(e) => {
            println!("{{\"ok\":false,\"error\":{}}}", quote(&e));
            std::process::exit(1);
        }
    }
}
