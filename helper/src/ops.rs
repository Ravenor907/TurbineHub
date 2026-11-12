//! The helper's jobs. Every path is under `Sys::root` ("/" for real, a scratch folder in tests) and every program
//! runs through `Run`, so the tests exercise the real code without touching the machine.

use crate::hid;
use crate::json::{self, Json, quote};
use crate::ledger::{Change, Entry, Ledger};
use crate::policy::{self, Setting, Values};
use crate::sha256;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const STATE_DIR: &str = "/var/lib/turbinehub";
pub const BIN: &str = "/var/lib/turbinehub/bin/turbinehub-helper";
pub const LEDGER: &str = "/var/lib/turbinehub/ledger.json";
pub const OWNER: &str = "/var/lib/turbinehub/owner";
pub const LOG: &str = "/var/lib/turbinehub/helper.log";
pub const BACKUP_DIR: &str = "/var/lib/turbinehub/backup";
pub const POLKIT_RULE: &str = "/etc/polkit-1/rules.d/49-turbinehub.rules";
pub const REAPPLY_UNIT: &str = "/etc/systemd/system/turbinehub-reapply.service";
/// SteamOS throws away every change under /etc when it updates, except the files a keep list names. This file adds
/// Turbine Hub's own files to that list (the list's folder is itself kept), so they are still there after an update.
pub const KEEP_LIST: &str = "/etc/atomic-update.conf.d/turbinehub.conf";
pub const DECKY_UNIT: &str = "/etc/systemd/system/plugin_loader.service";
pub const DECKY_REPO: &str = "SteamDeckHomebrew/decky-loader";

pub const SYSTEMCTL: &str = "/usr/bin/systemctl";
pub const UDEVADM: &str = "/usr/bin/udevadm";
pub const MODPROBE: &str = "/usr/bin/modprobe";
pub const EFIBOOTMGR: &str = "/usr/bin/efibootmgr";
pub const CURL: &str = "/usr/bin/curl";

/// The version of Turbine Hub this helper came with (0.5, 0.6 ...), set by the build like the app's. `status`
/// answers it, so a check on a real machine can tell which helper is installed.
pub const VERSION: &str = match option_env!("TURBINEHUB_VERSION") {
    Some(v) => v,
    None => "dev",
};

/// How long a job waits for another job to finish before it gives up. Shorter than the app's own two minutes.
pub const LOCK_WAIT: Duration = Duration::from_secs(60);

/// One job at a time: takes the helper's lock, waiting up to `wait` for the job that holds it. A job never waits
/// for ever: one that can't get the lock says the helper is busy and ends, so nothing that holds the lock (another
/// job, or a job started by this one) can leave the app, or the start of the machine, hanging. The lock file is
/// root's alone: a file everyone may read is a file any program may lock, and that would stop every job. Answers
/// None when there is no lock file to take (before the first install).
pub fn take_lock(dir: &Path, wait: Duration) -> Result<Option<fs::File>, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let Ok(f) = fs::OpenOptions::new().create(true).truncate(false).write(true).mode(0o600).open(dir.join("lock")) else { return Ok(None) };
    // A lock file from an older helper was readable by everyone.
    let _ = f.set_permissions(fs::Permissions::from_mode(0o600));
    let start = Instant::now();
    loop {
        match f.try_lock() {
            Ok(()) => return Ok(Some(f)),
            Err(fs::TryLockError::WouldBlock) if start.elapsed() < wait => std::thread::sleep(Duration::from_millis(20)),
            Err(fs::TryLockError::WouldBlock) => return Err("Turbine Hub's helper is busy with another change. Try again in a moment.".into()),
            Err(fs::TryLockError::Error(_)) => return Ok(None),
        }
    }
}

/// The number of a recorded change from the caller's JSON: a whole number from 1 up, nothing else. (1.9 is not
/// change 1.)
pub fn change_id(j: &Json) -> Result<u64, String> {
    let v = j.get("id").and_then(Json::as_f64).ok_or("id is missing")?;
    if v.fract() != 0.0 || !(1.0..9.0e15).contains(&v) {
        return Err("id must be a whole number".into());
    }
    Ok(v as u64)
}

/// How long one program may run. Downloads carry their own limit (curl's --max-time) and get a little more than
/// it; a service may take its full start or stop time; everything else answers in moments.
pub fn time_limit(prog: &str, args: &[&str]) -> Duration {
    let own = args.iter().position(|a| *a == "--max-time").and_then(|i| args.get(i + 1)?.parse::<u64>().ok());
    Duration::from_secs(match (prog, own) {
        (CURL, Some(secs)) => secs + 30,
        (SYSTEMCTL, _) => 100,
        _ => 20,
    })
}

