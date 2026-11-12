//! The newest GE-Proton in one press. GE-Proton (github.com/GloriousEggroll/proton-ge-custom) is a community build
//! of Proton with fixes Valve's builds don't carry yet; "Try another Proton" is where a player wants it.
//!
//! Turbine Hub asks GitHub which release is the newest (its API through `sources`, so the last answer is kept; when
//! the API doesn't answer, the releases page, read by where it redirects to, the way Turbine Hub's own update check
//! does), downloads the release's .tar.gz into Turbine Hub's cache, checks its SHA-512 against the .sha512sum
//! published beside it, unpacks it with `tar` (which refuses anything that would land outside) into a scratch
//! folder inside Steam's compatibilitytools.d, and only then gives it its name there. Steam lists it after its
//! next start; Turbine Hub's own Proton list shows it at once.
//!
//! Since GE-Proton 11 a release has one build for each kind of processor, named after it
//! ("GE-Proton11-12-x86_64.tar.gz", with a folder of that name inside); before that there was one build, named
//! like the release ("GE-Proton10-20.tar.gz"). Both are read, and the build keeps the folder name its makers gave it.
//!
//! Removing is for the builds Turbine Hub fetched itself (it keeps their names), and refused while Steam or a pick
//! in Turbine Hub still names the build for a game.

use crate::compat_tool as tool;
use crate::json::{self, Json};
use crate::net::{Client, Expect, NetError, Request};
use crate::sources::{self, Origin, Source};
use crate::{sha512, tar};
use std::fs;
use std::path::{Path, PathBuf};

macro_rules! repo {
    () => {
        "GloriousEggroll/proton-ge-custom"
    };
}
/// The newest release with its files, from GitHub's API.
pub const API_LATEST: &str = concat!("https://api.github.com/repos/", repo!(), "/releases/latest");
/// The newest release's own page: GitHub redirects it to ".../releases/tag/<tag>".
pub const LATEST_PAGE: &str = concat!("https://github.com/", repo!(), "/releases/latest");
const TAG_MARK: &str = concat!("/", repo!(), "/releases/tag/");
const DOWNLOAD_BASE: &str = concat!("https://github.com/", repo!(), "/releases/download/");
/// Where it comes from, in words for the dialog.
pub const WHERE: &str = concat!("github.com/", repo!());

/// GitHub's answer about the newest release, kept for six hours (and used when GitHub doesn't answer).
pub const RELEASE: Source = Source { name: "proton-ge-release.json", urls: &[API_LATEST], fresh_s: 6 * 3600, bundled: None, valid: release_valid };

/// No GE-Proton archive comes near this (they are around half a gigabyte); anything bigger is stopped.
pub const MAX_ARCHIVE: u64 = 3 << 30;
/// The most a build may unpack to, and the most entries it may have.
pub const LIMITS: tar::Limits = tar::Limits { total: 12 << 30, entries: 400_000 };

/// Is this a GE-Proton release name ("GE-Proton10-20")? It becomes a folder name and part of an address, so only
/// that exact shape passes.
pub fn tag_ok(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix("GE-Proton") else { return false };
    let Some((major, minor)) = rest.split_once('-') else { return false };
    let digits = |t: &str| !t.is_empty() && t.len() <= 4 && t.bytes().all(|b| b.is_ascii_digit());
    digits(major) && digits(minor)
}

/// The kind of processor this Turbine Hub was built for ("x86_64", "aarch64"): a release has one build for each.
pub fn arch() -> &'static str {
    std::env::consts::ARCH
}

/// The names a release's build may have, the one in use today first: "GE-Proton11-12-x86_64" (a build for each
/// kind of processor, since GE-Proton 11), and plain "GE-Proton10-20" (every release before that).
pub fn names(tag: &str) -> [String; 2] {
    [format!("{tag}-{}", arch()), tag.to_string()]
}

/// Is this a build's name: a release name, with or without the kind of processor after it? It becomes a folder
/// name, so only those shapes pass.
pub fn name_ok(name: &str) -> bool {
    tag_ok(name) || ["-x86_64", "-aarch64"].iter().any(|s| name.strip_suffix(s).is_some_and(tag_ok))
}

