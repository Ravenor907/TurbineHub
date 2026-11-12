//! Turbine Hub's own updates from its GitHub releases: find the newest release (pre-releases only when Labs mode opts
//! in; from the releases page and feed, not GitHub's API), compare one-dot versions, download TurbineHub.flatpak and check its size and SHA-256. Installing is
//! done by the app with `install_args`.

use crate::json::{self, Json};
use crate::net::{Client, Download, Expect, NetError, Progress, Request};
use crate::sha256;
use std::cmp::Ordering;
use std::path::Path;

/// The repository's path on GitHub, for the addresses below.
macro_rules! repo {
    () => {
        "Ravenor907/TurbineHub"
    };
}
/// The newest full release's own page. GitHub redirects it to ".../releases/tag/<tag>", so the tag is read
/// from where the request ends up and GitHub's API (60 requests an hour for a connection that isn't signed in,
/// which Check for updates ran into as a 403) isn't asked.
pub const LATEST_PAGE: &str = concat!("https://github.com/", repo!(), "/releases/latest");
/// Every release with its tag, pre-releases included (Labs mode), as the repository's feed.
pub const FEED_URL: &str = concat!("https://github.com/", repo!(), "/releases.atom");
const TAG_MARK: &str = concat!("/", repo!(), "/releases/tag/");
const DOWNLOAD_BASE: &str = concat!("https://github.com/", repo!(), "/releases/download/");
/// One release by its tag from GitHub's API: asked only while installing a release that has no checksum file
/// beside its bundle (releases before 0.63), for GitHub's own digest.
pub const TAG_API: &str = concat!("https://api.github.com/repos/", repo!(), "/releases/tags/");
/// A check at start asks GitHub at most this often; in between the last answer is used.
pub const CHECK_EVERY_SECS: u64 = 6 * 60 * 60;
pub const ASSET: &str = "TurbineHub.flatpak";
/// No Turbine Hub bundle comes near this; anything bigger is stopped.
pub const MAX_BUNDLE: u64 = 1 << 30;

/// Compares Turbine Hub versions written with one dot ("0.3", "0.31", "1.0"; a leading "v" is ignored). The part after
/// the dot is read as decimal digits, so 0.3 < 0.31 < 0.4 < 1.0 (0.31 is "thirty-one hundredths", above 0.3 and
/// below 0.4). None if either isn't a one-dot version.
pub fn compare(a: &str, b: &str) -> Option<Ordering> {
    let (wa, fa) = parse_version(a)?;
    let (wb, fb) = parse_version(b)?;
    Some(wa.cmp(&wb).then_with(|| {
        // Compare fractions digit by digit, padding the shorter with zeros ("3" vs "31" is "30" vs "31").
        let len = fa.len().max(fb.len());
        let pad = |f: &str| format!("{f:0<len$}");
        pad(fa).cmp(&pad(fb))
    }))
}

/// Splits "v0.31" into (0, "31"). Trailing zeros of the fraction are dropped so "0.30" equals "0.3".
fn parse_version(v: &str) -> Option<(u64, &str)> {
    let v = v.trim();
    let v = v.strip_prefix(['v', 'V']).unwrap_or(v);
    let (whole, frac) = v.split_once('.').unwrap_or((v, ""));
    if whole.is_empty() || whole.len() > 9 || !whole.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((whole.parse().ok()?, frac.trim_end_matches('0')))
}

/// Is `candidate` newer than `current`?
pub fn is_newer(candidate: &str, current: &str) -> bool {
    compare(candidate, current) == Some(Ordering::Greater)
}

/// A release that can be installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// "0.31" (the tag without its "v").
    pub version: String,
    pub tag: String,
    pub name: String,
    pub prerelease: bool,
    pub notes: String,
    pub asset_url: String,
    pub asset_size: u64,
    /// SHA-256 from GitHub's asset digest or the release notes, when present.
    pub sha256: Option<String>,
    /// Address of a TurbineHub.flatpak.sha256 asset, when the release has one.
    pub sha256_asset_url: Option<String>,
    pub page_url: String,
}