/// Runs one program to its end, or stops it when it runs past `limit`: a program that waits for something that
/// never comes (udev waiting for this very helper, a service that never starts) must not keep the helper, and the
/// lock it holds, for ever.
pub fn run_limited(prog: &str, args: &[&str], limit: Duration) -> Result<String, String> {
    use std::io::Read;
    use std::process::Stdio;
    let name = prog.rsplit('/').next().unwrap_or(prog);
    let mut child = std::process::Command::new(prog)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Couldn't run {prog} ({e})"))?;
    // Both pipes are read as the program writes, so a program that prints a lot never waits on a full pipe.
    fn reader(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    }
    let (out, err) = (reader(child.stdout.take()), reader(child.stderr.take()));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(if start.elapsed() < Duration::from_millis(200) { 2 } else { 20 })),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{name} took too long, so it was stopped"));
            }
            Err(e) => return Err(format!("Couldn't wait for {prog} ({e})")),
        }
    };
    let text = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
    if status.success() {
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned();
        Err(format!("{name} failed: {}", err.trim().lines().last().unwrap_or("no message")))
    }
}

/// Runs one program with fixed arguments and returns what it printed. Never a shell.
pub trait Run {
    fn run(&mut self, prog: &str, args: &[&str]) -> Result<String, String>;
}

/// The real machine.
pub struct Host;

impl Run for Host {
    fn run(&mut self, prog: &str, args: &[&str]) -> Result<String, String> {
        run_limited(prog, args, time_limit(prog, args))
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

    /// Where a recorded /sys value is now. The kernel numbers its hwmon folders afresh at every start (hwmon6 one
    /// day, hwmon5 the next), so a value recorded under one number is looked for under the number the same device
    /// has today. Anything else is where it was recorded.
    fn sysfs_now(&self, path: &str) -> String {
        if self.p(path).exists() {
            return path.to_string();
        }
        let Some((dir, rest)) = path.split_once("/hwmon/hwmon") else { return path.to_string() };
        let Some((_, file)) = rest.split_once('/') else { return path.to_string() };
        let mut found: Vec<String> = fs::read_dir(self.p(&format!("{dir}/hwmon")))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("hwmon"))
            .map(|n| format!("{dir}/hwmon/{n}/{file}"))
            .filter(|p| policy::sysfs_rule(p).is_some() && self.p(p).exists())
            .collect();
        if found.len() == 1 { found.remove(0) } else { path.to_string() }
    }

    /// The wake switch (power/wakeup) of every USB device plugged in now with this maker and product id.
    fn wake_files(&self, vendor: &str, product: &str) -> Vec<String> {
        let mut out: Vec<String> = fs::read_dir(self.p("/sys/bus/usb/devices"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                let id = |f: &str| fs::read_to_string(e.path().join(f)).map(|t| t.trim().to_ascii_lowercase()).unwrap_or_default();
                id("idVendor") == vendor && id("idProduct") == product
            })
            .map(|e| format!("/sys/bus/usb/devices/{}/power/wakeup", e.file_name().to_string_lossy()))
            .filter(|p| policy::sysfs_rule(p).is_some() && self.sysfs_target(p).is_ok())
            .collect();
        out.sort();
        out
    }

