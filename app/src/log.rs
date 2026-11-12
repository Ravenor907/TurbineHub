//! Turbine Hub's own log and crash log, for the window run only.
//!
//! `app.log`: what the app did (presses by their action, the steps of a confirmed dialog and how each ended, what
//! the host answered, update checks, Qt's and QML's warnings), one timestamped line each. `crash.log`: panics, Qt's
//! fatal errors and runs that ended without closing. Both live in `logs` in Turbine Hub's data folder, are kept
//! small, and never hold the home folder, the user name, a key, a password or an address. Nothing is sent anywhere:
//! the last lines go into a report only when the player writes one.
//!
//! Every rule (what is hidden, the size caps, what counts as a crash) is the engine's `applog`; this file holds the
//! open file, the clock, the panic hook and Qt's message handler. Before `start` (and in runs that never call it:
//! pictures, made-up machines, the background helper, `--json`) every call does nothing.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use turbinehub_engine::applog::{self, LastRun, Log, Private, QtLine};

struct State {
    log: Option<Log>,
    private: Private,
}

fn state() -> MutexGuard<'static, State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    let m = S.get_or_init(|| Mutex::new(State { log: None, private: Private::default() }));
    // A panic while the lock was held must not silence the log for the rest of the run.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Opens the logs in `dir` (made if needed), says how the last run ended and catches panics from here on. Called
/// once, at the start of a window run on the real machine.
pub fn start(dir: PathBuf) {
    let user = std::env::var("USER").ok();
    let private = Private::new(&turbinehub_engine::home_dir(), user.as_deref());
    let last = applog::begin(&dir);
    applog::trim_crash(&dir, applog::CRASH_CAP);
    {
        let mut s = state();
        s.private = private;
        s.log = Some(Log::open(&dir));
    }
    match last {
        LastRun::Closed => {}
        LastRun::DisplayGone => line("app", "The last run closed with the display (Game Mode ended or Steam restarted), not a crash"),
        LastRun::Unclean => crash("The last run ended without closing (a crash, or it was stopped)."),
    }
    install_panic_hook();
}

/// This run is closing as it should.
pub fn clean_exit() {
    line("app", "Closed");
    if let Some(dir) = dir() {
        applog::end(&dir);
    }
}

fn dir() -> Option<PathBuf> {
    state().log.as_ref().map(|l| l.dir().to_path_buf())
}

/// One line in the log: `kind` says what it is about ("app", "act", "step", "update", "steam", "qt" ...).
pub fn line(kind: &str, text: &str) {
    // The words are cleaned outside the lock, so nothing that goes wrong there can hold the log shut.
    let private = {
        let s = state();
        if s.log.is_none() {
            return;
        }
        s.private.clone()
    };
    let entry = applog::entry(&now_stamp(), kind, &private.scrub(text));
    if let Some(log) = state().log.as_mut() {
        log.add(&entry);
    }
}

/// One line in the crash log (and in the log).
pub fn crash(text: &str) {
    line("crash", text);
    let (dir, private) = {
        let s = state();
        (s.log.as_ref().map(|l| l.dir().to_path_buf()), s.private.clone())
    };
    if let Some(dir) = dir {
        applog::add_crash(&dir, &now_stamp(), &private.scrub(text));
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let what = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_else(|| "(no message)".into());
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        crash(&applog::panic_line(&thread, &place, &what));
        previous(info);
    }));
}