/// Finds a SHA-256 for the bundle in release notes: a 64 hex digit run on a line that names the bundle or says
/// SHA-256, or the only such run in the notes.
pub fn sha256_from_notes(notes: &str) -> Option<String> {
    let runs = |line: &str| -> Vec<String> {
        line.split(|c: char| !c.is_ascii_hexdigit()).filter(|w| w.len() == 64).map(|w| w.to_ascii_lowercase()).collect()
    };
    for line in notes.lines() {
        let l = line.to_ascii_lowercase();
        if (l.contains("sha256") || l.contains("sha-256") || l.contains(&ASSET.to_ascii_lowercase())) && let Some(h) = runs(line).into_iter().next() {
            return Some(h);
        }
    }
    let all: Vec<String> = notes.lines().flat_map(runs).collect();
    (all.len() == 1).then(|| all[0].clone())
}

/// Reads the hash out of a .sha256 file ("<hash>  TurbineHub.flatpak" or just the hash).
pub fn parse_sha256_file(text: &str) -> Option<String> {
    let first = text.split_whitespace().next()?;
    sha256::is_hex_digest(first).then(|| first.to_ascii_lowercase())
}

/// Reads one release object from GitHub's API. None for drafts, releases without the bundle, or tags that aren't
/// one-dot versions.
pub fn release_from_json(r: &Json) -> Option<Release> {
    if r.get("draft").and_then(Json::as_bool) == Some(true) {
        return None;
    }
    let tag = r.str_at("tag_name")?.to_string();
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    parse_version(&version)?;
    let assets = r.get("assets").map(Json::as_array).unwrap_or(&[]);
    let bundle = assets.iter().find(|a| a.str_at("name") == Some(ASSET))?;
    let asset_url = bundle.str_at("browser_download_url")?.to_string();
    let asset_size = match bundle.get("size") {
        Some(Json::Num(n)) if *n > 0.0 => *n as u64,
        _ => 0,
    };
    let notes = r.str_at("body").unwrap_or("").to_string();
    let digest = bundle
        .str_at("digest")
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|h| sha256::is_hex_digest(h))
        .map(str::to_ascii_lowercase);
    let sha_asset = format!("{ASSET}.sha256");
    let sha256_asset_url = assets
        .iter()
        .find(|a| a.str_at("name").is_some_and(|n| n.eq_ignore_ascii_case(&sha_asset)))
        .and_then(|a| a.str_at("browser_download_url"))
        .map(str::to_string);
    Some(Release {
        sha256: digest.or_else(|| sha256_from_notes(&notes)),
        version,
        tag,
        name: r.str_at("name").unwrap_or("").to_string(),
        prerelease: r.get("prerelease").and_then(Json::as_bool).unwrap_or(false),
        notes,
        asset_url,
        asset_size,
        sha256_asset_url,
        page_url: r.str_at("html_url").unwrap_or("").to_string(),
    })
}

/// From GitHub's release list: the newest usable release. Pre-releases count only when `labs` is on.
pub fn pick_release(list_json: &str, labs: bool) -> Result<Option<Release>, String> {
    let j = json::parse(list_json).map_err(|e| format!("GitHub's answer can't be read ({e})"))?;
    let items: Vec<Release> = match &j {
        Json::Arr(a) => a.iter().filter_map(release_from_json).collect(),
        obj @ Json::Obj(_) => release_from_json(obj).into_iter().collect(),
        _ => return Err("GitHub's answer isn't a release list".into()),
    };
    Ok(items
        .into_iter()
        .filter(|r| labs || !r.prerelease)
        .max_by(|a, b| compare(&a.version, &b.version).unwrap_or(Ordering::Equal)))
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    UpToDate,
    Available(Box<Release>),
}

/// Decides from a release list and the running version (pure).
pub fn decide(list_json: &str, current: &str, labs: bool) -> Result<Check, String> {
    Ok(match pick_release(list_json, labs)? {
        Some(r) if is_newer(&r.version, current) => Check::Available(Box::new(r)),
        _ => Check::UpToDate,
    })
}

