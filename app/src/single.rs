//! One Turbine Hub window at a time.
//!
//! The first Turbine Hub listens on a Unix socket in its runtime folder (only the player can open it). A second
//! launch finds it answering, asks it to come to the front and ends, so two windows never change the same settings
//! side by side. The only line the socket takes is that one request; anything else is dropped.
//!
//! In the Flatpak the folder is $XDG_RUNTIME_DIR/app/<app id>/, which Flatpak shares between every copy of the same
//! app (the rest of the runtime folder is private to each copy); outside it, $XDG_RUNTIME_DIR/<app id>/.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const APP_ID: &str = "io.github.ravenor907.TurbineHub";
/// What a second launch sends: the running Turbine Hub comes to the front. Started from Turbine Files' own icon
/// (`--page=files`) it sends the second line, and the open window comes forward on Files.
const SHOW: &str = "turbinehub:show";
const SHOW_FILES: &str = "turbinehub:show files";
/// Longest line read from the socket.
const MAX_LINE: u64 = 64;

/// A second launch asked for the window (the interface takes it at its next look), and whether it asked for Files.
static SHOW_ASKED: AtomicBool = AtomicBool::new(false);
static FILES_ASKED: AtomicBool = AtomicBool::new(false);
/// This run made the socket (so it is this run's to remove).
static LISTENING: AtomicBool = AtomicBool::new(false);

/// Whether a second launch asked for the window since the last call: None when not, else the page it asked for
/// ("" for the window as it is, "files" for Files).
pub fn take_show() -> Option<String> {
    let files = FILES_ASKED.swap(false, Ordering::SeqCst);
    SHOW_ASKED.swap(false, Ordering::SeqCst).then(|| if files { "files".to_string() } else { String::new() })
}

/// Where the socket lives. None when there is no runtime folder.
fn socket_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()).map(PathBuf::from)?;
    let dir = if Path::new("/.flatpak-info").exists() { base.join("app").join(APP_ID) } else { base.join(APP_ID) };
    Some(dir.join("turbinehub.sock"))
}

/// Asks the Turbine Hub listening at `path` to come to the front. Ok only when it said it will.
fn ask(path: &Path, files: bool) -> std::io::Result<()> {
    let mut s = UnixStream::connect(path)?;
    s.set_write_timeout(Some(Duration::from_secs(2)))?;
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    s.write_all(if files { SHOW_FILES } else { SHOW }.as_bytes())?;
    s.write_all(b"\n")?;
    s.flush()?;
    let mut ok = [0u8; 2];
    s.read_exact(&mut ok)?;
    if &ok == b"ok" { Ok(()) } else { Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not taken")) }
}

/// One connection: the line it sent, answered "ok" when it is a request to show and "no" otherwise. A request is
/// handed to `asked` with the page it is for ("" for the window as it is) before the answer goes out, so whoever
/// hears "ok" knows it was taken.
fn serve(stream: UnixStream, asked: &dyn Fn(&str)) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut writer = stream.try_clone().ok();
    let mut line = String::new();
    let mut r = BufReader::new(stream.take(MAX_LINE + 1));
    let taken = if r.read_line(&mut line).is_err() {
        None
    } else if line.trim() == SHOW {
        Some("")
    } else if line.trim() == SHOW_FILES {
        Some("files")
    } else {
        None
    };
    if let Some(page) = taken {
        asked(page);
    }
    if let Some(w) = writer.as_mut() {
        let _ = w.write_all(if taken.is_some() { b"ok" } else { b"no" });
    }
}

/// Starts listening at `path` for later launches; every request to show goes to `asked`. False when the socket
/// can't be made.
fn listen(path: &Path, asked: impl Fn(&str) + Send + 'static) -> bool {
    let Some(dir) = path.parent() else { return false };
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    // Nothing answers there, so a socket file left behind is from a Turbine Hub that didn't close: it goes.
    if fs::symlink_metadata(path).is_ok() {
        let _ = fs::remove_file(path);
    }
    let Ok(listener) = UnixListener::bind(path) else { return false };
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    std::thread::Builder::new()
        .name("turbinehub-single".into())
        .spawn(move || {
            for conn in listener.incoming() {
                match conn {
                    Ok(s) => serve(s, &asked),
                    Err(_) => std::thread::sleep(Duration::from_millis(200)),
                }
            }
        })
        .is_ok()
}