/// A message from Qt or QML (installed in main through qtglue.cpp). kind: 0 debug, 1 warning, 2 critical, 3 fatal,
/// 4 info.
pub extern "C" fn qt_message(kind: i32, place: *const std::os::raw::c_char, text: *const std::os::raw::c_char) {
    let read = |p: *const std::os::raw::c_char| {
        if p.is_null() {
            String::new()
        } else {
            // SAFETY: qtglue.cpp passes NUL-terminated UTF-8 that lives for the whole call.
            unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    };
    match applog::qt_line(kind, &read(place), &read(text)) {
        QtLine::Skip => {}
        // Leaving Game Mode takes the display away and Qt ends the run right after this message. It isn't a
        // crash, and the next start says so.
        QtLine::DisplayGone => {
            line("app", "The display went away (Game Mode ended or Steam restarted): Turbine Hub closes with it");
            if let Some(dir) = dir() {
                applog::display_gone(&dir);
            }
        }
        QtLine::Fatal(msg) => crash(&msg),
        QtLine::Line(msg) => line("qt", &msg),
    }
}

// ---------------------------------------------------------------------------------------------------- local time

#[repr(C)]
struct Tm {
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
    gmtoff: i64,
    zone: *const std::os::raw::c_char,
}

unsafe extern "C" {
    fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
}

/// The local date and time, to the millisecond: 2026-11-06 14:03:22.120.
fn now_stamp() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let mut tm = Tm { sec: 0, min: 0, hour: 0, mday: 1, mon: 0, year: 70, wday: 0, yday: 0, isdst: 0, gmtoff: 0, zone: std::ptr::null() };
    // SAFETY: both pointers are valid for the call; glibc's struct tm on 64-bit Linux matches Tm.
    if unsafe { localtime_r(&secs, &mut tm) }.is_null() {
        return format!("{secs}");
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}", tm.year + 1900, tm.mon + 1, tm.mday, tm.hour, tm.min, tm.sec, d.subsec_millis())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn the_stamp_is_a_local_date_and_time() {
        let s = now_stamp();
        assert!(s.len() == 23 && s.as_bytes()[4] == b'-' && s.as_bytes()[10] == b' ' && s.as_bytes()[19] == b'.', "{s}");
    }

    // One test for the whole life of the log, because the log is one per process.
    #[test]
    fn a_run_is_logged_with_nothing_private_and_a_crash_is_told_from_game_mode_ending() {
        // The pictures, the background helper and --json never start it: every call is a no-op for them.
        line("test", "not kept");
        crash("not kept either");
        assert!(dir().is_none());

        let logs = std::env::temp_dir().join(format!("turbinehub-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&logs);
        // The run before this one never closed.
        assert_eq!(applog::begin(&logs), LastRun::Closed);
        start(logs.clone());
        let home = turbinehub_engine::home_dir();
        line("step", &format!("Host flatpak list in {}/Games: done", home.display()));
        line("update", "curl with token=abc123secret to 140.82.121.6 failed\nsecond line");
        let read = |name: &str| std::fs::read_to_string(logs.join(name)).unwrap_or_default();
        let text = read(applog::FILE);
        assert!(text.contains("crash   The last run ended without closing"), "{text}");
        assert!(text.contains("step    Host flatpak list in ~/Games: done"), "{text}");
        assert!(text.contains("token=(hidden) to (hidden) failed second line"), "{text}");
        assert!(!text.contains("abc123secret") && !text.contains("140.82.121.6") && !text.contains("not kept"));
        if home.to_string_lossy().len() > 1 {
            assert!(!text.contains(&home.to_string_lossy().into_owned()));
        }
        assert!(read(applog::CRASH).contains("The last run ended without closing"));

        // A panic on any thread lands in the crash log, with where it happened.
        let _ = std::thread::Builder::new().name("worker".into()).spawn(|| panic!("the test's own panic")).unwrap().join();
        let crashes = read(applog::CRASH);
        assert!(crashes.contains("panic in thread worker at app/src/log.rs:") && crashes.contains("the test's own panic"), "{crashes}");

        // Qt's messages: a QML warning is a line, a fatal error a crash, debug chatter nothing.
        let say = |kind: i32, place: &str, text: &str| {
            let (p, t) = (CString::new(place).unwrap(), CString::new(text).unwrap());
            qt_message(kind, p.as_ptr(), t.as_ptr());
        };
        say(1, "qrc:/qt/qml/TurbineHub/qml/Main.qml:12", "TypeError: nothing there");
        say(0, "qt.qpa.xcb", "chatter");
        say(3, "", "Could not load the platform plugin");
        qt_message(1, std::ptr::null(), std::ptr::null());
        let text = read(applog::FILE);
        assert!(text.contains("qt      warning: TypeError: nothing there (qrc:/qt/qml/TurbineHub/qml/Main.qml:12)") && !text.contains("chatter"), "{text}");
        assert!(read(applog::CRASH).contains("Qt fatal: Could not load the platform plugin"));

        // Game Mode ending takes the display away: not a crash, and the next start says so.
        say(1, "", "The X11 connection broke (error 1). Did the X11 server die?");
        assert!(read(applog::FILE).contains("The display went away"));
        let crashes = read(applog::CRASH);
        assert_eq!(applog::begin(&logs), LastRun::DisplayGone);
        assert_eq!(read(applog::CRASH), crashes);
        // A run that closes as it should leaves nothing for the next start to report.
        clean_exit();
        assert!(read(applog::FILE).trim_end().ends_with("app     Closed"));
        assert_eq!(applog::begin(&logs), LastRun::Closed);
        let _ = std::fs::remove_dir_all(&logs);
    }
}