/// "v0.63" from ".../releases/tag/v0.63" (where the latest page ends up). None for any other address or a tag
/// that isn't a one-dot version.
pub fn tag_from_url(url: &str) -> Option<String> {
    let at = url.find(TAG_MARK)?;
    let tag: String = url[at + TAG_MARK.len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '.').collect();
    parse_version(&tag)?;
    Some(tag)
}

/// Every one-dot tag the releases feed names, in its order (newest first).
pub fn tags_from_feed(feed: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (at, _) in feed.match_indices(TAG_MARK) {
        if let Some(tag) = tag_from_url(&feed[at..]) && !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

/// The release a tag names, with the addresses its bundle and the bundle's checksum file have on GitHub. The size
/// isn't known this way; the SHA-256 (required before anything installs) covers it.
pub fn release_for_tag(tag: &str, prerelease: bool) -> Option<Release> {
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    parse_version(&version)?;
    Some(Release {
        version,
        tag: tag.to_string(),
        name: String::new(),
        prerelease,
        notes: String::new(),
        asset_url: format!("{DOWNLOAD_BASE}{tag}/{ASSET}"),
        asset_size: 0,
        sha256: None,
        sha256_asset_url: Some(format!("{DOWNLOAD_BASE}{tag}/{ASSET}.sha256")),
        page_url: format!("https://github.com{TAG_MARK}{tag}"),
    })
}

/// The words for a limit or refusal from GitHub at an update check; other failures keep their own.
pub fn limit_words(e: NetError) -> NetError {
    match e {
        NetError::GitHubLimit(m) => {
            NetError::Failed(format!("GitHub is limiting update checks right now. Try again in about {}.", crate::net::minutes_words(m)))
        }
        e => e,
    }
}

/// The newest full release (never a pre-release), or None while there is none. Asks the latest page and reads the
/// tag from where GitHub redirects it; the page itself is thrown away.
pub fn latest(client: &Client) -> Result<Option<Release>, NetError> {
    match client.send(&Request::get(LATEST_PAGE).max_bytes(8 << 20).max_time(30), None) {
        Ok(r) => {
            let _ = std::fs::remove_file(&r.body);
            Ok(tag_from_url(&r.effective_url).and_then(|t| release_for_tag(&t, false)))
        }
        Err(NetError::NotFound) => Ok(None),
        Err(e) => Err(limit_words(e)),
    }
}

/// The newest release: full releases only, or with Labs mode whatever the feed lists (a tag that isn't the latest
/// full release is a pre-release).
pub fn newest(client: &Client, labs: bool) -> Result<Option<Release>, NetError> {
    let mut best = latest(client)?;
    if labs {
        let stable = best.as_ref().map(|r| r.tag.clone());
        let feed = client.send(&Request::get(FEED_URL).max_bytes(8 << 20).max_time(30), None).map_err(limit_words)?.take_text()?;
        for tag in tags_from_feed(&feed) {
            let Some(r) = release_for_tag(&tag, stable.as_deref() != Some(tag.as_str())) else { continue };
            if best.as_ref().is_none_or(|b| compare(&r.version, &b.version) == Some(Ordering::Greater)) {
                best = Some(r);
            }
        }
    }
    Ok(best)
}

/// Asks GitHub for the newest release, without its API. Without Labs mode only full releases count; with it the
/// releases feed is read too, so pre-releases count.
pub fn check(client: &Client, current: &str, labs: bool) -> Result<Check, NetError> {
    Ok(match newest(client, labs)? {
        Some(r) if is_newer(&r.version, current) => Check::Available(Box::new(r)),
        _ => Check::UpToDate,
    })
}

/// The last answer of an update check, kept so a start within `CHECK_EVERY_SECS` of it doesn't ask GitHub
/// again. Check for updates (the button) always asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remembered {
    /// When GitHub answered (seconds since 1970).
    pub at: u64,
    /// Whether pre-releases counted.
    pub labs: bool,
    /// The newest release's tag, "" when there was none.
    pub tag: String,
    pub prerelease: bool,
}

impl Remembered {
    pub fn of(newest: Option<&Release>, labs: bool, at: u64) -> Remembered {
        Remembered { at, labs, tag: newest.map(|r| r.tag.clone()).unwrap_or_default(), prerelease: newest.is_some_and(|r| r.prerelease) }
    }

    /// One line: "<at> <labs 0/1> <prerelease 0/1> <tag>".
    pub fn to_text(&self) -> String {
        format!("{} {} {} {}\n", self.at, self.labs as u8, self.prerelease as u8, self.tag)
    }

    pub fn parse(text: &str) -> Option<Remembered> {
        let mut w = text.split_whitespace();
        let at = w.next()?.parse().ok()?;
        let labs = w.next()? == "1";
        let prerelease = w.next()? == "1";
        let tag = w.next().unwrap_or("").to_string();
        if !tag.is_empty() {
            release_for_tag(&tag, prerelease)?;
        }
        Some(Remembered { at, labs, tag, prerelease })
    }

    /// May a start at `now` use this instead of asking? Only for the same channel, and not from the future.
    pub fn fresh(&self, labs: bool, now: u64) -> bool {
        self.labs == labs && self.at <= now && now - self.at < CHECK_EVERY_SECS
    }

    /// What it says to a Turbine Hub running `current`.
    pub fn answer(&self, current: &str) -> Check {
        match release_for_tag(&self.tag, self.prerelease) {
            Some(r) if is_newer(&r.version, current) => Check::Available(Box::new(r)),
            _ => Check::UpToDate,
        }
    }
}

/// Where the last answer is kept: one small file in Turbine Hub's cache folder.
pub fn remembered_file(cache: &Path) -> std::path::PathBuf {
    cache.join("update-check")
}

/// Looks for a newer Turbine Hub without asking GitHub more than once in `CHECK_EVERY_SECS`: the checks Turbine
/// Hub makes by itself (at its start, after every finished job) use the answer kept in `kept` while it is fresh.
/// `ask_anyway`: the player pressed Check now, which always asks. `newest` is the question itself (`newest`
/// through the internet). Answers the check and whether GitHub was asked for it.
pub fn check_kept(kept: &Path, current: &str, labs: bool, now: u64, ask_anyway: bool, newest: &mut dyn FnMut() -> Result<Option<Release>, NetError>) -> Result<(Check, bool), NetError> {
    if !ask_anyway && let Some(last) = std::fs::read_to_string(kept).ok().and_then(|t| Remembered::parse(&t)).filter(|r| r.fresh(labs, now)) {
        return Ok((last.answer(current), false));
    }
    let found = newest()?;
    let last = Remembered::of(found.as_ref(), labs, now);
    // An answer that couldn't be kept only means the next start asks again.
    let _ = crate::write_atomic(kept, last.to_text().as_bytes());
    Ok((last.answer(current), true))
}

/// Why installing the downloaded bundle failed, in words the player can act on. `said`: what flatpak printed.
/// Flatpak's own words are for the log.
pub fn install_problem(said: &str) -> String {
    let low = said.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| low.contains(w));
    if has(&["no space", "not enough disk space", "disk full"]) {
        "There isn't enough free space for the update. Free some space, then try again.".into()
    } else if has(&["not allowed", "authoriz", "polkit", "permission denied", "requires root"]) {
        "Turbine Hub is installed for every user, so the system has to allow its update. In Desktop Mode, open Turbine Hub and update again: the system asks for your password there.".into()
    } else if has(&["summary", "remote", "could not resolve", "couldn't resolve", "unable to connect", "network", "timed out", "connection"]) {
        "Turbine Hub's update needs the internet to fetch a part from Flathub. Connect, then try again.".into()
    } else {
        let line = said.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("it stopped with an error");
        format!("The update couldn't be installed ({}). Nothing changed: this version keeps working.", line.trim_end_matches('.'))
    }
}

