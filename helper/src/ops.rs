//! The helper's jobs. Every path is under `Sys::root` ("/" for real, a scratch folder in tests) and every program
//! runs through `Run`, so the tests exercise the real code without touching the machine.

use crate::json::{self, Json, quote};
use crate::ledger::{Change, Entry, Ledger};
use crate::policy::{self, Setting, Values};
use crate::sha256;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub const STATE_DIR: &str = "/var/lib/turbinehub";
pub const BIN: &str = "/var/lib/turbinehub/bin/turbinehub-helper";
pub const LEDGER: &str = "/var/lib/turbinehub/ledger.json";
pub const OWNER: &str = "/var/lib/turbinehub/owner";
pub const LOG: &str = "/var/lib/turbinehub/helper.log";
pub const BACKUP_DIR: &str = "/var/lib/turbinehub/backup";
pub const POLKIT_RULE: &str = "/etc/polkit-1/rules.d/49-turbinehub.rules";
pub const REAPPLY_UNIT: &str = "/etc/systemd/system/turbinehub-reapply.service";
pub const DECKY_UNIT: &str = "/etc/systemd/system/plugin_loader.service";
pub const DECKY_REPO: &str = "SteamDeckHomebrew/decky-loader";

pub const SYSTEMCTL: &str = "/usr/bin/systemctl";
pub const UDEVADM: &str = "/usr/bin/udevadm";
pub const MODPROBE: &str = "/usr/bin/modprobe";
pub const EFIBOOTMGR: &str = "/usr/bin/efibootmgr";
pub const CURL: &str = "/usr/bin/curl";

/// Runs one program with fixed arguments and returns what it printed. Never a shell.
pub trait Run {
    fn run(&mut self, prog: &str, args: &[&str]) -> Result<String, String>;
}

/// The real machine.
pub struct Host;

impl Run for Host {
    fn run(&mut self, prog: &str, args: &[&str]) -> Result<String, String> {
        let out = std::process::Command::new(prog)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin")
            .env("LANG", "C")
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("Couldn't run {prog} ({e})"))?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.success() {
            Ok(text)
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!("{} failed: {}", prog.rsplit('/').next().unwrap_or(prog), err.trim().lines().last().unwrap_or("no message")))
        }
    }
}

/// The player the helper works for (written at install, checked on every call).
#[derive(Debug, Clone, PartialEq)]
pub struct Owner {
    pub uid: u32,
    pub name: String,
    pub home: PathBuf,
}

pub struct Sys<R: Run> {
    pub root: PathBuf,
    pub run: R,
    pub now: u64,
}

fn io_err(what: &str, e: io::Error) -> String {
    format!("Couldn't {what} ({e})")
}

/// A Decky release tag: v, then numbers and dots, maybe -preN.
pub fn decky_tag_ok(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else { return false };
    let (main, pre) = rest.split_once("-pre").unwrap_or((rest, "0"));
    tag.len() <= 24
        && !main.is_empty()
        && main.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && !pre.is_empty()
        && pre.bytes().all(|b| b.is_ascii_digit())
}

fn user_name_ok(n: &str) -> bool {
    let mut b = n.bytes();
    matches!(b.next(), Some(c) if c.is_ascii_lowercase() || c == b'_') && n.len() <= 32 && b.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
}

/// The live value in a /sys or /proc file: the bracketed word for files like "always [madvise] never".
pub fn live_word(text: &str) -> String {
    let t = text.trim();
    match (t.find('['), t.find(']')) {
        (Some(a), Some(b)) if b > a => t[a + 1..b].to_string(),
        _ => t.to_string(),
    }
}

impl<R: Run> Sys<R> {
    pub fn p(&self, abs: &str) -> PathBuf {
        self.root.join(abs.trim_start_matches('/'))
    }

    fn read(&self, abs: &str) -> Option<String> {
        fs::read_to_string(self.p(abs)).ok()
    }

