//! The newest GE-Proton and what players say on ProtonDB, fetched through the engine's own client from the build's
//! stand-in server (tools/stand-in.py), which serves a small made-up release: asked for, downloaded, checked
//! against its SHA-512, unpacked into a made-up Steam folder and removed again. Nothing here leaves the machine.
//! Where there is no python3 the tests have nothing to check.

mod support;

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use support::scratch;
use turbinehub_engine::net::Client;
use turbinehub_engine::{compat_tool, proton_get, protondb};

/// A stand-in for curl that asks the stand-in server instead of the real sites (the same path, on 127.0.0.1) and
/// answers the way curl does: the body, the headers, and "<status> <where it ended up>" with the real site's
/// name. Three marker files change what it does: `no-api` makes GitHub's API refuse (its limit), `tamper` adds a
/// byte to every .tar.gz, and `slow` writes the first half of a .tar.gz and then hangs, as a download under way.
const CURL: &str = r#"#!/usr/bin/env python3
import os, sys, urllib.error, urllib.parse, urllib.request
BASE, MARKS = "@BASE@", "@MARKS@"
args, out, hdr, url = sys.argv[1:], None, None, None
i = 0
while i < len(args):
    if args[i] == "--output": out = args[i + 1]
    if args[i] == "--dump-header": hdr = args[i + 1]
    if args[i] == "--": url = args[i + 1]
    i += 1
parts = urllib.parse.urlsplit(url)
with open(os.path.join(MARKS, "requests.log"), "a") as f:
    f.write(url + "\n")
def fail(code):
    open(hdr, "w").write("HTTP/1.1 %d\r\n\r\n" % code)
    sys.stdout.write("%d %s" % (code, url))
    sys.exit(22)
if parts.netloc == "api.github.com" and os.path.exists(os.path.join(MARKS, "no-api")):
    fail(403)
try:
    with urllib.request.urlopen(BASE + parts.path + ("?" + parts.query if parts.query else ""), timeout=20) as r:
        body, ended = r.read(), urllib.parse.urlsplit(r.geturl()).path
except urllib.error.HTTPError as e:
    fail(e.code)
if parts.path.endswith(".tar.gz") and os.path.exists(os.path.join(MARKS, "tamper")):
    body += b"x"