/// After an update the desktop's menu keeps the entry and icon from before until its list of programs is rebuilt.
/// These are KDE's own tool for that (the first one there is enough); where neither is there nothing happens.
pub fn menu_refresh() -> Vec<Vec<String>> {
    ["kbuildsycoca6", "kbuildsycoca5"].iter().map(|tool| ["flatpak-spawn", "--host", tool, "--noincremental"].iter().map(|s| s.to_string()).collect()).collect()
}

/// What to do once the new version is installed: the running one keeps going until it is closed. In a Desktop
/// Mode window Turbine Hub can start itself again; in Game Mode only what Steam starts shows, so the player
/// closes it and starts it from Steam.
pub fn after_install_words(version: &str, window: bool) -> String {
    if window { format!("Turbine Hub {version} is installed. Restart Turbine Hub to use it.") } else { format!("Turbine Hub {version} is installed. Close Turbine Hub, then start it again from Steam.") }
}

/// The host command that starts Turbine Hub again (Desktop Mode, after an update).
pub fn start_again(flatpak_id: &str) -> Vec<String> {
    ["flatpak-spawn", "--host", "flatpak", "run", flatpak_id].iter().map(|s| s.to_string()).collect()
}

/// Downloads the bundle to a temp file and checks it: size when the release gave one, SHA-256 from the release
/// (GitHub's asset digest or the notes), the checksum file beside the bundle, or, for a release without that file,
/// GitHub's digest for the bundle. A release without any published SHA-256 is refused, so nothing unchecked is
/// installed.
pub fn download(client: &Client, release: &Release, progress: Option<Progress>) -> Result<Download, NetError> {
    let mut sha = release.sha256.clone();
    let mut size = release.asset_size;
    if sha.is_none() && let Some(url) = &release.sha256_asset_url {
        match client.send(&Request::get(url).max_bytes(4096).max_time(30), None) {
            Ok(r) => sha = parse_sha256_file(&r.take_text()?),
            Err(NetError::NotFound) => {}
            Err(e) => return Err(limit_words(e)),
        }
    }
    if sha.is_none() && !release.tag.is_empty() {
        let req = Request::get(&format!("{TAG_API}{}", release.tag)).header("Accept", "application/vnd.github+json").max_bytes(8 << 20).max_time(30);
        match client.send(&req, None) {
            Ok(r) => {
                if let Ok(Some(listed)) = pick_release(&r.take_text()?, true) && listed.tag == release.tag && listed.asset_url == release.asset_url {
                    sha = listed.sha256;
                    size = listed.asset_size;
                }
            }
            Err(NetError::NotFound) => {}
            Err(e) => return Err(limit_words(e)),
        }
    }
    let Some(sha) = sha else {
        return Err(NetError::Failed("This release doesn't publish a SHA-256 for its bundle, so Turbine Hub won't install it.".into()));
    };
    let check = Expect {
        max_bytes: if size > 0 { size } else { MAX_BUNDLE },
        size: (size > 0).then_some(size),
        sha256: Some(sha),
    };
    client.download(&release.asset_url, &check, progress)
}

