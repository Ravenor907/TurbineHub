//! Network access. Every download and API call goes through `curl` from the Flatpak runtime, run with fixed
//! arguments (never a shell), HTTPS only, with a size cap and timeouts, into a temp file that is checked before use.
//! Parsing lives in the source modules as pure functions so it can be tested offline. This is the only module that
//! starts curl, so it can be swapped later.

use crate::sha256;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Where an installed mod came from. Shared with the store, which records it next to each mod.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceInfo {
    /// "thunderstore", "nexus", "zip" or "loader".
    pub kind: String,
    pub id: String,
    pub version: Option<String>,
    pub url: Option<String>,
    pub sha256: Option<String>,
    /// Dependency ids.
    pub needs: Vec<String>,
    /// The mod's own title where it came from (Nexus's "Seamless Co-op"); the mod is named by it.
    pub title: Option<String>,
    /// Its picture there, for the list and the panel.
    pub picture: Option<String>,
}

/// Why a request failed, worded for the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// No connection, or the server's name couldn't be looked up.
    Offline,
    NotFound,
    /// The server refused the request (401 or 403).
    Denied(u16),
    /// Too many requests; try again later.
    RateLimited,
    /// GitHub is limiting or refusing requests from this connection (a 429, or a 401/403 from one of its
    /// hosts); never the player's account or key. The minutes until it says to ask again.
    GitHubLimit(u32),
    TooBig { limit: u64 },
    Timeout,
    /// The address isn't HTTPS or its host isn't on Turbine Hub's list of allowed hosts.
    NotAllowed(String),
    /// The downloaded file isn't what the source said it would be.
    Mismatch(String),
    /// Some other HTTP status.
    Http(u16),
    /// The player stopped it.
    Cancelled,
    /// The site isn't answering or a protection page stood in for the answer (5xx, or a 401/403
    /// that is a web page with no answer from the site's API). Never the key's or the account's fault.
    Down,
    /// Anything else (curl missing, disk full, broken connection), in words.
    Failed(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Offline => write!(f, "No internet connection. Check Wi-Fi and try again."),
            NetError::NotFound => write!(f, "The server says that file or page doesn't exist (404)."),
            NetError::Denied(c) => write!(f, "The server refused the request ({c}). Try again later."),
            NetError::GitHubLimit(m) => write!(f, "GitHub is limiting requests right now. Try again in about {}.", minutes_words(*m)),
            NetError::RateLimited => write!(f, "The server says too many requests were made. Try again in a few minutes."),
            NetError::TooBig { limit } => {
                write!(f, "The download was bigger than the {} limit, so it was stopped.", human_size(*limit))
            }
            NetError::Timeout => write!(f, "The server took too long to answer. Try again later."),
            NetError::NotAllowed(u) => write!(f, "Turbine Hub only downloads over HTTPS from known sites, and {u} isn't one."),
            NetError::Mismatch(why) => write!(f, "The download didn't match what the site published ({why}), so it was deleted."),
            NetError::Http(c) => write!(f, "The server answered with an error ({c}). Try again later."),
            NetError::Cancelled => write!(f, "The download was stopped."),
            NetError::Down => write!(f, "The site isn't answering right now. Try again in a few minutes."),
            NetError::Failed(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for NetError {}

/// "12.3 MB" style sizes for messages.
pub fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", b / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Hosts Turbine Hub may talk to (plan: downloads only from an allowed list). A leading dot allows every subdomain of a
/// site's own domain (Thunderstore's CDNs, Nexus's API and its download servers such as cf-files.nexusmods.com or
/// supporter-files.nexus-cdn.com, GitHub's API and codeload). A leading dot is never put on a domain where anyone
/// can have a subdomain (githubusercontent.com and github.io are such domains), so those hosts are listed one by
/// one: raw files (the LOOT masterlist) and the two hosts GitHub sends release downloads to.
pub const ALLOWED_HOSTS: &[&str] = &[
    "thunderstore.io",
    ".thunderstore.io",
    ".nexusmods.com",
    ".nexus-cdn.com",
    "skse.silverlock.org",
    "github.com",
    ".github.com",
    "raw.githubusercontent.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    // Microsoft's Visual C++ runtime installer (its short link and its download server), for Cyberpunk's
    // frameworks under Proton (vcrun.rs).
    "aka.ms",
    "download.visualstudio.microsoft.com",
];

/// Domains where anyone can get a subdomain of their own, so a leading-dot entry for them would let in sites run by
/// anyone. `ALLOWED_HOSTS` may name single hosts under them but never the whole domain (checked by the tests).
pub const SHARED_DOMAINS: &[&str] = &["githubusercontent.com", "github.io", "githubpreview.dev", "pages.dev", "workers.dev", "netlify.app", "vercel.app", "web.app", "firebaseapp.com", "appspot.com", "herokuapp.com", "cloudfront.net", "amazonaws.com", "blogspot.com", "r2.dev", "b-cdn.net"];

/// The host part of an https URL, lowercased, without port or user info. None if it isn't https.
pub fn https_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| {
        url.get(..8).filter(|p| p.eq_ignore_ascii_case("https://")).map(|_| &url[8..])
    })?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("");
    if host.is_empty() || !host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

/// May Turbine Hub fetch this URL? HTTPS only, and only from `ALLOWED_HOSTS`.
pub fn allowed_url(url: &str) -> bool {
    let Some(host) = https_host(url) else { return false };
    ALLOWED_HOSTS.iter().any(|a| match a.strip_prefix('.') {
        Some(base) => host.ends_with(a) && host.len() > base.len() + 1,
        None => host == *a,
    })
}

/// Percent-encodes one URL path segment or query value.
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// What to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    None,
    /// The contents of a file, sent as-is with this Content-Type.
    File(PathBuf, String),
}