    /// Sets the wake switch of those devices now. The rule under /etc is what lasts (udev applies it when a device
    /// appears); this is so Apply and Undo show at once, without a replug or a restart. A device that can't wake
    /// the machine at all refuses the write, which is fine.
    fn set_wake_now(&self, vendor: &str, product: &str, word: &str) {
        for f in self.wake_files(vendor, product) {
            if let Ok(t) = self.sysfs_target(&f) {
                let _ = fs::write(t, word);
            }
        }
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

    /// What is there now, for the "before" of a new entry. The kernel keeps each dirty-page limit two ways, as bytes
    /// or as a share of memory, and only one is in use: the other reads 0, which the kernel won't take back. So for
    /// the byte limits the share is kept too, and Undo puts that back instead.
    fn before_of(&mut self, s: &Setting) -> Vec<(String, String)> {
        let v = match s {
            Setting::Sysctl { key, .. } => self.read(&Self::proc_path(key)).map(|t| t.trim().to_string()),
            Setting::Sysfs { path, .. } => self.read(path).map(|t| live_word(&t)),
            Setting::Service { unit, .. } => Some(self.service_enabled(unit).to_string()),
            // Whether the device may wake the machine now, so Undo can say so again. Not plugged in: nothing to keep.
            Setting::Wake { vendor, product, .. } => self.wake_files(vendor, product).first().and_then(|f| self.read(f)).map(|t| t.trim().to_string()).filter(|v| WAKE_WORDS.contains(&v.as_str())),
            _ => None,
        };
        let mut out: Vec<(String, String)> = v.map(|v| vec![(if matches!(s, Setting::Service { .. }) { "enabled" } else { "value" }.to_string(), v)]).unwrap_or_default();
        if let Setting::Sysctl { key, .. } = s
            && let Some((ratio, _)) = ratio_of(key)
            && let Some(r) = self.read(&Self::proc_path(ratio)).map(|t| t.trim().to_string())
        {
            out.push(("ratio".into(), r));
        }
        out
    }

    /// Puts a kernel setting back from its "before". A byte limit that was 0 (not in use) goes back through its
    /// share of memory instead, which turns the byte limit off again. Records from before the share was kept get the
    /// kernel's own default share.
    fn restore_sysctl(&mut self, key: &str, e: &Entry) -> Result<(), String> {
        let write = |sys: &Self, k: &str, v: &str| fs::write(sys.p(&Self::proc_path(k)), format!("{v}\n")).map_err(|er| io_err(&format!("set {k}"), er));
        match (e.before("value"), ratio_of(key)) {
            (Some("0"), Some((ratio, default))) => {
                let share = e.before("ratio").filter(|v| v.parse::<u64>().is_ok_and(|n| n > 0 && n <= 100)).unwrap_or(default);
                write(self, ratio, share)
            }
            (Some(v), _) => write(self, key, v),
            (None, _) => Ok(()),
        }
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
                let path = &self.sysfs_now(path);
                let t = self.sysfs_target(path)?;
                fs::write(t, value).map_err(|e| io_err(&format!("set {path}"), e))
            }
            Setting::Service { unit, enabled } => self.set_service(unit, *enabled),
            Setting::Module { name } => self.run.run(MODPROBE, &[name]).map(|_| ()),
            Setting::Wake { vendor, product, wake } => {
                self.set_wake_now(vendor, product, if *wake { "enabled" } else { "disabled" });
                Ok(())
            }
            Setting::Access { .. } | Setting::Zram { .. } => Ok(()),
        }
    }

    /// Puts back what was there before.
    fn restore_live(&mut self, e: &Entry) -> Result<(), String> {
        match &e.change {
            Change::Set(Setting::Sysctl { key, .. }) => self.restore_sysctl(key, e),
            Change::Set(Setting::Sysfs { path, .. }) => {
                let path = &self.sysfs_now(path);
                match e.before("value") {
                    // The device is gone (a battery, a card, a USB device taken out): there is nothing to put back,
                    // and the change must still leave the record instead of staying there for ever.
                    Some(_) if !self.p(path).exists() => Ok(()),
                    Some(v) => self.sysfs_target(path).and_then(|t| fs::write(t, v).map_err(|er| io_err(&format!("set {path}"), er))),
                    None => Ok(()),
                }
            }
            Change::Set(Setting::Service { unit, .. }) => {
                let was = e.before("enabled") == Some("true");
                self.set_service(unit, was)
            }
            Change::Set(Setting::Wake { vendor, product, .. }) => {
                if let Some(v) = e.before("value").filter(|v| WAKE_WORDS.contains(v)) {
                    self.set_wake_now(vendor, product, v);
                }
                Ok(())
            }
            Change::Set(Setting::Module { .. } | Setting::Access { .. } | Setting::Zram { .. }) => Ok(()),
            Change::Decky { .. } => self.undo_decky(e),
        }
    }

    /// Writes the files under /etc from the ledger and reloads what reads them. Returns the files changed.
    /// Puts back the helper's own two files when they are gone or differ: the rule that lets the owner start the
    /// helper without the password, and the keep list. A SteamOS update drops both unless the keep list was already
    /// there, and without the rule every change would ask for the password again (or fail in Game Mode, where
    /// nothing can ask). Run at every start by the reapply unit, which SteamOS keeps. Answers what it wrote.
    pub fn own_files(&mut self) -> Vec<&'static str> {
        let mut wrote = Vec::new();
        if let Some(owner) = self.owner() {
            let rule = polkit_rule(&owner.name);
            if self.read(POLKIT_RULE).as_deref() != Some(rule.as_str()) && self.write_file(POLKIT_RULE, &rule, 0o644).is_ok() {
                wrote.push(POLKIT_RULE);
            }
        }
        let keep = keep_list();
        if self.read(KEEP_LIST).as_deref() != Some(keep.as_str()) && self.write_file(KEEP_LIST, &keep, 0o644).is_ok() {
            wrote.push(KEEP_LIST);
        }
        wrote
    }

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
                // Never --settle here: the lighting rule starts this helper again for each device, and a job that
                // waited for those to finish would wait on itself.
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
        if matches!(s, Setting::Access { what: policy::Access::Lighting }) {
            self.open_lights();
        }
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
                Change::Set(Setting::Sysfs { path, value }) => self.read(&self.sysfs_now(path)).map(|t| live_word(&t) == *value).unwrap_or(false),
                Change::Set(Setting::Service { unit, enabled }) => self.service_enabled(unit) == *enabled,
                // Every such device plugged in now (none is fine: the rule waits for it).
                Change::Set(Setting::Wake { vendor, product, wake }) => self.wake_files(vendor, product).iter().all(|f| match self.read(f).as_deref().map(str::trim) {
                    Some(v) if WAKE_WORDS.contains(&v) => (v == "enabled") == *wake,
                    _ => true,
                }),
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
        let mut fixed: Vec<String> = self.own_files().into_iter().map(String::from).collect();
        fixed.extend(self.render_files(&l)?.into_iter().map(String::from));
        let mut errors = Vec::new();
        for e in &l.entries {
            if let Change::Set(s @ (Setting::Sysctl { .. } | Setting::Sysfs { .. } | Setting::Module { .. })) = &e.change {
                match self.apply_live(s) {
                    Ok(()) => fixed.push(s.words()),
                    Err(er) => errors.push(er),
                }
            }
        }
        // udev may have met the lights before the helper's folder was there at start, or the controller service may
        // have closed them again: open them now.
        self.open_lights();
        if errors.is_empty() { Ok(fixed) } else { Err(errors.join("; ")) }
    }

    /// Opens one hidraw interface for writing by everyone, when it is an ASUS lighting interface (see `hid`) and
    /// the player asked for the lighting access rule. Run by that rule's udev line for each ASUS hidraw device as it
    /// appears (at start, and again after a sleep that brings the controller back), and by `reapply`. Only the write
    /// bit is added, which is all the lighting commands need (output reports are written, feature reports go
    /// through an ioctl that works on a node opened for writing): Turbine Hub never makes the interface readable,
    /// so nothing typed or pressed can be seen through it because of this. Every other bit stays as it was: another
    /// lighting app on the same machine (Ally Hub opens these interfaces for reading too) keeps what it set.
    /// Answers whether the interface was one of them.
    pub fn hid_access(&self, name: &str) -> Result<bool, String> {
        if !hid::node_name_ok(name) {
            return Err("Not a hidraw device".into());
        }
        if self.ledger()?.find_key(&Setting::Access { what: policy::Access::Lighting }.key()).is_none() {
            return Ok(false);
        }
        let dir = self.p(&format!("/sys/class/hidraw/{name}/device"));
        let uevent = fs::read_to_string(dir.join("uevent")).unwrap_or_default();
        let desc = fs::read(dir.join("report_descriptor")).unwrap_or_default();
        if hid::role(&uevent, &desc).is_none() {
            return Ok(false);
        }
        let node = self.p(&format!("/dev/{name}"));
        let meta = fs::symlink_metadata(&node).map_err(|e| io_err(&format!("look at /dev/{name}"), e))?;
        if meta.file_type().is_symlink() || meta.is_dir() {
            return Err(format!("/dev/{name} isn't a device"));
        }
        let mode = (meta.permissions().mode() & 0o7777) | 0o002;
        fs::set_permissions(&node, fs::Permissions::from_mode(mode)).map_err(|e| io_err(&format!("open /dev/{name} for lighting"), e))?;
        Ok(true)
    }

    /// `hid_access` for every hidraw interface there is now.
    /// Opens every RGB light's color and brightness files for writing by everyone, right now, when the player asked
    /// for the lighting access rule. The udev rule does the same as lights appear, but udev can be slow, and on some
    /// handhelds the controller service puts the files back as they were after a boot or a sleep; this runs when the
    /// rule is set, at every boot and after every wake. Only lights (folders under /sys/class/leds with a
    /// multi_intensity file) and only those two files, plus the light's own "effect" and "mode" files where it has
    /// them (a light left in an effect of its own ignores the colors written to it, so that has to be set back to
    /// plain colors); links pointing out of /sys are refused.
    pub fn open_lights(&self) {
        if !matches!(self.ledger().map(|l| l.find_key(&Setting::Access { what: policy::Access::Lighting }.key()).is_some()), Ok(true)) {
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let Ok(sys) = fs::canonicalize(self.p("/sys")) else { return };
        for e in fs::read_dir(self.p("/sys/class/leds")).into_iter().flatten().flatten() {
            let dir = e.path();
            if !dir.join("multi_intensity").exists() {
                continue;
            }
            for f in ["multi_intensity", "brightness", "effect", "mode"] {
                let Ok(real) = fs::canonicalize(dir.join(f)) else { continue };
                if !real.starts_with(&sys) || !real.is_file() {
                    continue;
                }
                if let Ok(m) = fs::metadata(&real) {
                    let _ = fs::set_permissions(&real, fs::Permissions::from_mode((m.permissions().mode() & 0o777) | 0o666));
                }
            }
        }
        self.hid_access_all();
    }

    fn hid_access_all(&self) {
        for e in fs::read_dir(self.p("/sys/class/hidraw")).into_iter().flatten().flatten() {
            let _ = self.hid_access(&e.file_name().to_string_lossy());
        }
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
        Ok(format!("{{\"version\":{},\"changes\":[{}]}}", quote(VERSION), items.join(",")))
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
        self.write_file(KEEP_LIST, &keep_list(), 0o644)?;
        self.run.run(SYSTEMCTL, &["daemon-reload"])?;
        self.run.run(SYSTEMCTL, &["enable", "turbinehub-reapply.service"])?;
        Ok(())
    }

    /// Undoes everything, then removes the helper itself.
    pub fn uninstall(&mut self) -> Result<(), String> {
        self.undo_all()?;
        let _ = self.run.run(SYSTEMCTL, &["disable", "turbinehub-reapply.service"]);
        for f in [POLKIT_RULE, REAPPLY_UNIT, KEEP_LIST] {
            let _ = fs::remove_file(self.p(f));
        }
        let _ = self.run.run(SYSTEMCTL, &["daemon-reload"]);
        fs::remove_dir_all(self.p(STATE_DIR)).map_err(|e| io_err("remove the helper", e))
    }

    pub fn log(&self, line: &str) {
        use std::io::Write;
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(self.p(LOG)) {
            let _ = writeln!(f, "{} {}", self.now, log_text(line));
        }
    }
}