/// The program and arguments that install a downloaded bundle on the host (run by the app, never through a
/// shell). --reinstall lets a bundle replace the installed copy of the same app.
pub fn install_args(bundle: &Path) -> Vec<String> {
    install_args_to(bundle, "--user")
}

/// Where the running Turbine Hub is installed, from the sandbox's /.flatpak-info
/// (`[Instance] app-path`): "--system" for the system installation (/var/lib/flatpak), "--user" otherwise.
pub fn install_scope(flatpak_info: &str) -> &'static str {
    let mut in_instance = false;
    for line in flatpak_info.lines().map(str::trim) {
        if line.starts_with('[') {
            in_instance = line == "[Instance]";
        } else if let (true, Some(path)) = (in_instance, line.strip_prefix("app-path=")) {
            return if path.trim().starts_with("/var/lib/flatpak/") { "--system" } else { "--user" };
        }
    }
    "--user"
}

/// The same, into the installation `scope` names (install_scope), so an update never adds a second copy.
pub fn install_args_to(bundle: &Path, scope: &str) -> Vec<String> {
    let scope = if scope == "--system" { "--system" } else { "--user" };
    ["flatpak-spawn", "--host", "flatpak", "install", scope, "-y", "--reinstall", "--bundle"]
        .iter()
        .map(|s| s.to_string())
        .chain(std::iter::once(bundle.display().to_string()))
        .collect()
}