/// A release that can be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// "GE-Proton11-12": the release's tag.
    pub tag: String,
    /// "GE-Proton11-12-x86_64": the build's name, which its files and the folder inside its archive carry.
    pub name: String,
    pub archive_url: String,
    /// The archive's size when GitHub said it.
    pub archive_size: Option<u64>,
    pub sums_url: String,
}

impl Release {
    pub fn archive_name(&self) -> String {
        format!("{}.tar.gz", self.name)
    }

    fn named(tag: &str, name: &str) -> Release {
        Release { tag: tag.to_string(), name: name.to_string(), archive_url: format!("{DOWNLOAD_BASE}{tag}/{name}.tar.gz"), archive_size: None, sums_url: format!("{DOWNLOAD_BASE}{tag}/{name}.sha512sum") }
    }

    /// The same release under the plain name releases had before GE-Proton 11, for when the name in use today
    /// isn't there. Those builds were all for x86_64, so there is none for any other processor.
    pub fn older_naming(&self) -> Option<Release> {
        (arch() == "x86_64" && self.name != self.tag).then(|| Release::named(&self.tag, &self.tag))
    }
}

/// The release a tag names, with the addresses its files have on GitHub under the name in use today.
pub fn release_for_tag(tag: &str) -> Option<Release> {
    tag_ok(tag).then(|| Release::named(tag, &names(tag)[0]))
}

/// Reads GitHub's answer about a release: its tag, and this machine's .tar.gz and .sha512sum where the answer
/// lists them under this repository's download address (anything else in their place is passed over). An answer
/// that lists no build for this machine gives the addresses the tag does.
pub fn release_from_json(text: &str) -> Option<Release> {
    let j = json::parse(text).ok()?;
    let tag = j.str_at("tag_name").filter(|t| tag_ok(t))?;
    let assets = j.get("assets").map(Json::as_array).unwrap_or(&[]);
    let find = |name: String| assets.iter().find(|a| a.str_at("name") == Some(name.as_str()) && a.str_at("browser_download_url").is_some_and(|u| u.starts_with(DOWNLOAD_BASE) && crate::net::allowed_url(u)));
    for name in names(tag).iter().filter(|n| n.as_str() != tag || arch() == "x86_64") {
        let mut r = Release::named(tag, name);
        let Some(a) = find(r.archive_name()) else { continue };
        r.archive_url = a.str_at("browser_download_url").unwrap_or_default().to_string();
        r.archive_size = a.get("size").and_then(Json::as_f64).filter(|n| *n > 0.0).map(|n| n as u64);
        if let Some(a) = find(format!("{name}.sha512sum")) {
            r.sums_url = a.str_at("browser_download_url").unwrap_or_default().to_string();
        }
        return Some(r);
    }
    release_for_tag(tag)
}

fn release_valid(text: &str) -> bool {
    release_from_json(text).is_some()
}