    fn write_file(&self, abs: &str, text: &str, mode: u32) -> Result<(), String> {
        let path = self.p(abs);
        if let Some(d) = path.parent() {
            fs::create_dir_all(d).map_err(|e| io_err(&format!("make {}", d.display()), e))?;
        }
        let tmp = path.with_extension("turbinehub-new");
        fs::write(&tmp, text).map_err(|e| io_err(&format!("write {abs}"), e))?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(mode)).map_err(|e| io_err("set permissions", e))?;
        fs::rename(&tmp, &path).map_err(|e| io_err(&format!("write {abs}"), e))
    }

    pub fn steamos(&self) -> String {
        self.read("/etc/os-release").unwrap_or_default().lines().find_map(|l| l.strip_prefix("VERSION_ID=")).unwrap_or("").trim_matches('"').to_string()
    }

    // ---------------------------------------------------------------- owner

    /// The user with this uid, from /etc/passwd.
    pub fn user(&self, uid: u32) -> Option<Owner> {
        self.read("/etc/passwd")?.lines().find_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            (f.len() >= 7 && f[2].parse::<u32>().ok() == Some(uid) && user_name_ok(f[0])).then(|| Owner { uid, name: f[0].into(), home: PathBuf::from(f[5]) })
        })
    }

    pub fn owner(&self) -> Option<Owner> {
        let uid = self.read(OWNER)?.trim().parse().ok()?;
        self.user(uid)
    }

    // ---------------------------------------------------------------- ledger

    pub fn ledger(&self) -> Result<Ledger, String> {
        Ledger::parse(&self.read(LEDGER).unwrap_or_default())
    }

    fn save(&self, l: &Ledger) -> Result<(), String> {
        self.write_file(LEDGER, &l.to_text(), 0o644)
    }

    // ---------------------------------------------------------------- live values

    fn proc_path(key: &str) -> String {
        format!("/proc/sys/{}", key.replace('.', "/"))
    }

    /// A /sys path checked against the real folder: it must already exist (nothing is created under /sys) and, with
    /// links followed, still be inside /sys.
    fn sysfs_target(&self, path: &str) -> Result<PathBuf, String> {
        let full = self.p(path);
        let real = fs::canonicalize(&full).map_err(|_| format!("{path} isn't on this machine"))?;
        let sys = fs::canonicalize(self.p("/sys")).map_err(|_| "No /sys".to_string())?;
        if !real.starts_with(&sys) || !real.is_file() {
            return Err(format!("{path} isn't a value Turbine Hub changes"));
        }
        Ok(full)
    }

    fn check_live(&self, s: &Setting) -> Result<(), String> {
        if let Setting::Sysfs { path, value } = s {
            self.sysfs_target(path)?;
            if let Some(Values::IntBetweenSiblings(lo, hi)) = policy::sysfs_rule(path) {
                let dir = Path::new(path).parent().unwrap_or(Path::new("/")).to_string_lossy().into_owned();
                let get = |n: &str| self.read(&format!("{dir}/{n}")).and_then(|t| t.trim().parse::<u64>().ok());
                let (Some(lo), Some(hi)) = (get(lo), get(hi)) else { return Err(format!("Couldn't read the limits for {path}")) };
                let v: u64 = value.parse().map_err(|_| "Not a number".to_string())?;
                if v < lo || v > hi {
                    return Err(format!("{path} must be between {lo} and {hi}"));
                }
            }
        }
        Ok(())
    }

    /// What is there now, for the "before" of a new entry.
    fn before_of(&mut self, s: &Setting) -> Vec<(String, String)> {
        let v = match s {
            Setting::Sysctl { key, .. } => self.read(&Self::proc_path(key)).map(|t| t.trim().to_string()),
            Setting::Sysfs { path, .. } => self.read(path).map(|t| live_word(&t)),
            Setting::Service { unit, .. } => Some(self.service_enabled(unit).to_string()),
            _ => None,
        };
        v.map(|v| vec![(if matches!(s, Setting::Service { .. }) { "enabled" } else { "value" }.to_string(), v)]).unwrap_or_default()
    }

    fn service_enabled(&mut self, unit: &str) -> bool {
        self.run.run(SYSTEMCTL, &["is-enabled", unit]).map(|o| o.trim() == "enabled").unwrap_or(false)
    }

    fn set_service(&mut self, unit: &str, on: bool) -> Result<(), String> {
        self.run.run(SYSTEMCTL, &[if on { "enable" } else { "disable" }, "--now", unit]).map(|_| ())
    }

    /// Puts the setting's value on the running system (files are written by `render_files`).
    fn apply_live(&mut self, s: &Setting) -> Result<(), String> {
        match s {
            Setting::Sysctl { key, value } => fs::write(self.p(&Self::proc_path(key)), format!("{value}\n")).map_err(|e| io_err(&format!("set {key}"), e)),
            Setting::Sysfs { path, value } => {
                let t = self.sysfs_target(path)?;
                fs::write(t, value).map_err(|e| io_err(&format!("set {path}"), e))
            }
            Setting::Service { unit, enabled } => self.set_service(unit, *enabled),
            Setting::Module { name } => self.run.run(MODPROBE, &[name]).map(|_| ()),
            Setting::Wake { .. } | Setting::Access { .. } | Setting::Zram { .. } => Ok(()),
        }
    }

    /// Puts back what was there before.
    fn restore_live(&mut self, e: &Entry) -> Result<(), String> {
        match &e.change {
            Change::Set(Setting::Sysctl { key, .. }) => match e.before("value") {
                Some(v) => fs::write(self.p(&Self::proc_path(key)), format!("{v}\n")).map_err(|er| io_err(&format!("set {key}"), er)),
                None => Ok(()),
            },
            Change::Set(Setting::Sysfs { path, .. }) => match e.before("value") {
                Some(v) => self.sysfs_target(path).and_then(|t| fs::write(t, v).map_err(|er| io_err(&format!("set {path}"), er))),
                None => Ok(()),
            },
            Change::Set(Setting::Service { unit, .. }) => {
                let was = e.before("enabled") == Some("true");
                self.set_service(unit, was)
            }
            Change::Set(Setting::Module { .. } | Setting::Wake { .. } | Setting::Access { .. } | Setting::Zram { .. }) => Ok(()),
            Change::Decky { .. } => self.undo_decky(e),
        }
    }

    /// Writes the files under /etc from the ledger and reloads what reads them. Returns the files changed.
    pub fn render_files(&mut self, l: &Ledger) -> Result<Vec<&'static str>, String> {
        let settings = l.settings();
        let mut changed = Vec::new();
        for f in policy::FILES {
            let want = policy::render(f, &settings);
            let have = self.read(f);
            match want {
                Some(t) if have.as_deref() != Some(t.as_str()) => {
                    self.write_file(f, &t, 0o644)?;
                    changed.push(*f);
                }
                None if have.is_some() => {
                    fs::remove_file(self.p(f)).map_err(|e| io_err(&format!("remove {f}"), e))?;
                    changed.push(*f);
                }
                _ => {}
            }
        }
        if changed.iter().any(|f| f.starts_with("/etc/udev/")) {
            self.run.run(UDEVADM, &["control", "--reload"])?;
            for sub in ["usb", "leds", "hidraw", "misc"] {
                let _ = self.run.run(UDEVADM, &["trigger", "--action=add", &format!("--subsystem-match={sub}")]);
            }
        }
        Ok(changed)
    }

    // ---------------------------------------------------------------- verbs

    /// Applies one setting and records it. On failure nothing stays changed.
    pub fn apply(&mut self, s: Setting) -> Result<u64, String> {
        s.check()?;
        self.check_live(&s)?;
        let mut l = self.ledger()?;
        let old = l.clone();
        let before = if l.find_key(&s.key()).is_some() { Vec::new() } else { self.before_of(&s) };
        let steamos = self.steamos();
        let id = l.record(Change::Set(s.clone()), before, self.now, &steamos);
        let result = self.render_files(&l).and_then(|_| self.apply_live(&s));
        if let Err(e) = result {
            let _ = self.render_files(&old);
            if let Some(prev) = old.find_key(&s.key()).cloned() {
                if let Change::Set(p) = &prev.change {
                    let _ = self.apply_live(p);
                }
            } else if let Some(new) = l.find_key(&s.key()).cloned() {
                let _ = self.restore_live(&new);
            }
            return Err(e);
        }
        self.save(&l)?;
        Ok(id)
    }

    /// Takes one change back.
    pub fn undo(&mut self, id: u64) -> Result<String, String> {
        let mut l = self.ledger()?;
        let e = l.remove(id).ok_or_else(|| format!("No change {id} in the record"))?;
        self.render_files(&l)?;
        self.restore_live(&e)?;
        self.save(&l)?;
        Ok(e.change.words())
    }

    /// Takes every change back, newest first. Keeps going past a failure and reports it.
    pub fn undo_all(&mut self) -> Result<Vec<String>, String> {
        let ids: Vec<u64> = self.ledger()?.entries.iter().rev().map(|e| e.id).collect();
        let mut errors = Vec::new();
        for id in ids {
            if let Err(e) = self.undo(id) {
                errors.push(e);
            }
        }
        if errors.is_empty() { Ok(Vec::new()) } else { Err(errors.join("; ")) }
    }

    /// What differs between the record and the machine, in plain words. Read only.
    pub fn verify(&mut self) -> Result<Vec<String>, String> {
        let l = self.ledger()?;
        let mut drift = Vec::new();
        let settings = l.settings();
        for f in policy::FILES {
            if policy::render(f, &settings).as_deref() != self.read(f).as_deref() {
                drift.push(format!("{f} isn't as Turbine Hub left it"));
            }
        }
        for e in &l.entries {
            let ok = match &e.change {
                Change::Set(Setting::Sysctl { key, value }) => self.read(&Self::proc_path(key)).map(|t| t.trim() == value.to_string()).unwrap_or(false),
                Change::Set(Setting::Sysfs { path, value }) => self.read(path).map(|t| live_word(&t) == *value).unwrap_or(false),
                Change::Set(Setting::Service { unit, enabled }) => self.service_enabled(unit) == *enabled,
                _ => true,
            };
            if !ok {
                drift.push(format!("{} isn't in effect", e.change.words()));
            }
        }
        Ok(drift)
    }

    /// Puts everything in the record back in effect: at boot (/sys forgets) and after a SteamOS update.
    pub fn reapply(&mut self) -> Result<Vec<String>, String> {
        let l = self.ledger()?;
        let mut fixed: Vec<String> = self.render_files(&l)?.into_iter().map(String::from).collect();
        let mut errors = Vec::new();
        for e in &l.entries {
            if let Change::Set(s @ (Setting::Sysctl { .. } | Setting::Sysfs { .. } | Setting::Module { .. })) = &e.change {
                match self.apply_live(s) {
                    Ok(()) => fixed.push(s.words()),
                    Err(er) => errors.push(er),
                }
            }
        }
        if errors.is_empty() { Ok(fixed) } else { Err(errors.join("; ")) }
    }

    /// Sets a /sys value now without recording it (Game Boost while a game runs). Gone at the next boot.
    pub fn set_now(&mut self, path: &str, value: &str) -> Result<(), String> {
        let s = Setting::Sysfs { path: path.into(), value: value.into() };
        s.check()?;
        self.check_live(&s)?;
        self.apply_live(&s)
    }

    pub fn status(&self) -> Result<String, String> {
        let l = self.ledger()?;
        let items: Vec<String> = l.entries.iter().map(|e| format!("{{\"id\":{},\"words\":{},\"applied\":{}}}", e.id, quote(&e.change.words()), e.applied)).collect();
        Ok(format!("{{\"version\":{},\"changes\":[{}]}}", quote(env!("CARGO_PKG_VERSION")), items.join(",")))
    }

    // ---------------------------------------------------------------- Decky Loader

    /// Installs or repairs Decky Loader from its official GitHub release `tag`. The helper downloads it itself and
    /// checks it against the SHA-256 GitHub publishes for the file, so only Decky's own builds ever run as root.
    pub fn decky_install(&mut self, tag: &str, owner: &Owner) -> Result<u64, String> {
        if !decky_tag_ok(tag) {
            return Err("That isn't a Decky Loader release".into());
        }
        let api = format!("https://api.github.com/repos/{DECKY_REPO}/releases/tags/{tag}");
        let text = self.run.run(CURL, &["-fsSL", "--proto", "=https", "--max-time", "30", "-H", "Accept: application/vnd.github+json", &api])?;
        let j = json::parse(&text).map_err(|_| "GitHub's answer wasn't readable".to_string())?;
        let asset = j.get("assets").map(Json::as_array).unwrap_or(&[]).iter().find(|a| a.str_at("name") == Some("PluginLoader")).ok_or("This release has no PluginLoader")?;
        let want_url = format!("https://github.com/{DECKY_REPO}/releases/download/{tag}/PluginLoader");
        if asset.str_at("browser_download_url") != Some(want_url.as_str()) {
            return Err("The release points somewhere unexpected".into());
        }
        let digest = asset.str_at("digest").and_then(|d| d.strip_prefix("sha256:")).filter(|d| sha256::is_hex_digest(d)).ok_or("GitHub doesn't list a SHA-256 for this release, so it isn't installed")?.to_ascii_lowercase();
        let tmp = format!("{STATE_DIR}/PluginLoader.download");
        let tmp_full = self.p(&tmp);
        fs::create_dir_all(self.p(STATE_DIR)).map_err(|e| io_err("make the helper's folder", e))?;
        let _ = fs::remove_file(&tmp_full);
        let tmp_text = tmp_full.to_string_lossy().into_owned();
        self.run.run(CURL, &["-fsSL", "--proto", "=https", "--proto-redir", "=https", "--max-time", "600", "--max-filesize", "300000000", "-o", &tmp_text, &want_url])?;
        let got = sha256::file_hex(&tmp_full).map_err(|e| io_err("read the download", e))?;
        if !sha256::same_digest(&got, &digest) {
            let _ = fs::remove_file(&tmp_full);
            return Err("The download didn't match GitHub's SHA-256, so it wasn't installed".into());
        }

        let home = owner.home.to_string_lossy().into_owned();
        let services = format!("{home}/homebrew/services");
        let binary = format!("{services}/PluginLoader");
        let mut l = self.ledger()?;
        let first = l.find_key("decky").is_none();
        let mut before = Vec::new();
        if first {
            fs::create_dir_all(self.p(BACKUP_DIR)).map_err(|e| io_err("make the backup folder", e))?;
            for (name, path) in [("binary", binary.as_str()), ("unit", DECKY_UNIT)] {
                if self.p(path).is_file() {
                    let b = format!("{BACKUP_DIR}/decky-{}-{name}", l.next);
                    fs::copy(self.p(path), self.p(&b)).map_err(|e| io_err(&format!("back up {path}"), e))?;
                    before.push((name.to_string(), b));
                }
            }
            before.push(("enabled".into(), self.service_enabled("plugin_loader.service").to_string()));
        }
        for d in [format!("{home}/homebrew"), format!("{home}/homebrew/plugins"), format!("{home}/homebrew/settings"), format!("{home}/homebrew/data"), format!("{home}/homebrew/logs")] {
            let p = self.p(&d);
            if !p.exists() {
                fs::create_dir_all(&p).map_err(|e| io_err(&format!("make {d}"), e))?;
                if self.root == Path::new("/") {
                    let _ = std::os::unix::fs::chown(&p, Some(owner.uid), Some(owner.uid));
                }
            }
        }
        // The loader runs as root, so its own folder stays root's: nothing the player's programs can swap.
        fs::create_dir_all(self.p(&services)).map_err(|e| io_err("make Decky's folder", e))?;
        if self.root == Path::new("/") {
            let _ = std::os::unix::fs::chown(self.p(&services), Some(0), Some(0));
        }
        let _ = self.run.run(SYSTEMCTL, &["stop", "plugin_loader.service"]);
        fs::rename(&tmp_full, self.p(&binary)).or_else(|_| fs::copy(&tmp_full, self.p(&binary)).map(|_| ()).and_then(|_| fs::remove_file(&tmp_full))).map_err(|e| io_err("install Decky Loader", e))?;
        fs::set_permissions(self.p(&binary), fs::Permissions::from_mode(0o755)).map_err(|e| io_err("set permissions", e))?;
        self.write_file(&format!("{services}/.loader.version"), tag, 0o644)?;
        self.write_file(DECKY_UNIT, &decky_unit(&home), 0o644)?;
        self.run.run(SYSTEMCTL, &["daemon-reload"])?;
        self.set_service("plugin_loader.service", true)?;
        let steamos = self.steamos();
        let id = l.record(Change::Decky { tag: tag.into() }, before, self.now, &steamos);
        self.save(&l)?;
        Ok(id)
    }

    fn undo_decky(&mut self, e: &Entry) -> Result<(), String> {
        let owner = self.owner().ok_or("Turbine Hub's helper has no owner")?;
        let binary = format!("{}/homebrew/services/PluginLoader", owner.home.to_string_lossy());
        let _ = self.set_service("plugin_loader.service", false);
        for (name, path) in [("binary", binary.as_str()), ("unit", DECKY_UNIT)] {
            match e.before(name) {
                Some(b) if b.starts_with(BACKUP_DIR) && !b.contains("..") => {
                    fs::copy(self.p(b), self.p(path)).map_err(|er| io_err(&format!("put back {path}"), er))?;
                }
                _ => {
                    let _ = fs::remove_file(self.p(path));
                }
            }
        }
        self.run.run(SYSTEMCTL, &["daemon-reload"])?;
        if e.before("enabled") == Some("true") {
            self.set_service("plugin_loader.service", true)?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- Restart to Windows

    /// Makes the next boot (only) start Windows Boot Manager. `entry` is its four-digit number from efibootmgr.
    pub fn boot_next(&mut self, entry: &str) -> Result<(), String> {
        if entry.len() != 4 || !entry.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("That isn't a boot entry".into());
        }
        let list = self.run.run(EFIBOOTMGR, &[])?;
        let Some((_, uuid)) = windows_entries(&list).into_iter().find(|(n, _)| n.eq_ignore_ascii_case(entry)) else {
            return Err("That boot entry isn't Windows".into());
        };
        // The partition it starts from has to be on a drive in this machine, or the next start would hang.
        if !self.p(&format!("/dev/disk/by-partuuid/{uuid}")).exists() {
            return Err("Windows isn't on any drive in this machine any more".into());
        }
        self.run.run(EFIBOOTMGR, &["-n", entry]).map(|_| ())
    }

    // ---------------------------------------------------------------- install and remove

    /// Installs the helper from `from` (the copy inside the Flatpak) for `owner`. Only ever run through sudo with the
    /// player's password: a new helper never arrives without it.
    pub fn install(&mut self, from: &Path, owner: &Owner) -> Result<(), String> {
        if !user_name_ok(&owner.name) {
            return Err("Unexpected user name".into());
        }
        for d in [STATE_DIR, "/var/lib/turbinehub/bin", BACKUP_DIR] {
            fs::create_dir_all(self.p(d)).map_err(|e| io_err(&format!("make {d}"), e))?;
            fs::set_permissions(self.p(d), fs::Permissions::from_mode(0o755)).map_err(|e| io_err("set permissions", e))?;
        }
        let bin = self.p(BIN);
        if fs::canonicalize(from).ok() != fs::canonicalize(&bin).ok() {
            let tmp = bin.with_extension("new");
            fs::copy(from, &tmp).map_err(|e| io_err("copy the helper", e))?;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)).map_err(|e| io_err("set permissions", e))?;
            fs::rename(&tmp, &bin).map_err(|e| io_err("install the helper", e))?;
        }
        self.write_file(OWNER, &format!("{}\n", owner.uid), 0o644)?;
        self.write_file(POLKIT_RULE, &polkit_rule(&owner.name), 0o644)?;
        self.write_file(REAPPLY_UNIT, REAPPLY_UNIT_TEXT, 0o644)?;
        self.run.run(SYSTEMCTL, &["daemon-reload"])?;
        self.run.run(SYSTEMCTL, &["enable", "turbinehub-reapply.service"])?;
        Ok(())
    }

    /// Undoes everything, then removes the helper itself.
    pub fn uninstall(&mut self) -> Result<(), String> {
        self.undo_all()?;
        let _ = self.run.run(SYSTEMCTL, &["disable", "turbinehub-reapply.service"]);
        for f in [POLKIT_RULE, REAPPLY_UNIT] {
            let _ = fs::remove_file(self.p(f));
        }
        let _ = self.run.run(SYSTEMCTL, &["daemon-reload"]);
        fs::remove_dir_all(self.p(STATE_DIR)).map_err(|e| io_err("remove the helper", e))
    }

    pub fn log(&self, line: &str) {
        use std::io::Write;
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(self.p(LOG)) {
            let _ = writeln!(f, "{} {line}", self.now);
        }
    }
}

