//! Turbine Hub's own logs: what they hold, what is hidden in them and how they are kept small.
//!
//! Two files in Turbine Hub's data folder, under `logs`: `app.log` (what the app did in this run and the ones
//! before: presses, the steps of a confirmed dialog and how they ended, what the host answered, Qt's warnings) and
//! `crash.log` (panics, Qt's fatal errors and runs that ended without closing). `app.log` starts over at 1 MB (the
//! one before is kept as `app.log.1`), `crash.log` is cut to its newer half at 200 KB.
//!
//! Nothing private is written: the home folder becomes "~", the player's user name "(user)", and anything that looks
//! like a key, a password, a network address or a mail address "(hidden)" (see `Private::scrub`). Nothing is ever
//! sent anywhere: the files only leave the machine inside a report the player writes and shares by hand.
//!
//! The app's `log.rs` holds the open file, the clock, the panic hook and Qt's message handler; every decision is
//! here.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const FILE: &str = "app.log";
pub const CRASH: &str = "crash.log";
/// `app.log` starts over past this size.
pub const FILE_CAP: u64 = 1024 * 1024;
/// `crash.log` is cut to its newer half past this size.
pub const CRASH_CAP: u64 = 200 * 1024;
/// How much of the logs goes into a report.
pub const REPORT_LINES: usize = 200;
pub const REPORT_CRASH_BYTES: usize = 8 * 1024;

const RUNNING: &str = "running.mark";
const DISPLAY_GONE: &str = "display-gone.mark";

/// Where the logs are kept: `logs` in Turbine Hub's data folder.
pub fn dir(data: &Path) -> PathBuf {
    data.join("logs")
}

// ------------------------------------------------------------------------------------------------ what is hidden

/// What must never show in a log line: the player's home folder and user name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Private {
    homes: Vec<String>,
    user: String,
}

impl Private {
    /// `home`: the player's home folder. `user`: the user name when the system gave one (else the home folder's
    /// own name is taken for it, where the folder is one of /home's).
    pub fn new(home: &Path, user: Option<&str>) -> Private {
        let in_home = home.parent().is_some_and(|p| p == Path::new("/home") || p == Path::new("/var/home"));
        let from_home = home.file_name().filter(|_| in_home).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let user = user.map(str::trim).filter(|u| !u.is_empty()).map(String::from).unwrap_or(from_home);
        let mut homes = Vec::new();
        let home = home.to_string_lossy().trim_end_matches('/').to_string();
        if home.len() > 1 {
            homes.push(home);
        }
        // The same folder under the names SteamOS and other systems give it.
        if user.len() > 1 && user != "root" {
            for base in ["/var/home/", "/home/"] {
                let h = format!("{base}{user}");
                if !homes.contains(&h) {
                    homes.push(h);
                }
            }
        }
        // The longest first, so "/var/home/x" never leaves "/var" behind.
        homes.sort_by_key(|h| std::cmp::Reverse(h.len()));
        Private { homes, user: if user == "root" { String::new() } else { user } }
    }

    /// One line as it may be written: on one line, the home folder as "~", the user name as "(user)", and keys,
    /// passwords and addresses as "(hidden)".
    pub fn scrub(&self, text: &str) -> String {
        let mut out = text.replace(['\n', '\r', '\t'], " ");
        for h in &self.homes {
            out = replace_path(&out, h);
        }
        if self.user.len() >= 2 {
            out = replace_word(&out, &self.user, "(user)");
        }
        let out = crate::report::redact(&out);
        out.split(' ').map(|tok| if looks_like_address(tok) { "(hidden)".to_string() } else { strip_query(tok) }).collect::<Vec<_>>().join(" ")
    }
}

/// Replaces a folder's path by "~" where it stands as a whole path or the start of one.
fn replace_path(text: &str, path: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(path) {
        let after = rest[i + path.len()..].chars().next();
        out.push_str(&rest[..i]);
        // "/home/sam" inside "/home/samantha" is another folder.
        if after.is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.') {
            out.push('~');
        } else {
            out.push_str(path);
        }
        rest = &rest[i + path.len()..];
    }
    out.push_str(rest);
    out
}