/// "GE-Proton10-20" from ".../releases/tag/GE-Proton10-20" (where the latest page ends up).
pub fn tag_from_url(url: &str) -> Option<String> {
    let at = url.find(TAG_MARK)?;
    let tag: String = url[at + TAG_MARK.len()..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    tag_ok(&tag).then_some(tag)
}

/// The newest release as it was last found, without going online. None before the first answer.
pub fn known(cache: &Path) -> Option<Release> {
    release_from_json(&sources::local(&RELEASE, cache))
}

/// Asks which release is the newest: GitHub's API through `sources` (a saved answer younger than six hours is
/// used as it is), then the releases page when the API didn't answer now (it allows only 60 questions an hour),
/// then whatever was found last.
pub fn newest(client: &Client, cache: &Path, now: u64) -> Result<Release, String> {
    let got = sources::get(&RELEASE, cache, now, &mut |u| client.get_text(u, &[]).map_err(|e| e.to_string()));
    if !got.offline
        && let Some(r) = release_from_json(&got.text)
    {
        return Ok(r);
    }
    match client.send(&Request::get(LATEST_PAGE).max_bytes(8 << 20).max_time(30), None) {
        Ok(page) => {
            let _ = fs::remove_file(&page.body);
            let r = tag_from_url(&page.effective_url).and_then(|t| release_for_tag(&t)).ok_or("GitHub's page for the newest GE-Proton didn't name a release.")?;
            // Kept like the API's answer, so the Proton page knows the newest one too.
            let _ = crate::write_atomic(&cache.join(RELEASE.name), format!("{{\"tag_name\":{}}}\n", json::quote(&r.tag)).as_bytes());
            Ok(r)
        }
        Err(e) => match (got.origin, release_from_json(&got.text)) {
            (Origin::Saved, Some(r)) => Ok(r),
            _ => Err(format!("Couldn't find the newest GE-Proton. {e}")),
        },
    }
}

/// Where a build's folder is, or goes.
pub fn build_dir(steam_root: &Path, name: &str) -> PathBuf {
    tool::tools_dir(steam_root).join(name)
}

/// The folder a release is installed as, when it is (under either of its names, whoever put it there).
pub fn installed_as(steam_root: &Path, tag: &str) -> Option<String> {
    names(tag).into_iter().find(|n| tag_ok(tag) && tool::is_proton(&build_dir(steam_root, n)))
}

/// Is this release installed?
pub fn installed(steam_root: &Path, tag: &str) -> bool {
    installed_as(steam_root, tag).is_some()
}

/// Turbine Hub's list of the builds it fetched itself, one name a line.
pub fn record_file(folder: &Path) -> PathBuf {
    folder.join("proton-get.txt")
}

/// The builds Turbine Hub fetched that are still there.
pub fn fetched(steam_root: &Path, folder: &Path) -> Vec<String> {
    let text = fs::read_to_string(record_file(folder)).unwrap_or_default();
    let mut out: Vec<String> = text.lines().map(str::trim).filter(|t| name_ok(t) && build_dir(steam_root, t).is_dir()).map(String::from).collect();
    out.dedup();
    out
}

fn remember(folder: &Path, name: &str, there: bool) {
    let text = fs::read_to_string(record_file(folder)).unwrap_or_default();
    let mut names: Vec<&str> = text.lines().map(str::trim).filter(|t| name_ok(t) && *t != name).collect();
    if there {
        names.push(name);
    }
    let _ = crate::write_atomic(&record_file(folder), names.iter().map(|n| format!("{n}\n")).collect::<String>().as_bytes());
}

/// Unpacks a build's archive into Steam's folder for tools. `tag`: the build's name, which the one folder in the
/// archive has to carry. Everything goes into a scratch folder there first (the
/// same disk, so giving it its name afterwards is one step); the build gets its name only when all of it unpacked,
/// the archive held exactly the one folder its name promises, and that folder is a Proton. Otherwise nothing is
/// left behind.
pub fn install_archive(archive: &Path, tag: &str, steam_root: &Path) -> Result<(), String> {
    if !name_ok(tag) {
        return Err("That isn't a GE-Proton release.".into());
    }
    let tools = tool::tools_dir(steam_root);
    let target = tools.join(tag);
    if fs::symlink_metadata(&target).is_ok() {
        return if tool::is_proton(&target) { Ok(()) } else { Err(format!("A folder called {tag} is in Steam's folder for tools already, and it isn't a Proton. Turbine Hub left it alone.")) };
    }
    fs::create_dir_all(&tools).map_err(|e| format!("Couldn't make Steam's folder for tools: {e}"))?;
    let scratch = tools.join(format!(".{tag}.turbinehub-unpacking"));
    let _ = fs::remove_dir_all(&scratch);
    let result = (|| {
        let done = tar::unpack_tar_gz(archive, &scratch, LIMITS).map_err(|e| format!("Couldn't unpack {tag}: {e}. Nothing was installed."))?;
        if done.top != [tag.to_string()] || !tool::is_proton(&scratch.join(tag)) {
            return Err(format!("What was downloaded isn't the Proton build {tag}. Nothing was installed."));
        }
        fs::rename(scratch.join(tag), &target).map_err(|e| format!("Couldn't put {tag} into Steam's folder for tools: {e}"))
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

/// Downloads a release, checks it and installs it. `tag`: the release to get, or "" for the newest. `say` gets a
/// line for the player at each stage. Answers the name of the build that is installed now (its folder's name).
pub fn install(client: &Client, cache: &Path, steam_root: &Path, folder: &Path, tag: &str, now: u64, say: &dyn Fn(String)) -> Result<String, String> {
    let mut release = if tag.is_empty() {
        say("Asking GitHub for the newest GE-Proton".into());
        newest(client, cache, now)?
    } else {
        // What GitHub last said about this very release names its files; otherwise the tag does.
        known(cache).filter(|k| k.tag == tag).or_else(|| release_for_tag(tag)).ok_or("That isn't a GE-Proton release.")?
    };
    let tag = release.tag.clone();
    if let Some(name) = installed_as(steam_root, &tag) {
        return Ok(name);
    }
    // The checksum first: without it nothing big is downloaded at all.
    let sums_of = |r: &Release| client.send(&Request::get(&r.sums_url).max_bytes(64 << 10).max_time(60), None).and_then(|r| r.take_text());
    let sums = match (sums_of(&release), release.older_naming()) {
        (Err(NetError::NotFound), Some(older)) => {
            release = older;
            sums_of(&release)
        }
        (first, _) => first,
    }
    .map_err(|e| format!("Couldn't get the checksum published with {tag}. {e}"))?;
    let want = sha512::from_sum_file(&sums, &release.archive_name()).ok_or_else(|| format!("The checksum file published with {tag} doesn't name its archive, so nothing was downloaded."))?;
    say(format!("Downloading {tag}"));
    let step = std::cell::Cell::new(0u64);
    let mut progress = |got: u64, total: Option<u64>| {
        // A line every 25 MB, not at every look.
        if got / (25 << 20) != step.get() {
            step.set(got / (25 << 20));
            say(match total {
                Some(t) if t > 0 => format!("Downloading {tag}: {} of {}", crate::net::human_size(got), crate::net::human_size(t)),
                _ => format!("Downloading {tag}: {}", crate::net::human_size(got)),
            });
        }
        true
    };
    let got = client.download(&release.archive_url, &Expect::up_to(MAX_ARCHIVE), Some(&mut progress)).map_err(|e| format!("Couldn't download {tag}. {e}"))?;
    say(format!("Checking {tag}"));
    let result = match sha512::file_hex(&got.path) {
        Ok(have) if sha512::same_digest(&have, &want) => {
            say(format!("Unpacking {tag}"));
            install_archive(&got.path, &release.name, steam_root)
        }
        Ok(_) => Err(NetError::Mismatch("its SHA-512 is different".into()).to_string()),
        Err(e) => Err(format!("Couldn't read the download ({e}).")),
    };
    let _ = fs::remove_file(&got.path);
    result?;
    remember(folder, &release.name, true);
    Ok(release.name)
}

/// The name Steam's settings have for a build: what its compatibilitytool.vdf says, else its folder's name.
fn steam_name(dir: &Path, tag: &str) -> String {
    fs::read_to_string(dir.join("compatibilitytool.vdf")).ok().and_then(|t| crate::vdf::parse(&t).ok()).and_then(|v| v.path(&["compatibilitytools", "compat_tools"]).and_then(|c| c.entries().first().map(|(k, _)| k.clone()))).unwrap_or_else(|| tag.to_string())
}

/// Who is set to use a build: Steam app ids, from Steam's own settings (a Proton forced for a game) and from the
/// picks in Turbine Hub. "0" stands for Steam's choice for all games.
pub fn used_by(steam_root: &Path, folder: &Path, tag: &str) -> Vec<String> {
    let dir = build_dir(steam_root, tag);
    let config = tool::config_text(steam_root);
    let mut out = Vec::new();
    for name in [steam_name(&dir, tag), tag.to_string()] {
        if tool::chosen_in(&config, "0").as_deref() == Some(name.as_str()) {
            out.push("0".to_string());
        }
        out.extend(tool::games_using_in(&config, &name));
    }
    if let Ok(rd) = fs::read_dir(tool::launch_dir(folder)) {
        for e in rd.flatten() {
            let file = e.file_name().to_string_lossy().into_owned();
            if let Some(app) = file.strip_suffix(".proton").filter(|a| !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit()))
                && tool::proton_pick(folder, app).is_some_and(|d| d == dir)
            {
                out.push(app.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Takes a build Turbine Hub fetched out again. Refused for a build it didn't fetch, and while a game is set to
/// use it (the answer then says how many).
pub fn remove(steam_root: &Path, folder: &Path, tag: &str) -> Result<(), String> {
    if !fetched(steam_root, folder).iter().any(|t| t == tag) {
        return Err(format!("Turbine Hub didn't fetch {tag}, so it leaves it alone."));
    }
    let dir = build_dir(steam_root, tag);
    // Only ever a real folder inside Steam's folder for tools: a link there is somebody else's. (`fetched` let
    // only a build's name through, so `tag` can't lead anywhere else.)
    if !fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) {
        return Err(format!("{tag} isn't a folder of its own in Steam's folder for tools. Turbine Hub left it alone."));
    }
    let using = used_by(steam_root, folder, tag);
    if !using.is_empty() {
        let who = if using.iter().any(|u| u == "0") { "Steam uses it for all games".to_string() } else if using.len() == 1 { "1 game is set to use it".to_string() } else { format!("{} games are set to use it", using.len()) };
        return Err(format!("{who}. Pick another Proton there first."));
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("Couldn't remove {tag}: {e}"))?;
    remember(folder, tag, false);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-protonget-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_a_ge_proton_name_passes() {
        for good in ["GE-Proton10-20", "GE-Proton9-1", "GE-Proton8-32"] {
            assert!(tag_ok(good), "{good}");
        }
        for bad in ["", "GE-Proton", "GE-Proton10", "GE-Proton10-", "GE-Proton-4", "Proton-10-4", "GE-Proton10-4/..", "GE-Proton10-4 ", "../GE-Proton10-4", "GE-Proton10-4-LoL", "GE-Proton10000-4", "ge-proton10-4"] {
            assert!(!tag_ok(bad), "{bad}");
        }
        assert_eq!(tag_from_url("https://github.com/GloriousEggroll/proton-ge-custom/releases/tag/GE-Proton10-20").as_deref(), Some("GE-Proton10-20"));
        assert_eq!(tag_from_url("https://github.com/GloriousEggroll/proton-ge-custom/releases/tag/GE-Proton10-20?x=1").as_deref(), Some("GE-Proton10-20"));
        assert_eq!(tag_from_url("https://github.com/someone/else/releases/tag/GE-Proton10-20"), None);
        assert_eq!(tag_from_url("https://github.com/GloriousEggroll/proton-ge-custom/releases"), None);
        for u in [API_LATEST, LATEST_PAGE, &release_for_tag("GE-Proton10-20").unwrap().archive_url, &release_for_tag("GE-Proton10-20").unwrap().sums_url] {
            assert!(crate::net::allowed_url(u), "{u}");
        }
        // A build's name: the release's, with or without the kind of processor. Nothing else is a folder name.
        for good in ["GE-Proton10-20", "GE-Proton11-12-x86_64", "GE-Proton11-12-aarch64"] {
            assert!(name_ok(good), "{good}");
        }
        for bad in ["GE-Proton11-12-x86", "GE-Proton11-12-x86_64/..", "x86_64", "-x86_64", "GE-Proton11-12-x86_64-x86_64", ".."] {
            assert!(!name_ok(bad), "{bad}");
        }
    }

    #[test]
    fn the_release_is_read_from_githubs_answer_and_only_its_own_files_count() {
        let tag = "GE-Proton11-99";
        let base = format!("https://github.com/GloriousEggroll/proton-ge-custom/releases/download/{tag}");
        let asset = |name: &str, size: u64, at: &str| format!(r#"{{"name":"{name}","size":{size},"browser_download_url":"{at}/{name}"}}"#);
        let answer = |assets: &[String]| format!(r#"{{"tag_name":"{tag}","assets":[{}]}}"#, assets.join(","));
        // Today's shape: a build for each kind of processor. This machine's is taken.
        let mine = format!("{tag}-{}", arch());
        let both: Vec<String> = ["aarch64", "x86_64"].iter().flat_map(|a| [asset(&format!("{tag}-{a}.sha512sum"), 160, &base), asset(&format!("{tag}-{a}.tar.gz"), if *a == arch() { 566_859_228 } else { 650_286_473 }, &base)]).collect();
        let r = release_from_json(&answer(&both)).unwrap();
        assert_eq!((r.tag.as_str(), r.name.as_str(), r.archive_size), (tag, mine.as_str(), Some(566_859_228)));
        assert_eq!((r.archive_name(), r.archive_url, r.sums_url), (format!("{mine}.tar.gz"), format!("{base}/{mine}.tar.gz"), format!("{base}/{mine}.sha512sum")));
        // The shape before GE-Proton 11: one build, named like the release (those were all for x86_64).
        let plain = answer(&[asset(&format!("{tag}.sha512sum"), 150, &base), asset(&format!("{tag}.tar.gz"), 480_000_000, &base), asset(&format!("{tag}.tar.zst"), 1, &base)]);
        let r = release_from_json(&plain).unwrap();
        if arch() == "x86_64" {
            assert_eq!((r.name.as_str(), r.archive_size, r.archive_url.as_str()), (tag, Some(480_000_000), format!("{base}/{tag}.tar.gz").as_str()));
        } else {
            assert_eq!(r, release_for_tag(tag).unwrap());
        }
        // A file listed at some other address is passed over: the release's own address is used.
        let r = release_from_json(&answer(&[asset(&format!("{mine}.tar.gz"), 5, "https://example.com")])).unwrap();
        assert_eq!((r.archive_url, r.archive_size), (format!("{base}/{mine}.tar.gz"), None));
        // Only a tag: the addresses come from it, under today's name, with the older name to fall back on. A
        // limit message or another project's tag is no release.
        let r = release_from_json(&format!(r#"{{"tag_name":"{tag}"}}"#)).unwrap();
        assert_eq!((Some(&r), r.name.as_str()), (release_for_tag(tag).as_ref(), mine.as_str()));
        assert_eq!(r.older_naming().map(|o| o.archive_url), (arch() == "x86_64").then(|| format!("{base}/{tag}.tar.gz")));
        assert_eq!(r.older_naming().and_then(|o| o.older_naming()), None);
        assert!(!(RELEASE.valid)(r#"{"message":"API rate limit exceeded"}"#) && !(RELEASE.valid)(r#"{"tag_name":"v1.2"}"#) && !(RELEASE.valid)("<html>"));
        let d = dir("known");
        assert_eq!(known(&d), None);
        fs::write(d.join(RELEASE.name), answer(&both)).unwrap();
        assert_eq!(known(&d).unwrap().name, mine);
        // A release counts as installed under either name, whoever put it there.
        let root = steam(&d);
        assert_eq!(installed_as(&root, tag), None);
        for name in [tag, mine.as_str()] {
            let build = build_dir(&root, name);
            fs::create_dir_all(&build).unwrap();
            fs::write(build.join("proton"), "").unwrap();
            fs::write(build.join("toolmanifest.vdf"), "").unwrap();
            assert_eq!(installed_as(&root, tag).as_deref(), Some(name));
            fs::remove_dir_all(&build).unwrap();
        }
        let _ = fs::remove_dir_all(&d);
    }

    fn steam(d: &Path) -> PathBuf {
        let root = d.join("Steam");
        fs::create_dir_all(root.join("config")).unwrap();
        root
    }

    #[test]
    fn an_archive_is_installed_whole_or_not_at_all() {
        let d = dir("install");
        let root = steam(&d);
        let tag = "GE-Proton10-99";
        let good = d.join("good.tar.gz");
        fs::write(&good, crate::deflate::gzip(&tar::testing::proton(tag))).unwrap();
        install_archive(&good, tag, &root).unwrap();
        assert!(installed(&root, tag));
        let list = tool::installed_protons(&root);
        assert_eq!(list.iter().map(|p| (p.name.as_str(), p.steam_name.as_deref())).collect::<Vec<_>>(), vec![(tag, Some(tag))]);
        // Nothing of the scratch folder stays, and a second go changes nothing.
        let names = |p: &Path| -> Vec<String> { fs::read_dir(p).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect() };
        assert_eq!(names(&tool::tools_dir(&root)), vec![tag]);
        install_archive(&good, tag, &root).unwrap();
        // The archive of another build under this name, a build with a file that climbs out, and something that
        // isn't an archive: each is refused and leaves nothing behind.
        let other = "GE-Proton10-98";
        assert!(install_archive(&good, other, &root).unwrap_err().contains("isn't the Proton build"));
        let mut climbing = tar::testing::proton(other);
        climbing.truncate(climbing.len() - 1024);
        climbing.extend(tar::testing::archive(&[tar::testing::entry(b'0', "GE-Proton10-98/../../escaped", "", b"x")]));
        let bad = d.join("bad.tar.gz");
        fs::write(&bad, crate::deflate::gzip(&climbing)).unwrap();
        let why = install_archive(&bad, other, &root).unwrap_err();
        assert!(why.contains("outside its folder") && why.ends_with("Nothing was installed."), "{why}");
        fs::write(&bad, b"<html>Not Found</html> and more words to pass for a file").unwrap();
        assert!(install_archive(&bad, other, &root).unwrap_err().contains("isn't a .tar.gz"));
        assert!(install_archive(&good, "../x", &root).is_err());
        assert_eq!(names(&tool::tools_dir(&root)), vec![tag]);
        assert!(!d.join("escaped").exists() && !root.join("escaped").exists());
        // A folder of that name that isn't a Proton is somebody else's: left alone.
        fs::create_dir_all(build_dir(&root, other)).unwrap();
        assert!(install_archive(&good, other, &root).unwrap_err().contains("left it alone"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn remove_is_only_for_fetched_builds_no_game_uses() {
        let d = dir("remove");
        let root = steam(&d);
        let folder = d.join("turbinehub");
        let tag = "GE-Proton10-99";
        let good = d.join("good.tar.gz");
        fs::write(&good, crate::deflate::gzip(&tar::testing::proton(tag))).unwrap();
        install_archive(&good, tag, &root).unwrap();
        // Not in Turbine Hub's list (ProtonPlus put it there, say): left alone.
        assert!(remove(&root, &folder, tag).unwrap_err().contains("didn't fetch"));
        remember(&folder, tag, true);
        assert_eq!(fetched(&root, &folder), vec![tag]);
        // Steam names it for a game, and for all games.
        let mapping = |inner: &str| format!("\"InstallConfigStore\"\n{{\n\"Software\"\n{{\n\"Valve\"\n{{\n\"Steam\"\n{{\n\"CompatToolMapping\"\n{{\n{inner}\n}}\n}}\n}}\n}}\n}}\n");
        fs::write(tool::config_file(&root), mapping("\"1245620\"\n{\n\"name\" \"GE-Proton10-99\"\n}")).unwrap();
        assert_eq!(used_by(&root, &folder, tag), vec!["1245620"]);
        assert_eq!(remove(&root, &folder, tag).unwrap_err(), "1 game is set to use it. Pick another Proton there first.");
        fs::write(tool::config_file(&root), mapping("\"0\"\n{\n\"name\" \"GE-Proton10-99\"\n}")).unwrap();
        assert!(remove(&root, &folder, tag).unwrap_err().starts_with("Steam uses it for all games"));
        fs::write(tool::config_file(&root), mapping("\"1245620\"\n{\n\"name\" \"proton_experimental\"\n}")).unwrap();
        // A pick in Turbine Hub names it.
        tool::set_proton_pick(&folder, "1086940", Some(&build_dir(&root, tag))).unwrap();
        assert_eq!(used_by(&root, &folder, tag), vec!["1086940"]);
        assert!(remove(&root, &folder, tag).is_err() && installed(&root, tag));
        tool::set_proton_pick(&folder, "1086940", None).unwrap();
        assert_eq!(used_by(&root, &folder, tag), Vec::<String>::new());
        remove(&root, &folder, tag).unwrap();
        assert!(!build_dir(&root, tag).exists() && fetched(&root, &folder).is_empty());
        assert!(remove(&root, &folder, tag).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