/// The two words a wake switch takes.
const WAKE_WORDS: [&str; 2] = ["enabled", "disabled"];

/// One line for the log: whatever the caller sent (a job name, a path in a refusal) stays on that one line, with
/// nothing in it that could pass for a line of its own, and never longer than a line should be.
pub fn log_text(line: &str) -> String {
    line.chars().take(400).map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// The share-of-memory setting that stands in for a byte limit while that limit is off (reads 0), and the kernel's
/// own default for it.
pub fn ratio_of(key: &str) -> Option<(&'static str, &'static str)> {
    match key {
        "vm.dirty_bytes" => Some(("vm.dirty_ratio", "20")),
        "vm.dirty_background_bytes" => Some(("vm.dirty_background_ratio", "10")),
        _ => None,
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

/// The keep list's text: every file Turbine Hub writes under /etc, one per line, and nothing else.
pub fn keep_list() -> String {
    let mut t = String::from("## Turbine Hub: its own settings files, kept when SteamOS updates. Removed with Turbine Hub's helper.\n");
    for f in policy::FILES.iter().copied().chain([POLKIT_RULE, REAPPLY_UNIT]) {
        t.push_str(f);
        t.push('\n');
    }
    t
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
After=local-fs.target systemd-udev-settle.service suspend.target hibernate.target hybrid-sleep.target suspend-then-hibernate.target\n\n\
[Service]\nType=oneshot\nExecStart=/var/lib/turbinehub/bin/turbinehub-helper reapply\n\n\
[Install]\nWantedBy=multi-user.target suspend.target hibernate.target hybrid-sleep.target suspend-then-hibernate.target\n";

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