/// Replaces `word` where it stands alone (not inside a longer word).
fn replace_word(text: &str, word: &str, with: &str) -> String {
    let inside = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut before: Option<char> = None;
    while let Some(i) = rest.find(word) {
        let prev = rest[..i].chars().next_back().or(before);
        let after = rest[i + word.len()..].chars().next();
        out.push_str(&rest[..i]);
        if prev.is_some_and(inside) || after.is_some_and(inside) {
            out.push_str(word);
        } else {
            out.push_str(with);
        }
        before = word.chars().next_back();
        rest = &rest[i + word.len()..];
    }
    out.push_str(rest);
    out
}

fn is_ipv4(t: &str) -> bool {
    // With a port after it too.
    let host = match t.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => t,
    };
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok_and(|n| n <= 255))
}

fn is_mac(t: &str) -> bool {
    let sep = if t.contains(':') { ':' } else { '-' };
    let parts: Vec<&str> = t.split(sep).collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn is_ipv6(t: &str) -> bool {
    let t = t.split('%').next().unwrap_or("").trim_start_matches('[').trim_end_matches(']');
    let colons = t.matches(':').count();
    // A clock time has two colons and a USB id one: an address has a "::" or at least four of them.
    (t.contains("::") || colons >= 4) && t.len() >= 3 && t.chars().all(|c| c.is_ascii_hexdigit() || c == ':') && t.chars().any(|c| c.is_ascii_hexdigit()) && t.split(':').all(|g| g.len() <= 4)
}

fn is_mail(t: &str) -> bool {
    t.split_once('@').is_some_and(|(name, host)| !name.is_empty() && host.contains('.') && !host.starts_with('.') && !host.ends_with('.'))
}

/// Does this word look like a network or mail address? The word may carry punctuation around it, and an address
/// inside a web link counts too.
pub fn looks_like_address(tok: &str) -> bool {
    let core = tok.trim_matches(|c: char| "\"',;()<>{}".contains(c)).trim_end_matches(['.', ':']);
    if core.is_empty() {
        return false;
    }
    if is_ipv4(core) || is_mac(core) || is_ipv6(core) || is_mail(core) {
        return true;
    }
    // A link to a machine by its number, or with a name and password in it.
    if let Some((_, rest)) = core.split_once("://") {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        return host.contains('@') || is_ipv4(host) || is_ipv6(host);
    }
    false
}

/// A web link without what follows its question mark (search words, tokens).
fn strip_query(tok: &str) -> String {
    match (tok.contains("://"), tok.split_once('?')) {
        (true, Some((link, _))) => format!("{link}?(hidden)"),
        _ => tok.to_string(),
    }
}

// ------------------------------------------------------------------------------------------------ the files

/// The open `app.log`, with its size so it starts over at the cap.
pub struct Log {
    dir: PathBuf,
    file: Option<File>,
    size: u64,
    cap: u64,
}

impl Log {
    /// Opens (and makes) `app.log` in `dir`. A folder that can't be written leaves a log that keeps nothing.
    pub fn open(dir: &Path) -> Log {
        Log::open_with(dir, FILE_CAP)
    }

    pub fn open_with(dir: &Path, cap: u64) -> Log {
        let _ = fs::create_dir_all(dir);
        let path = dir.join(FILE);
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Log { dir: dir.to_path_buf(), file: OpenOptions::new().create(true).append(true).open(&path).ok(), size, cap }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Adds one line (already scrubbed). Past the cap the file becomes `app.log.1` and a new one starts, so the
    /// two together never hold more than twice the cap.
    pub fn add(&mut self, entry: &str) {
        let bytes = entry.len() as u64 + 1;
        if self.size + bytes > self.cap && self.size > 0 {
            self.file = None;
            let path = self.dir.join(FILE);
            let _ = fs::rename(&path, self.dir.join(format!("{FILE}.1")));
            self.file = OpenOptions::new().create(true).write(true).truncate(true).open(&path).ok();
            self.size = 0;
        }
        if let Some(f) = self.file.as_mut()
            && writeln!(f, "{entry}").is_ok()
        {
            self.size += bytes;
        }
    }
}

/// One log line as written: when, what it is about, the words.
pub fn entry(stamp: &str, kind: &str, text: &str) -> String {
    format!("{stamp} {kind:<7} {text}")
}

/// The last `n` lines of `app.log`, oldest first: earlier runs too, so the lines before a crash are there. Reads at
/// most the file's last 256 KB.
pub fn last_lines(dir: &Path, n: usize) -> Vec<String> {
    let read = || -> Option<Vec<String>> {
        let mut f = File::open(dir.join(FILE)).ok()?;
        let len = f.metadata().ok()?.len();
        let from = len.saturating_sub(256 * 1024);
        f.seek(SeekFrom::Start(from)).ok()?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).ok()?;
        let text = String::from_utf8_lossy(&buf).into_owned();
        // A read that starts in the middle of the file starts in the middle of a line: that piece is dropped.
        let mut lines: Vec<String> = text.lines().skip(usize::from(from > 0)).map(str::to_string).collect();
        lines.drain(..lines.len().saturating_sub(n));
        Some(lines)
    };
    read().unwrap_or_default()
}

/// Adds one line (already scrubbed) to `crash.log`.
pub fn add_crash(dir: &Path, stamp: &str, text: &str) {
    let _ = fs::create_dir_all(dir);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(dir.join(CRASH)) {
        let _ = writeln!(f, "{stamp} {text}");
    }
}

/// Cuts `crash.log` to the newer half of `cap` once it is past it, at a line's start.
pub fn trim_crash(dir: &Path, cap: u64) {
    let path = dir.join(CRASH);
    let Ok(meta) = fs::metadata(&path) else { return };
    if meta.len() <= cap {
        return;
    }
    if let Ok(all) = fs::read(&path) {
        let keep = &all[all.len() - (cap as usize / 2).min(all.len())..];
        let start = keep.iter().position(|b| *b == b'\n').map(|i| i + 1).unwrap_or(0);
        let _ = fs::write(&path, &keep[start..]);
    }
}

/// The last lines of `crash.log`, at most `max` bytes of them, oldest first.
pub fn crash_tail(dir: &Path, max: usize) -> Vec<String> {
    let text = fs::read(dir.join(CRASH)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    if text.len() <= max {
        return text.lines().map(String::from).collect();
    }
    let mut at = text.len() - max;
    while !text.is_char_boundary(at) {
        at += 1;
    }
    let cut = &text[at..];
    cut.find('\n').map(|i| &cut[i + 1..]).unwrap_or(cut).lines().map(String::from).collect()
}

// ------------------------------------------------------------------------------------------------ how a run ended

/// How the run before this one ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastRun {
    /// It closed, or there was none.
    Closed,
    /// The display went away under it (Game Mode ended, Steam restarted): not a crash.
    DisplayGone,
    /// It ended without closing: a crash, or it was stopped.
    Unclean,
}

/// Called once when a window run starts: answers how the last one ended and marks this one as running.
pub fn begin(dir: &Path) -> LastRun {
    let _ = fs::create_dir_all(dir);
    let gone = fs::remove_file(dir.join(DISPLAY_GONE)).is_ok();
    let unclean = dir.join(RUNNING).exists();
    let _ = fs::write(dir.join(RUNNING), b"");
    if gone {
        LastRun::DisplayGone
    } else if unclean {
        LastRun::Unclean
    } else {
        LastRun::Closed
    }
}

/// This run is closing as it should.
pub fn end(dir: &Path) {
    let _ = fs::remove_file(dir.join(RUNNING));
}

/// The display went away and Qt ends the run right after: the next start must not call it a crash.
pub fn display_gone(dir: &Path) {
    let _ = fs::remove_file(dir.join(RUNNING));
    let _ = fs::write(dir.join(DISPLAY_GONE), b"");
}

// ------------------------------------------------------------------------------------------------ Qt's messages

/// What to do with one message from Qt or QML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QtLine {
    /// Qt's own chatter: not kept.
    Skip,
    /// The display went away (Game Mode ended or Steam restarted). Qt closes the app right after; it isn't a
    /// crash.
    DisplayGone,
    /// A fatal error: into the crash log.
    Fatal(String),
    /// A warning, an error or a line a page printed: into the app's log.
    Line(String),
}