/// One request. Build it with `Request::get` and the helpers, then hand it to `Client`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Body,
    /// Most bytes the answer may have.
    pub max_bytes: u64,
    /// Seconds to connect.
    pub connect_timeout: u32,
    /// Seconds the whole transfer may take (0 means no limit, for big downloads; a stall still ends it).
    pub max_time: u32,
}

impl Request {
    pub fn get(url: &str) -> Request {
        Request {
            url: url.to_string(),
            headers: Vec::new(),
            body: Body::None,
            max_bytes: 64 << 20,
            connect_timeout: 15,
            max_time: 60,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Request {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn max_bytes(mut self, n: u64) -> Request {
        self.max_bytes = n;
        self
    }

    pub fn max_time(mut self, secs: u32) -> Request {
        self.max_time = secs;
        self
    }

    pub fn post_file(mut self, path: &Path, content_type: &str) -> Request {
        self.body = Body::File(path.to_path_buf(), content_type.to_string());
        self
    }
}

/// The answer to a request: status, headers and where the body was saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    /// Headers of the final answer (after redirects), names lowercased.
    pub headers: Vec<(String, String)>,
    pub effective_url: String,
    pub body: PathBuf,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers.iter().rev().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }

    /// The body as text (and the temp file removed).
    pub fn take_text(self) -> Result<String, NetError> {
        let bytes = fs::read(&self.body).map_err(|e| NetError::Failed(format!("Couldn't read the answer ({e}).")));
        let _ = fs::remove_file(&self.body);
        String::from_utf8(bytes?).map_err(|_| NetError::Failed("The server's answer isn't valid text.".into()))
    }
}

/// The arguments curl gets for a request (pure, so it can be checked in tests). The URL comes last, after "--", so
/// nothing in it can be read as an option.
pub fn curl_args(req: &Request, user_agent: &str, out: &Path, header_file: &Path) -> Vec<String> {
    let mut a: Vec<String> = [
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-redirs",
        "8",
        "--compressed",
        "--globoff",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push("--max-filesize".into());
    a.push(req.max_bytes.to_string());
    a.push("--connect-timeout".into());
    a.push(req.connect_timeout.to_string());
    if req.max_time > 0 {
        a.push("--max-time".into());
        a.push(req.max_time.to_string());
    }
    // A stalled transfer (under 1 KB/s for 60 seconds) ends even without a total time limit.
    a.extend(["--speed-limit", "1024", "--speed-time", "60"].map(String::from));
    a.push("--user-agent".into());
    a.push(user_agent.to_string());
    for (k, v) in &req.headers {
        a.push("--header".into());
        a.push(format!("{k}: {v}"));
    }
    if let Body::File(path, ctype) = &req.body {
        a.push("--header".into());
        a.push(format!("Content-Type: {ctype}"));
        a.push("--data-binary".into());
        a.push(format!("@{}", path.display()));
    }
    a.push("--dump-header".into());
    a.push(header_file.display().to_string());
    a.push("--output".into());
    a.push(out.display().to_string());
    a.push("--write-out".into());
    a.push("%{http_code} %{url_effective}".into());
    a.push("--".into());
    a.push(req.url.clone());
    a
}

/// Reads the headers curl saved: only the last answer's block counts (earlier ones are redirects).
pub fn parse_headers(text: &str) -> (u16, Vec<(String, String)>) {
    let mut status = 0;
    let mut headers = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("HTTP/") {
            status = line.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
            headers.clear();
        } else if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    (status, headers)
}

/// Is this refusal really the site being down or blocked, not the key? A 5xx, or a 401/403 whose
/// answer is a web page (a protection page: its cf-mitigated or cf-ray header, or HTML) instead of the API's JSON.
pub fn site_down(e: &NetError, headers: &[(String, String)], body_start: &[u8]) -> bool {
    let h = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.to_ascii_lowercase());
    let html = h("content-type").is_some_and(|t| t.contains("text/html"))
        || String::from_utf8_lossy(body_start).trim_start().to_ascii_lowercase().starts_with("<!doctype html")
        || String::from_utf8_lossy(body_start).trim_start().to_ascii_lowercase().starts_with("<html");
    match e {
        NetError::Http(s) if (500..=599).contains(s) => true,
        NetError::Denied(_) => h("cf-mitigated").is_some() || (html && (h("cf-ray").is_some() || h("server").is_some_and(|v| v.contains("cloudflare")))),
        _ => false,
    }
}

/// "1 minute", "12 minutes".
pub fn minutes_words(m: u32) -> String {
    if m == 1 {
        "1 minute".into()
    } else {
        format!("{m} minutes")
    }
}

/// Is this a limit or refusal from GitHub (a 403 at Check for updates: 60 requests an hour for a
/// connection that isn't signed in)? The minutes to wait: from Retry-After (seconds), else X-RateLimit-Reset (the
/// time it lifts, seconds since 1970) against `now`, else 15. At least 1, at most 60. None for other sites and
/// other failures.
pub fn github_limit(e: &NetError, url: &str, headers: &[(String, String)], now: u64) -> Option<u32> {
    if !matches!(e, NetError::Denied(_) | NetError::RateLimited) {
        return None;
    }
    let host = https_host(url)?;
    if host != "github.com" && !host.ends_with(".github.com") {
        return None;
    }
    let h = |k: &str| headers.iter().rev().find(|(n, _)| n == k).and_then(|(_, v)| v.trim().parse::<u64>().ok());
    let secs = h("retry-after").or_else(|| h("x-ratelimit-reset").map(|at| at.saturating_sub(now))).unwrap_or(15 * 60);
    Some(secs.div_ceil(60).clamp(1, 60) as u32)
}

/// Turns curl's exit code and the HTTP status into a result (pure).
pub fn classify(exit: i32, status: u16, max_bytes: u64) -> Result<(), NetError> {
    match exit {
        0 => match status {
            200..=299 => Ok(()),
            404 | 410 => Err(NetError::NotFound),
            401 | 403 => Err(NetError::Denied(status)),
            429 => Err(NetError::RateLimited),
            _ => Err(NetError::Http(status)),
        },
        22 => match status {
            404 | 410 => Err(NetError::NotFound),
            401 | 403 => Err(NetError::Denied(status)),
            429 => Err(NetError::RateLimited),
            s => Err(NetError::Http(s)),
        },
        5..=7 => Err(NetError::Offline),
        28 => Err(NetError::Timeout),
        63 => Err(NetError::TooBig { limit: max_bytes }),
        1 => Err(NetError::Failed("The address isn't HTTPS, so Turbine Hub didn't open it.".into())),
        23 => Err(NetError::Failed("Couldn't save the download. Is the disk full?".into())),
        35 | 51 | 53 | 54 | 58 | 59 | 60 | 77 | 80 | 82 | 83 | 90 | 91 => {
            Err(NetError::Failed("The site's security certificate couldn't be checked, so nothing was downloaded.".into()))
        }
        47 => Err(NetError::Failed("The site redirected too many times.".into())),
        18 | 52 | 55 | 56 | 92 | 16 => Err(NetError::Failed("The connection broke during the transfer. Try again.".into())),
        c => Err(NetError::Failed(format!("The transfer failed (curl error {c})."))),
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A new, unused file name in `dir`.
pub fn temp_path(dir: &Path, tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let clean: String = tag.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    dir.join(format!("turbinehub-{}-{t:x}-{n}-{clean}", std::process::id()))
}

/// What the app shows while a download runs: bytes so far and, when known, the total.
pub type Progress<'a> = &'a mut dyn FnMut(u64, Option<u64>) -> bool;

/// Runs curl. Holds the program name (so tests can put a stand-in in its place), the User-Agent and the temp folder.
#[derive(Clone, Debug)]
pub struct Client {
    pub program: PathBuf,
    pub user_agent: String,
    pub temp_dir: PathBuf,
}

impl Client {
    /// `version` is Turbine Hub's own one-dot version, sent in the User-Agent.
    pub fn new(version: &str, temp_dir: &Path) -> Client {
        Client {
            program: PathBuf::from("curl"),
            user_agent: format!("TurbineHub/{version}"),
            temp_dir: temp_dir.to_path_buf(),
        }
    }

    /// Sends a request and saves the answer to a temp file. `progress` is called a few times a second with the bytes
    /// received so far; returning false stops the transfer.
    pub fn send(&self, req: &Request, mut progress: Option<Progress>) -> Result<Response, NetError> {
        if !allowed_url(&req.url) {
            return Err(NetError::NotAllowed(https_host(&req.url).unwrap_or_else(|| req.url.clone())));
        }
        fs::create_dir_all(&self.temp_dir)
            .map_err(|e| NetError::Failed(format!("Couldn't make the download folder ({e}).")))?;
        let out = temp_path(&self.temp_dir, "body");
        let hdr = temp_path(&self.temp_dir, "headers");
        let args = curl_args(req, &self.user_agent, &out, &hdr);
        let cleanup = |also_body: bool| {
            let _ = fs::remove_file(&hdr);
            if also_body {
                let _ = fs::remove_file(&out);
            }
        };
        // "Text file busy" (ETXTBSY) can happen for a moment when the program file was just written while another
        // thread was starting a process; a short retry gets past it.
        let mut spawned = Command::new(&self.program).args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
        for _ in 0..20 {
            match &spawned {
                Err(e) if e.raw_os_error() == Some(26) => {
                    std::thread::sleep(Duration::from_millis(25));
                    spawned = Command::new(&self.program).args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
                }
                _ => break,
            }
        }
        let mut child = match spawned
        {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(NetError::Failed("Couldn't find curl, which Turbine Hub uses to download.".into()))
            }
            Err(e) => return Err(NetError::Failed(format!("Couldn't start the download ({e})."))),
        };
        let mut total: Option<u64> = None;
        let started = Instant::now();
        let exit = loop {
            match child.try_wait() {
                Ok(Some(st)) => break st.code().unwrap_or(-1),
                Ok(None) => {}
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    cleanup(true);
                    return Err(NetError::Failed(format!("The download stopped unexpectedly ({e}).")));
                }
            }
            let got = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            if got > req.max_bytes {
                let _ = child.kill();
                let _ = child.wait();
                cleanup(true);
                return Err(NetError::TooBig { limit: req.max_bytes });
            }
            if total.is_none() && let Ok(h) = fs::read_to_string(&hdr) {
                let (st, headers) = parse_headers(&h);
                if (200..300).contains(&st) {
                    total = headers.iter().find(|(k, _)| k == "content-length").and_then(|(_, v)| v.parse().ok());
                }
            }
            if let Some(p) = progress.as_mut() && !p(got, total) {
                let _ = child.kill();
                let _ = child.wait();
                cleanup(true);
                return Err(NetError::Cancelled);
            }
            // Short sleeps at first so quick API calls aren't slowed down.
            let pause = if started.elapsed() < Duration::from_millis(300) { 10 } else { 150 };
            std::thread::sleep(Duration::from_millis(pause));
        };
        let mut stdout = String::new();
        if let Some(mut s) = child.stdout.take() {
            let _ = io::Read::read_to_string(&mut s, &mut stdout);
        }
        let mut w = stdout.trim().splitn(2, ' ');
        let status: u16 = w.next().and_then(|c| c.parse().ok()).unwrap_or(0);
        let effective_url = w.next().unwrap_or("").to_string();
        let (hstatus, headers) = parse_headers(&fs::read_to_string(&hdr).unwrap_or_default());
        let status = if status == 0 { hstatus } else { status };
        cleanup(false);
        if let Err(e) = classify(exit, status, req.max_bytes) {
            let page = fs::read(&out).map(|b| b.into_iter().take(512).collect::<Vec<u8>>()).unwrap_or_default();
            let _ = fs::remove_file(&out);
            if site_down(&e, &headers, &page) {
                return Err(NetError::Down);
            }
            let now = crate::now();
            return Err(match github_limit(&e, &req.url, &headers, now) {
                Some(m) => NetError::GitHubLimit(m),
                None => e,
            });
        }
        if !effective_url.is_empty() && !allowed_url(&effective_url) {
            let _ = fs::remove_file(&out);
            return Err(NetError::NotAllowed(https_host(&effective_url).unwrap_or(effective_url)));
        }
        let len = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        if len > req.max_bytes {
            let _ = fs::remove_file(&out);
            return Err(NetError::TooBig { limit: req.max_bytes });
        }
        if !out.exists() {
            // An empty answer: curl doesn't create the file.
            fs::write(&out, b"").map_err(|e| NetError::Failed(format!("Couldn't save the answer ({e}).")))?;
        }
        if let Some(p) = progress.as_mut() {
            // A stop asked for at the very end counts too. A transfer that finished before it was first looked at
            // was never asked whether to go on, so a download the player had stopped could still come back whole
            // (and the check of exactly that failed now and then on a fast machine).
            if !p(len, Some(len)) {
                let _ = fs::remove_file(&out);
                return Err(NetError::Cancelled);
            }
        }
        Ok(Response { status, headers, effective_url, body: out })
    }

    /// GETs a small JSON answer and returns its text.
    pub fn get_text(&self, url: &str, headers: &[(&str, &str)]) -> Result<String, NetError> {
        let mut req = Request::get(url).max_bytes(32 << 20).max_time(60);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        self.send(&req, None)?.take_text()
    }

    /// Downloads a file to a temp path and checks it against what the source published: its size when known and
    /// its SHA-256 when published. The file is deleted if anything doesn't match.
    pub fn download(&self, url: &str, check: &Expect, progress: Option<Progress>) -> Result<Download, NetError> {
        let req = Request::get(url).max_bytes(check.max_bytes).max_time(0);
        let resp = self.send(&req, progress)?;
        verify_file(&resp.body, check).inspect_err(|_| {
            let _ = fs::remove_file(&resp.body);
        })
    }
}

/// What a download must look like.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expect {
    pub max_bytes: u64,
    pub size: Option<u64>,
    pub sha256: Option<String>,
}

impl Expect {
    pub fn up_to(max_bytes: u64) -> Expect {
        Expect { max_bytes, size: None, sha256: None }
    }
}

/// A finished, checked download in a temp file. The caller moves or deletes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
}

/// Checks a file against `Expect` (size and SHA-256). Pure apart from reading the file.
pub fn verify_file(path: &Path, check: &Expect) -> Result<Download, NetError> {
    let size = fs::metadata(path).map_err(|e| NetError::Failed(format!("Couldn't read the download ({e}).")))?.len();
    if size > check.max_bytes {
        return Err(NetError::TooBig { limit: check.max_bytes });
    }
    if let Some(want) = check.size && want != size {
        return Err(NetError::Mismatch(format!("size {} instead of {}", human_size(size), human_size(want))));
    }
    let got = sha256::file_hex(path).map_err(|e| NetError::Failed(format!("Couldn't read the download ({e}).")))?;
    if let Some(want) = &check.sha256 && !sha256::same_digest(want, &got) {
        return Err(NetError::Mismatch("its SHA-256 is different".into()));
    }
    Ok(Download { path: path.to_path_buf(), size, sha256: got })
}