/// The Windows Boot Manager entries efibootmgr lists as switched on, each with the GPT partition it starts from:
/// (number, partition uuid in lower case). An entry without a partition is left out: firmware keeps entries for a
/// Windows that was wiped long ago, and starting one of those can leave the machine stuck at a black screen.
pub fn windows_entries(efibootmgr: &str) -> Vec<(String, String)> {
    efibootmgr
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("Boot")?;
            let (num, label) = rest.split_at(rest.find(|c: char| !c.is_ascii_hexdigit())?);
            let label = label.strip_prefix('*')?.trim_start();
            if num.len() != 4 || !label.starts_with("Windows Boot Manager") {
                return None;
            }
            let uuid = label.split("GPT,").nth(1)?.split([',', ')']).next()?.to_ascii_lowercase();
            (uuid.len() == 36 && uuid.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')).then(|| (num.to_string(), uuid))
        })
        .collect()
}

pub fn polkit_rule(user: &str) -> String {
    format!(
        "// Turbine Hub: lets {user} run Turbine Hub's helper without a password. Nothing else.\n\
         polkit.addRule(function(action, subject) {{\n    \
         if (action.id == \"org.freedesktop.policykit.exec\" &&\n        \
         action.lookup(\"program\") == \"{BIN}\" &&\n        \
         subject.user == \"{user}\" && subject.local && subject.active) {{\n        \
         return polkit.Result.YES;\n    }}\n}});\n"
    )
}

pub const REAPPLY_UNIT_TEXT: &str = "[Unit]\n\
Description=Turbine Hub: put recorded settings back after boot\n\
After=local-fs.target systemd-udev-settle.service\n\n\
[Service]\nType=oneshot\nExecStart=/var/lib/turbinehub/bin/turbinehub-helper reapply\n\n\
[Install]\nWantedBy=multi-user.target\n";

/// The same unit Decky's own installer writes.
pub fn decky_unit(home: &str) -> String {
    format!(
        "[Unit]\nDescription=SteamDeck Plugin Loader\nAfter=network-online.target\nWants=network-online.target\n\n\
         [Service]\nType=simple\nUser=root\nRestart=always\nKillMode=process\nTimeoutStopSec=15\n\
         ExecStart={home}/homebrew/services/PluginLoader\nWorkingDirectory={home}/homebrew/services\n\
         Environment=UNPRIVILEGED_PATH={home}/homebrew\nEnvironment=PRIVILEGED_PATH={home}/homebrew\nEnvironment=LOG_LEVEL=INFO\n\n\
         [Install]\nWantedBy=multi-user.target\n"
    )
}