/// Sorts one Qt message. `kind`: 0 debug, 1 warning, 2 critical, 3 fatal, 4 info. `place`: the file and line or the
/// category it came from.
pub fn qt_line(kind: i32, place: &str, text: &str) -> QtLine {
    // Leaving Game Mode takes the display away, and Qt says so in a warning or as its last, fatal words.
    if matches!(kind, 1..=3) && (text.contains("X11 connection broke") || text.contains("The Wayland connection broke") || text.contains("The Wayland connection experienced a fatal error")) {
        return QtLine::DisplayGone;
    }
    let from_qml = place.contains(".qml") || text.contains(".qml:") || place == "qml" || place == "js";
    let label = match kind {
        1 => "warning",
        2 => "critical",
        3 => "fatal",
        _ if from_qml => "console",
        _ => return QtLine::Skip,
    };
    let msg = if place.is_empty() { format!("{label}: {text}") } else { format!("{label}: {text} ({place})") };
    if kind == 3 { QtLine::Fatal(format!("Qt {msg}")) } else { QtLine::Line(msg) }
}

/// A panic as one crash line.
pub fn panic_line(thread: &str, place: &str, what: &str) -> String {
    let place = if place.is_empty() { String::new() } else { format!(" at {place}") };
    format!("panic in thread {thread}{place}: {what}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-applog-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn the_home_folder_the_user_name_keys_and_addresses_never_reach_a_line() {
        let p = Private::new(Path::new("/home/samwise"), None);
        assert_eq!(p.scrub("Read /home/samwise/.local/share/Steam/steamapps and /var/home/samwise/Games"), "Read ~/.local/share/Steam/steamapps and ~/Games");
        // Another folder that only starts the same way stays as it is; the name alone goes anywhere it stands.
        assert_eq!(p.scrub("/home/samwisely/x and /run/media/samwise/card, owner samwise."), "/home/samwisely/x and /run/media/(user)/card, owner (user).");
        assert_eq!(p.scrub("line one\nline two\ttab"), "line one line two tab");
        // Keys and passwords (the report's own rules), then addresses of every kind.
        assert_eq!(p.scrub("curl said token=abc123 and stopped"), "curl said token=(hidden) and stopped");
        assert!(!p.scrub("header Authorization: Bearer abcdef").contains("abcdef"));
        assert!(!p.scrub("got 9f8e7d6c5b4a39281706f5e4d3c2b1a0ff").contains("9f8e7d6c"));
        assert_eq!(p.scrub("host 192.168.1.20 answered, then 10.0.0.2:8080."), "host (hidden) answered, then (hidden)");
        assert_eq!(p.scrub("wake from a4:5e:60:c1:22:0f"), "wake from (hidden)");
        assert_eq!(p.scrub("via fe80::1c2d:3e4f:5a6b:7c8d%wlan0 and ::1"), "via (hidden) and (hidden)");
        assert_eq!(p.scrub("mail sam.gamgee@example.org now"), "mail (hidden) now");
        assert_eq!(p.scrub("GET https://api.github.com/repos/x/y/releases?per_page=5&access=1 ok"), "GET https://api.github.com/repos/x/y/releases?(hidden) ok");
        assert_eq!(p.scrub("GET http://192.168.1.9:8080/list and ftp://sam:pw@files.example.org/x"), "GET (hidden) and (hidden)");
        // What must stay readable: times, versions, USB ids, paths, sizes.
        let plain = "2026-11-12 14:03:22.120 step    Host flatpak install 0.3 3.9.5 0b05:1abe /sys/class/leds/x:rgb 1280x800";
        assert_eq!(p.scrub(plain), plain);
        // The system's own user name wins over the folder's.
        let p = Private::new(Path::new("/home/deck"), Some("frodo"));
        assert_eq!(p.scrub("/home/deck/x for frodo, on a Steam Deck with Decky"), "~/x for (user), on a Steam Deck with Decky");
        // A home that is only "/" (or root's) hides nothing by accident.
        assert_eq!(Private::new(Path::new("/"), Some("root")).scrub("/usr/bin/root is fine"), "/usr/bin/root is fine");
        // A home folder somewhere else (a test's, a made-up machine's) names no user.
        assert_eq!(Private::new(Path::new("/tmp/sample/home"), None).scrub("the home page in /tmp/sample/home/x"), "the home page in ~/x");
    }

    #[test]
    fn the_log_starts_over_at_its_cap_and_keeps_the_one_before() {
        let d = tmp("cap");
        let mut log = Log::open_with(&d, 200);
        for i in 0..40 {
            log.add(&entry("2026-11-12 10:00:00.000", "step", &format!("line number {i}")));
        }
        let now = fs::metadata(d.join(FILE)).unwrap().len();
        let before = fs::metadata(d.join("app.log.1")).unwrap().len();
        assert!(now <= 200 && before <= 200, "{now} {before}");
        // Only two files, ever.
        assert_eq!(fs::read_dir(&d).unwrap().count(), 2);
        assert_eq!(last_lines(&d, 2).last().map(String::as_str), Some("2026-11-12 10:00:00.000 step    line number 39"));
        // Opened again (the next run), it carries on in the same file and still honors the cap.
        let mut log = Log::open_with(&d, 200);
        log.add("one more");
        assert_eq!(last_lines(&d, 1), vec!["one more".to_string()]);
        assert!(fs::metadata(d.join(FILE)).unwrap().len() <= 200);
        assert!(last_lines(&tmp("none"), 5).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_crash_log_keeps_its_newer_half() {
        let d = tmp("crash");
        for i in 0..100 {
            add_crash(&d, "2026-11-12 10:00:00.000", &format!("panic number {i:03}"));
        }
        let all = fs::metadata(d.join(CRASH)).unwrap().len();
        trim_crash(&d, all + 1);
        assert_eq!(fs::metadata(d.join(CRASH)).unwrap().len(), all);
        trim_crash(&d, 1000);
        let text = fs::read_to_string(d.join(CRASH)).unwrap();
        assert!(text.len() <= 500 && text.starts_with("2026-11-12") && text.ends_with("panic number 099\n"), "{text}");
        let tail = crash_tail(&d, 100);
        assert!(tail.len() == 2 && tail[1].ends_with("panic number 099") && tail[0].starts_with("2026"), "{tail:?}");
        assert!(crash_tail(&tmp("no-crash"), 100).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_run_that_never_closed_is_told_apart_from_game_mode_ending() {
        let d = tmp("runs");
        assert_eq!(begin(&d), LastRun::Closed);
        end(&d);
        assert_eq!(begin(&d), LastRun::Closed);
        // No end: the next start sees a run that never closed.
        assert_eq!(begin(&d), LastRun::Unclean);
        // The display went away under it: said once, and not as a crash.
        display_gone(&d);
        assert_eq!(begin(&d), LastRun::DisplayGone);
        end(&d);
        assert_eq!(begin(&d), LastRun::Closed);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn qt_messages_are_sorted_and_the_display_going_away_is_no_crash() {
        assert_eq!(qt_line(1, "", "The X11 connection broke (error 1). Did the X11 server die?"), QtLine::DisplayGone);
        assert_eq!(qt_line(3, "", "The Wayland connection broke. Did the Wayland compositor die?"), QtLine::DisplayGone);
        assert_eq!(qt_line(3, "qrc:/x.cpp:3", "Could not load the Qt platform plugin"), QtLine::Fatal("Qt fatal: Could not load the Qt platform plugin (qrc:/x.cpp:3)".into()));
        assert_eq!(qt_line(1, "qrc:/qt/qml/TurbineHub/qml/Main.qml:40", "TypeError: x is undefined"), QtLine::Line("warning: TypeError: x is undefined (qrc:/qt/qml/TurbineHub/qml/Main.qml:40)".into()));
        assert_eq!(qt_line(0, "qml", "hello"), QtLine::Line("console: hello (qml)".into()));
        // Qt's own debug and info chatter stays out.
        assert_eq!(qt_line(0, "qt.qpa.xcb", "something"), QtLine::Skip);
        assert_eq!(qt_line(4, "", "info"), QtLine::Skip);
        assert_eq!(panic_line("main", "app/src/run.rs:10", "boom"), "panic in thread main at app/src/run.rs:10: boom");
    }
}