/// Called once at the start of a window run. True when another Turbine Hub is open and was asked to come to the
/// front: this launch ends. Otherwise this one is the Turbine Hub that later launches find (when the socket can't be
/// made, or the other one doesn't answer, it simply starts).
pub fn other_is_open() -> bool {
    let Some(path) = socket_path() else { return false };
    if ask(&path, crate::backend::option("page").as_deref() == Some("files")).is_ok() {
        return true;
    }
    let made = listen(&path, |page| {
        if page == "files" {
            FILES_ASKED.store(true, Ordering::SeqCst);
        }
        SHOW_ASKED.store(true, Ordering::SeqCst);
    });
    LISTENING.store(made, Ordering::SeqCst);
    false
}

/// Removes the socket when Turbine Hub closes (only the run that made it).
pub fn stop() {
    if LISTENING.swap(false, Ordering::SeqCst)
        && let Some(p) = socket_path()
    {
        let _ = fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    fn place(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("turbinehub-single-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("turbinehub.sock")
    }

    #[test]
    fn a_second_launch_finds_the_first_and_asks_it_to_show() {
        let path = place("show");
        // Nobody is there yet.
        assert!(ask(&path, false).is_err());
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        assert!(listen(&path, move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        assert!(ask(&path, false).is_ok());
        assert!(ask(&path, false).is_ok());
        // The listener counts on its own thread, a moment after each answer: wait for it instead of racing it.
        let started = std::time::Instant::now();
        while count.load(Ordering::SeqCst) < 2 && started.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(count.load(Ordering::SeqCst), 2);
        // Only the player may open it.
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_launch_from_turbine_files_asks_for_files() {
        let path = place("files");
        let pages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let p = pages.clone();
        assert!(listen(&path, move |page| p.lock().unwrap().push(page.to_string())));
        assert!(ask(&path, false).is_ok());
        assert!(ask(&path, true).is_ok());
        assert_eq!(*pages.lock().unwrap(), vec!["".to_string(), "files".to_string()]);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn any_other_line_is_refused_and_does_nothing() {
        let path = place("other");
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        assert!(listen(&path, move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        for line in ["hello\n", "turbinehub:show and more\n", "turbinehub:show settings\n", "\n"] {
            let mut s = UnixStream::connect(&path).unwrap();
            s.write_all(line.as_bytes()).unwrap();
            let mut answer = [0u8; 2];
            s.read_exact(&mut answer).unwrap();
            assert_eq!(&answer, b"no");
        }
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn after_a_restart_the_new_copy_becomes_the_one_that_listens() {
        let path = place("restart");
        let old = Arc::new(AtomicUsize::new(0));
        let c = old.clone();
        assert!(listen(&path, move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        // What `stop` does before the old copy starts the new one: the socket's name goes, while the old copy
        // (still closing) goes on listening on it.
        fs::remove_file(&path).unwrap();
        // The new copy finds nobody answering, so it doesn't end: it listens itself, and later launches reach it.
        assert!(ask(&path, false).is_err());
        let new = Arc::new(AtomicUsize::new(0));
        let c = new.clone();
        assert!(listen(&path, move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        assert!(ask(&path, false).is_ok());
        assert_eq!((old.load(Ordering::SeqCst), new.load(Ordering::SeqCst)), (0, 1));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_socket_left_by_a_copy_that_did_not_close_is_replaced() {
        let path = place("stale");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A file where the socket goes, with nobody listening.
        drop(UnixListener::bind(&path).unwrap());
        assert!(ask(&path, false).is_err());
        assert!(listen(&path, |_| {}));
        assert!(ask(&path, false).is_ok());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
