//! Talking to the privileged helper from inside the Flatpak: the exact command lines (run with `flatpak-spawn
//! --host`), and reading its one-line answers. The helper's own record is read straight from its file, which every
//! user may read.

use crate::json::{self, Json};
use std::path::Path;
pub use turbinehub_helper::ledger::{Change, Entry, Ledger};
pub use turbinehub_helper::ops::{BIN, LEDGER};
pub use turbinehub_helper::policy::{Access, Setting};

/// The command that runs one job through pkexec (no password once the helper is installed).
pub fn job(verb: &str, args: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = ["flatpak-spawn", "--host", "pkexec", BIN, verb].iter().map(|s| s.to_string()).collect();
    if let Some(a) = args {
        v.push(a.to_string());
    }
    v
}

pub fn apply(s: &Setting) -> Vec<String> {
    job("apply", Some(&s.to_json()))
}

pub fn undo(id: u64) -> Vec<String> {
    job("undo", Some(&format!("{{\"id\":{id}}}")))
}

/// Several /sys values at once, in one helper call.
pub fn set_now_many(values: &[(String, String)]) -> Vec<String> {
    let items: Vec<String> = values.iter().map(|(p, v)| format!("{{\"path\":{},\"value\":{}}}", json::quote(p), json::quote(v))).collect();
    job("set-now", Some(&format!("{{\"values\":[{}]}}", items.join(","))))
}

pub fn set_now(path: &str, value: &str) -> Vec<String> {
    job("set-now", Some(&format!("{{\"path\":{},\"value\":{}}}", json::quote(path), json::quote(value))))
}

/// Installing or updating the helper: through sudo, with the password on stdin (`-S`), never stored. `-k` makes sudo
/// ask every time instead of reusing a recent sign-in. `helper_in_flatpak` is the helper's host path inside the
/// installed Flatpak (from /.flatpak-info's app-path).
pub fn install(helper_in_flatpak: &Path) -> Vec<String> {
    ["flatpak-spawn", "--host", "sudo", "-S", "-k", "-p", "", &helper_in_flatpak.to_string_lossy(), "install"].iter().map(|s| s.to_string()).collect()
}

/// The helper's answer: Ok with the whole JSON, or Err with its message in plain words.
pub fn answer(stdout: &str) -> Result<Json, String> {
    let line = stdout.lines().rev().find(|l| l.trim_start().starts_with('{')).ok_or("The helper didn't answer")?;
    let j = json::parse(line).map_err(|_| "The helper's answer wasn't readable".to_string())?;
    if j.get("ok").and_then(Json::as_bool) == Some(true) { Ok(j) } else { Err(j.str_at("error").unwrap_or("The helper refused").to_string()) }
}

/// What pkexec or sudo printed when the helper never ran, in plain words.
pub fn launch_problem(stderr: &str, code: i32) -> Option<String> {
    let s = stderr.to_ascii_lowercase();
    if s.contains("incorrect password") || s.contains("sorry, try again") || s.contains("authentication failure") {
        Some("That password isn't right.".into())
    } else if s.contains("not authorized") || code == 127 && s.contains("pkexec") {
        Some("Turbine Hub's helper isn't set up yet.".into())
    } else if s.contains("no password was provided") || s.contains("a password is required") {
        Some("This machine has no password yet. Try again: Turbine Hub then helps you make one.".into())
    } else {
        None
    }
}

/// The helper's record, read directly (the file is readable by everyone). Missing means nothing was changed yet.
pub fn ledger(root: &Path) -> Ledger {
    std::fs::read_to_string(root.join(LEDGER.trim_start_matches('/'))).ok().and_then(|t| Ledger::parse(&t).ok()).unwrap_or_default()
}

/// Is the helper installed?
pub fn installed(root: &Path) -> bool {
    root.join(BIN.trim_start_matches('/')).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines_are_fixed() {
        let s = Setting::Sysctl { key: "vm.swappiness".into(), value: 100 };
        assert_eq!(apply(&s), vec!["flatpak-spawn", "--host", "pkexec", "/var/lib/turbinehub/bin/turbinehub-helper", "apply", r#"{"kind":"sysctl","key":"vm.swappiness","value":100}"#]);
        assert_eq!(undo(7).last().unwrap(), r#"{"id":7}"#);
        assert_eq!(set_now("/sys/a", "b\"").last().unwrap(), r#"{"path":"/sys/a","value":"b\""}"#);
        assert_eq!(install(Path::new("/var/lib/flatpak/app/x/files/bin/turbinehub-helper")).join(" "), "flatpak-spawn --host sudo -S -k -p  /var/lib/flatpak/app/x/files/bin/turbinehub-helper install");
    }

    #[test]
    fn reads_answers() {
        assert_eq!(answer("{\"ok\":true,\"id\":3}\n").unwrap().get("id").and_then(Json::as_f64), Some(3.0));
        assert_eq!(answer("noise\n{\"ok\":false,\"error\":\"vm.x isn't a setting\"}").unwrap_err(), "vm.x isn't a setting");
        assert!(answer("").is_err());
        assert_eq!(launch_problem("Sorry, try again.\nsudo: 1 incorrect password attempt", 1).as_deref(), Some("That password isn't right."));
        assert_eq!(launch_problem("Error executing command as another user: Not authorized", 127).as_deref(), Some("Turbine Hub's helper isn't set up yet."));
        assert_eq!(launch_problem("", 0), None);
    }
}