if parts.path.endswith(".tar.gz") and os.path.exists(os.path.join(MARKS, "slow")):
    import time
    open(hdr, "w").write("HTTP/1.1 200\r\ncontent-length: %d\r\n\r\n" % len(body))
    with open(out, "wb") as f:
        f.write(body[:len(body) // 2])
    time.sleep(60)
open(out, "wb").write(body)
open(hdr, "w").write("HTTP/1.1 200\r\ncontent-length: %d\r\n\r\n" % len(body))
sys.stdout.write("200 https://%s%s" % (parts.netloc, ended))
"#;

struct StandIn {
    server: Child,
    dir: PathBuf,
    client: Client,
}

impl Drop for StandIn {
    fn drop(&mut self) {
        let _ = self.server.kill();
        let _ = self.server.wait();
    }
}

impl StandIn {
    /// Starts the stand-in server on a free port of its own choosing (its first line says which). None where
    /// python3 isn't there.
    fn start(tag: &str) -> Option<StandIn> {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/stand-in.py");
        let mut server = Command::new("python3").arg(script).args(["--port", "0"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
        let mut line = String::new();
        BufReader::new(server.stdout.take()?).read_line(&mut line).ok()?;
        let Some(base) = line.trim().strip_prefix("stand-in on ").map(String::from) else {
            let _ = server.kill();
            let _ = server.wait();
            panic!("the stand-in server didn't say where it listens: {line:?}");
        };
        let dir = scratch(tag);
        let marks = dir.join("marks");
        fs::create_dir_all(&marks).unwrap();
        let curl = dir.join("curl");
        fs::write(&curl, CURL.replace("@BASE@", &base).replace("@MARKS@", &marks.display().to_string())).unwrap();
        fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
        let mut client = Client::new("0.1", &dir.join("cache/tmp"));
        client.program = curl;
        Some(StandIn { server, dir, client })
    }
    fn mark(&self, name: &str, on: bool) {
        let f = self.dir.join("marks").join(name);
        if on { fs::write(f, "").unwrap() } else { let _ = fs::remove_file(f); }
    }
    fn asked(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("marks/requests.log")).unwrap_or_default().lines().map(String::from).collect()
    }
    fn cache(&self) -> PathBuf {
        self.dir.join("cache")
    }
    /// A made-up Steam folder.
    fn steam(&self) -> PathBuf {
        let root = self.dir.join("home/.local/share/Steam");
        fs::create_dir_all(root.join("config")).unwrap();
        root
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    v.sort();
    v
}

/// The downloads in the cache: whole ones and ones still coming in.
fn downloads(cache: &Path) -> Vec<String> {
    names(cache).into_iter().filter(|n| n.ends_with(".part") || n.ends_with(".tar.gz")).collect()
}

const TAG: &str = "GE-Proton11-99";

/// The build of the made-up release for this machine: "GE-Proton11-99-x86_64".
fn build_name() -> String {
    format!("{TAG}-{}", proton_get::arch())
}

#[test]
fn the_newest_ge_proton_is_found_downloaded_checked_unpacked_and_removed() {
    let Some(s) = StandIn::start("ge") else { return };
    let (root, folder, cache) = (s.steam(), s.dir.join("turbinehub"), s.cache());
    let now = turbinehub_engine::now();
    let name = build_name();
    assert_eq!(proton_get::known(&cache), None);
    let newest = proton_get::newest(&s.client, &cache, now).unwrap();
    assert_eq!((newest.tag.as_str(), newest.name.as_str()), (TAG, name.as_str()));
    assert!(newest.archive_size.is_some_and(|n| n > 500) && newest.archive_url.ends_with(&format!("/{TAG}/{name}.tar.gz")));
    // The answer is kept: the page knows the newest one without asking, and a second question isn't sent.
    assert_eq!(proton_get::known(&cache).unwrap(), newest);
    assert_eq!((proton_get::newest(&s.client, &cache, now).unwrap(), s.asked().len()), (newest, 1));
    assert!(!proton_get::installed(&root, TAG));

    let said = std::cell::RefCell::new(Vec::new());
    let got = proton_get::install(&s.client, &cache, &root, &folder, "", now, &|line, _| {
        if !line.is_empty() {
            said.borrow_mut().push(line.to_string());
        }
        true
    })
    .unwrap();
    assert_eq!(got, name);
    assert_eq!(*said.borrow(), ["Asking GitHub for the newest GE-Proton".to_string(), format!("Downloading {TAG}"), format!("Checking {TAG}"), format!("Unpacking {TAG}")]);
    // It is a Proton Turbine Hub's list shows, with the name Steam will call it (its tool file has a comment
    // beside that name, as the real one does), its program can be run, its links and its long-named file are there.
    assert_eq!(proton_get::installed_as(&root, TAG), Some(name.clone()));
    let list = compat_tool::installed_protons(&root);
    assert_eq!(list.iter().map(|p| (p.name.as_str(), p.steam_name.as_deref(), p.usable)).collect::<Vec<_>>(), vec![(name.as_str(), Some(name.as_str()), true)]);
    let build = proton_get::build_dir(&root, &name);
    assert_eq!(fs::metadata(build.join("proton")).unwrap().permissions().mode() & 0o111, 0o111);
    assert!(fs::symlink_metadata(build.join("files/lib64")).unwrap().file_type().is_symlink());
    assert_eq!(fs::read(build.join("files/lib64/wine/x86_64-unix/ntdll.so")).unwrap().len(), 17 * 400);
    assert_eq!(fs::read(build.join("files/share/default_pfx/drive_c/windows/system32/ntdll.dll")).unwrap().len(), 17 * 400);
    assert_eq!(fs::read_to_string(build.join(format!("files/share/{}readme.txt", "deep/".repeat(24)))).unwrap(), "found by its long name\n");
    // Nothing is left over: no scratch folder beside it, no download in the cache.
    assert_eq!(names(&compat_tool::tools_dir(&root)), vec![name.clone()]);
    assert_eq!(names(&cache.join("tmp")), Vec::<String>::new());
    assert_eq!(downloads(&cache), Vec::<String>::new());
    assert_eq!(proton_get::fetched(&root, &folder), vec![name.clone()]);
    // A second press downloads nothing.
    let before = s.asked().len();
    assert_eq!(proton_get::install(&s.client, &cache, &root, &folder, TAG, now, &|_, _| true).unwrap(), name);
    assert_eq!(s.asked().len(), before);

    // Remove: refused while a pick names it, then it goes, and only it.
    compat_tool::set_proton_pick(&folder, "1086940", Some(&build)).unwrap();
    assert!(proton_get::remove(&root, &folder, &name).unwrap_err().starts_with("1 game is set to use it"));
    compat_tool::set_proton_pick(&folder, "1086940", None).unwrap();
    proton_get::remove(&root, &folder, &name).unwrap();
    assert!(!build.exists() && proton_get::fetched(&root, &folder).is_empty());
}

#[test]
fn a_download_that_does_not_match_its_checksum_installs_nothing() {
    let Some(s) = StandIn::start("tamper") else { return };
    let (root, folder, cache) = (s.steam(), s.dir.join("turbinehub"), s.cache());
    s.mark("tamper", true);
    let why = proton_get::install(&s.client, &cache, &root, &folder, TAG, 0, &|_, _| true).unwrap_err();
    assert!(why.contains("didn't match what the site published") && why.contains("SHA-512"), "{why}");
    assert!(!proton_get::installed(&root, TAG) && names(&compat_tool::tools_dir(&root)).is_empty());
    assert_eq!(names(&cache.join("tmp")), Vec::<String>::new());
    assert!(proton_get::fetched(&root, &folder).is_empty());
    // A release that isn't there, and a name that isn't a release's.
    assert!(proton_get::install(&s.client, &cache, &root, &folder, "GE-Proton1-1", 0, &|_, _| true).unwrap_err().starts_with("Couldn't get the checksum published with GE-Proton1-1"));
    assert_eq!(proton_get::install(&s.client, &cache, &root, &folder, "../../x", 0, &|_, _| true).unwrap_err(), "That isn't a GE-Proton release.");
    // The same download, untouched, goes in.
    s.mark("tamper", false);
    assert_eq!(proton_get::install(&s.client, &cache, &root, &folder, TAG, 0, &|_, _| true).unwrap(), build_name());
    assert!(proton_get::installed(&root, TAG));
}

/// A fetch that was stopped: no build and no half build in Steam's folder, no download in the cache, and no word
/// of it in the list of fetched builds.
fn nothing_left(s: &StandIn, at: &str) {
    let (root, folder, cache) = (s.steam(), s.dir.join("turbinehub"), s.cache());
    assert_eq!(names(&compat_tool::tools_dir(&root)), Vec::<String>::new(), "{at}");
    assert_eq!((downloads(&cache), names(&cache.join("tmp"))), (Vec::<String>::new(), Vec::<String>::new()), "{at}");
    assert!(!proton_get::installed(&root, TAG) && proton_get::fetched(&root, &folder).is_empty() && !proton_get::record_file(&folder).exists(), "{at}");
}

#[test]
fn a_fetch_stopped_anywhere_leaves_nothing_behind() {
    let Some(s) = StandIn::start("stop") else { return };
    let (root, folder, cache) = (s.steam(), s.dir.join("turbinehub"), s.cache());
    // In the middle of the download: half of it is in the cache as a .part when Stop comes.
    s.mark("slow", true);
    let part = cache.join(format!("{}.tar.gz.part", build_name()));
    let half = std::cell::Cell::new(false);
    let began = std::time::Instant::now();
    let why = proton_get::install(&s.client, &cache, &root, &folder, TAG, 0, &|_, _| {
        if fs::metadata(&part).is_ok_and(|m| m.len() > 0) {
            half.set(true);
        }
        !half.get()
    })
    .unwrap_err();
    assert_eq!(why, turbinehub_engine::STOPPED);
    assert!(half.get() && began.elapsed() < std::time::Duration::from_secs(30));
    nothing_left(&s, "in the middle of the download");
    s.mark("slow", false);
    // Stopped at every look in turn, from the first to the one right before the build gets its name: the same
    // each time. The stage it was at is noted, so all of them are known to have been stopped in.
    let mut stages: Vec<String> = Vec::new();
    for at in 0.. {
        assert!(at < 2000, "it never got through");
        let (n, stage) = (std::cell::Cell::new(0usize), std::cell::RefCell::new(String::new()));
        let result = proton_get::install(&s.client, &cache, &root, &folder, TAG, 0, &|line, _| {
            n.set(n.get() + 1);
            if !line.is_empty() {
                *stage.borrow_mut() = line.split(' ').next().unwrap_or("").to_string();
            }
            n.get() <= at
        });
        match result {
            Err(why) => {
                assert_eq!(why, turbinehub_engine::STOPPED, "stopped at look {at}");
                nothing_left(&s, &format!("stopped at look {at}"));
                stages.push(stage.into_inner());
            }
            // Never told to stop any more: it went through.
            Ok(name) => {
                assert_eq!((name, n.get() <= at), (build_name(), true));
                break;
            }
        }
    }
    for stage in ["Downloading", "Checking", "Unpacking"] {
        assert!(stages.iter().any(|x| x == stage), "never stopped while {stage}: {stages:?}");
    }
    // The last stop was right before the name was given: everything was unpacked by then.
    assert_eq!(stages.last().map(String::as_str), Some("Unpacking"));
    assert!(proton_get::installed(&root, TAG) && proton_get::fetched(&root, &folder) == [build_name()] && downloads(&cache).is_empty());
    // What a killed run left is gone after the next press, whatever that press fetches.
    proton_get::remove(&root, &folder, &build_name()).unwrap();
    fs::write(&part, "half").unwrap();
    fs::create_dir_all(proton_get::part_dir(&root, &build_name()).join(build_name())).unwrap();
    assert_eq!(proton_get::install(&s.client, &cache, &root, &folder, TAG, 0, &|_, _| true).unwrap(), build_name());
    assert_eq!((names(&compat_tool::tools_dir(&root)), downloads(&cache)), (vec![build_name()], Vec::<String>::new()));
}

#[test]
fn the_releases_page_answers_when_githubs_api_does_not() {
    let Some(s) = StandIn::start("page") else { return };
    let cache = s.cache();
    s.mark("no-api", true);
    let now = turbinehub_engine::now();
    let r = proton_get::newest(&s.client, &cache, now).unwrap();
    assert_eq!((r.tag.as_str(), r.archive_size), (TAG, None));
    assert_eq!(r, proton_get::release_for_tag(TAG).unwrap());
    assert_eq!(s.asked(), [proton_get::API_LATEST, proton_get::LATEST_PAGE]);
    // What the page said is kept too, and the whole install works from it.
    assert_eq!(proton_get::known(&cache).unwrap().tag, TAG);
    let (root, folder) = (s.steam(), s.dir.join("turbinehub"));
    assert_eq!(proton_get::install(&s.client, &cache, &root, &folder, "", now + 7 * 3600, &|_, _| true).unwrap(), build_name());
    assert!(proton_get::installed(&root, TAG));
}

#[test]
fn a_release_from_before_the_per_processor_builds_is_found_under_its_plain_name() {
    let Some(s) = StandIn::start("older") else { return };
    let (root, folder, cache) = (s.steam(), s.dir.join("turbinehub"), s.cache());
    let old = "GE-Proton10-98";
    let got = proton_get::install(&s.client, &cache, &root, &folder, old, 0, &|_, _| true);
    if proton_get::arch() != "x86_64" {
        // Those releases had no build for any other processor.
        assert!(got.unwrap_err().starts_with("Couldn't get the checksum published with GE-Proton10-98"));
        return;
    }
    assert_eq!(got.unwrap(), old);
    // Today's name was asked for first, then the plain one.
    let asked = s.asked();
    assert!(asked[0].ends_with("/GE-Proton10-98/GE-Proton10-98-x86_64.sha512sum") && asked[1].ends_with("/GE-Proton10-98/GE-Proton10-98.sha512sum") && asked[2].ends_with("/GE-Proton10-98/GE-Proton10-98.tar.gz"), "{asked:?}");
    assert_eq!(proton_get::installed_as(&root, old).as_deref(), Some(old));
    assert_eq!((names(&compat_tool::tools_dir(&root)), proton_get::fetched(&root, &folder)), (vec![old.to_string()], vec![old.to_string()]));
    proton_get::remove(&root, &folder, old).unwrap();
    assert!(names(&compat_tool::tools_dir(&root)).is_empty());
}

#[test]
fn what_players_say_comes_from_the_stand_in_too() {
    let Some(s) = StandIn::start("protondb") else { return };
    let cache = s.cache();
    let ids: Vec<String> = ["1245620", "1086940", "3999903", "892970", "1"].iter().map(|x| x.to_string()).collect();
    let fetched = protondb::refresh(&cache, &ids, turbinehub_engine::now(), 10, &mut |u| s.client.get_text(u, &[]).map_err(|e| e.to_string()));
    assert_eq!(fetched, 4);
    use protondb::Tier::*;
    assert_eq!(ids.iter().map(|id| protondb::tier(&cache, id)).collect::<Vec<_>>(), vec![Some(Gold), Some(Platinum), Some(Borked), None, None]);
    // The game without reports (a 404) is remembered as that, so it isn't asked about again this week.
    let before = s.asked().len();
    protondb::refresh(&cache, &ids, turbinehub_engine::now(), 10, &mut |u| s.client.get_text(u, &[]).map_err(|e| e.to_string()));
    assert_eq!(s.asked().len(), before);
    let mut text = String::new();
    fs::File::open(protondb::file(&cache, "1")).unwrap().read_to_string(&mut text).unwrap();
    assert!(protondb::valid(&text));
}
