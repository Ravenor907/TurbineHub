//! Self-update, offline. tests/data/github_releases.json is a recorded answer from GitHub's API for this
//! project's releases (one pre-release, 0.1, with GitHub's SHA-256 digest on the bundle).

mod support;

use std::cmp::Ordering::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use support::{data_text as data, scratch};
use turbinehub_engine::net::{Client, NetError};
use turbinehub_engine::selfupdate::{self as su, Check};

#[test]
fn one_dot_versions_compare_as_decimals() {
    assert_eq!(su::compare("0.3", "0.31"), Some(Less));
    assert_eq!(su::compare("0.31", "0.4"), Some(Less));
    assert_eq!(su::compare("0.4", "1.0"), Some(Less));
    assert_eq!(su::compare("0.31", "0.3"), Some(Greater));
    assert_eq!(su::compare("0.30", "0.3"), Some(Equal));
    assert_eq!(su::compare("v1.0", "1"), Some(Equal));
    assert_eq!(su::compare("0.9", "0.10"), Some(Greater), "0.9 is above 0.10 read as decimals");
    assert_eq!(su::compare("2.0", "10.0"), Some(Less));
    assert_eq!(su::compare("0.2.0", "0.3"), None, "two dots isn't a Turbine Hub version");
    assert_eq!(su::compare("0.3-beta", "0.3"), None);
    assert_eq!(su::compare("", "0.3"), None);
    assert!(su::is_newer("0.31", "0.3"));
    assert!(!su::is_newer("0.3", "0.3"));
    assert!(!su::is_newer("junk", "0.3"));
}

#[test]
fn reads_the_recorded_release_list() {
    let list = data("github_releases.json");
    // Without Labs mode the 0.1 pre-release doesn't count.
    assert_eq!(su::pick_release(&list, false).unwrap(), None);
    let r = su::pick_release(&list, true).unwrap().unwrap();
    assert_eq!(r.version, "0.1");
    assert_eq!(r.tag, "v0.1");
    assert!(r.prerelease);
    assert_eq!(r.asset_size, 705304);
    assert_eq!(r.asset_url, "https://github.com/Ravenor907/TurbineHub/releases/download/v0.1/TurbineHub.flatpak");
    assert_eq!(r.sha256.as_deref(), Some("f40cccd57658730bc0732a299f2d3c38b312ee8f9634bd6d303c8effc7106de7"));
    assert_eq!(su::decide(&list, "0.1", true).unwrap(), Check::UpToDate);
    assert!(matches!(su::decide(&list, "0.05", true).unwrap(), Check::Available(r) if r.version == "0.1"));
    assert!(su::pick_release("{]", true).is_err());
}

fn release(tag: &str, pre: bool, extra_assets: &str, body: &str) -> String {
    format!(
        "{{\"tag_name\":\"{tag}\",\"name\":\"Turbine Hub {tag}\",\"draft\":false,\"prerelease\":{pre},\"html_url\":\"https://github.com/Ravenor907/TurbineHub/releases/tag/{tag}\",\"body\":{},\"assets\":[{{\"name\":\"TurbineHub.flatpak\",\"size\":10,\"browser_download_url\":\"https://github.com/Ravenor907/TurbineHub/releases/download/{tag}/TurbineHub.flatpak\"}}{extra_assets}]}}",
        turbinehub_engine::json::quote(body)
    )
}

#[test]
fn picks_the_newest_and_finds_hashes() {
    let hash = "a".repeat(64);
    let list = format!(
        "[{},{},{},{}]",
        release("v0.4", true, "", ""),
        release("v0.31", false, "", &format!("SHA-256 of TurbineHub.flatpak: {hash}")),
        release("v0.3", false, "", ""),
        release("v0.3.1", false, "", "")
    );
    let stable = su::pick_release(&list, false).unwrap().unwrap();
    assert_eq!(stable.version, "0.31");
    assert_eq!(stable.sha256.as_deref(), Some(hash.as_str()));
    assert_eq!(su::pick_release(&list, true).unwrap().unwrap().version, "0.4");
    // A draft never counts, nor a release without the bundle.
    let draft = release("v9.0", false, "", "").replace("\"draft\":false", "\"draft\":true");
    let nobundle = release("v8.0", false, "", "").replace("TurbineHub.flatpak\",\"size", "Other.zip\",\"size");
    let list = format!("[{draft},{nobundle},{}]", release("v0.5", false, "", ""));
    assert_eq!(su::pick_release(&list, false).unwrap().unwrap().version, "0.5");
    // The "latest" endpoint answers with one object.
    assert_eq!(su::pick_release(&release("v1.0", false, "", ""), false).unwrap().unwrap().version, "1.0");

    assert_eq!(su::sha256_from_notes(&format!("Changes\n- stuff\n\nsha256: {}", hash.to_uppercase())), Some(hash.clone()));
    assert_eq!(su::sha256_from_notes(&hash), Some(hash.clone()));
    assert_eq!(su::sha256_from_notes(&format!("{hash}\n{}", "b".repeat(64))), None, "two loose hashes: unclear");
    assert_eq!(su::sha256_from_notes("no hash"), None);
    assert_eq!(su::parse_sha256_file(&format!("{hash}  TurbineHub.flatpak\n")), Some(hash.clone()));
    assert_eq!(su::parse_sha256_file("nope"), None);

    let with_asset = release("v0.6", false, ",{\"name\":\"TurbineHub.flatpak.sha256\",\"size\":90,\"browser_download_url\":\"https://github.com/Ravenor907/TurbineHub/releases/download/v0.6/TurbineHub.flatpak.sha256\"}", "");
    let r = su::pick_release(&with_asset, false).unwrap().unwrap();
    assert_eq!(r.sha256, None);
    assert!(r.sha256_asset_url.unwrap().ends_with(".flatpak.sha256"));
}

#[test]
fn install_arguments_are_fixed() {
    assert_eq!(
        su::install_args(Path::new("/home/deck/.var/app/io.github.ravenor907.TurbineHub/cache/u.flatpak")),
        vec![
            "flatpak-spawn",
            "--host",
            "flatpak",
            "install",
            "--user",
            "-y",
            "--reinstall",
            "--bundle",
            "/home/deck/.var/app/io.github.ravenor907.TurbineHub/cache/u.flatpak"
        ]
    );
}

// ---- Through a stand-in curl ----

const FAKE_CURL: &str = r#"#!/bin/sh
SERVED='@SERVED@'
out=; hdr=; url=
while [ $# -gt 0 ]; do
  case "$1" in
    --output) out=$2; shift;;
    --dump-header) hdr=$2; shift;;
    --) url=$2; shift;;
  esac
  shift
done
key=$(printf '%s' "$url" | tr -c 'A-Za-z0-9' '_')
printf '%s\n' "$url" >> "$SERVED/requests.log"
if [ -f "$SERVED/$key.status" ]; then
  { printf 'HTTP/2 %s\r\n' "$(cat "$SERVED/$key.status")"; cat "$SERVED/$key.headers" 2>/dev/null; printf '\r\n'; } > "$hdr"
  printf '%s %s' "$(cat "$SERVED/$key.status")" "$url"; exit 22
fi
if [ ! -f "$SERVED/$key" ]; then printf 'HTTP/2 404\r\n\r\n' > "$hdr"; printf '404 %s' "$url"; exit 22; fi
printf 'HTTP/2 200\r\n\r\n' > "$hdr"
cp "$SERVED/$key" "$out"
eff=$url
if [ -f "$SERVED/$key.effective" ]; then eff=$(cat "$SERVED/$key.effective"); fi
printf '200 %s' "$eff"
"#;

fn fake(tag: &str) -> (PathBuf, Client) {
    let dir = scratch(tag);
    fs::create_dir_all(dir.join("served")).unwrap();
    let script = dir.join("curl");
    fs::write(&script, FAKE_CURL.replace("@SERVED@", &dir.join("served").display().to_string())).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let mut c = Client::new("0.3", &dir.join("cache"));
    c.program = script;
    (dir, c)
}

fn serve(dir: &Path, url: &str, body: &[u8]) {
    let key: String = url.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    fs::write(dir.join("served").join(key), body).unwrap();
}

fn extra(dir: &Path, url: &str, ext: &str, text: &str) {
    let key: String = url.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    fs::write(dir.join("served").join(format!("{key}.{ext}")), text).unwrap();
}

fn requests(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("served/requests.log")).unwrap_or_default().lines().map(str::to_string).collect()
}

const TAG: &str = "https://github.com/Ravenor907/TurbineHub/releases/tag/";
const DL: &str = "https://github.com/Ravenor907/TurbineHub/releases/download/";

#[test]
fn checks_downloads_and_verifies() {
    let (dir, client) = fake("flow");
    // Only pre-releases so far: the latest page answers 404, which means up to date.
    assert_eq!(su::check(&client, "0.1", false).unwrap(), Check::UpToDate);
    // The newest release is read from where the latest page ends up, never from GitHub's API.
    serve(&dir, su::LATEST_PAGE, b"<html>the release page</html>");
    extra(&dir, su::LATEST_PAGE, "effective", &format!("{TAG}v0.31"));
    let Check::Available(r) = su::check(&client, "0.3", false).unwrap() else { panic!("0.31 is newer than 0.3") };
    assert_eq!((r.version.as_str(), r.tag.as_str(), r.prerelease), ("0.31", "v0.31", false));
    assert_eq!(r.asset_url, format!("{DL}v0.31/TurbineHub.flatpak"));
    assert_eq!(su::check(&client, "0.31", false).unwrap(), Check::UpToDate);
    assert_eq!(fs::read_dir(dir.join("cache")).unwrap().count(), 0, "the page isn't kept");
    // Labs mode: the feed's tags count too, and one that isn't the latest full release is a pre-release.
    serve(&dir, su::FEED_URL, format!("<feed><entry><link rel=\"alternate\" href=\"{TAG}v0.32\"/></entry><entry><link href=\"{TAG}v0.31\"/></entry><entry><link href=\"{TAG}nightly\"/></entry></feed>").as_bytes());
    let Check::Available(pre) = su::check(&client, "0.31", true).unwrap() else { panic!("0.32 is newer than 0.31") };
    assert_eq!((pre.version.as_str(), pre.prerelease), ("0.32", true));
    assert!(requests(&dir).iter().all(|u| !u.contains("api.github.com")), "{:?}", requests(&dir));

    // The checksum file beside the bundle is what the download is checked against.
    let bundle = b"0123456789".to_vec();
    let sha = turbinehub_engine::sha256::digest_hex(&bundle);
    let sha_url = format!("{DL}v0.31/TurbineHub.flatpak.sha256");
    assert_eq!(r.sha256_asset_url.as_deref(), Some(sha_url.as_str()));
    serve(&dir, &sha_url, format!("{sha}  TurbineHub.flatpak\n").as_bytes());
    serve(&dir, &r.asset_url, &bundle);
    let d = su::download(&client, &r, None).unwrap();
    assert_eq!(fs::read(&d.path).unwrap(), bundle);
    assert!(d.path.starts_with(dir.join("cache")));
    assert!(requests(&dir).iter().all(|u| !u.contains("api.github.com")));

    // A bundle that doesn't match is deleted.
    serve(&dir, &r.asset_url, b"9876543210");
    assert_eq!(su::download(&client, &r, None), Err(NetError::Mismatch("its SHA-256 is different".into())));
    // No checksum file (an older release): GitHub's own digest and size for that tag's bundle.
    fs::remove_file(dir.join("served").join(sha_url.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect::<String>())).unwrap();
    let api = format!("{}v0.31", su::TAG_API);
    serve(&dir, &api, release("v0.31", false, "", &format!("sha256 {sha}")).as_bytes());
    serve(&dir, &r.asset_url, &bundle);
    assert!(su::download(&client, &r, None).is_ok());
    serve(&dir, &r.asset_url, b"01234");
    assert!(matches!(su::download(&client, &r, None), Err(NetError::Mismatch(why)) if why.starts_with("size")));
    // No published hash at all: refused before downloading.
    fs::remove_file(dir.join("served").join(api.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect::<String>())).unwrap();
    assert!(su::download(&client, &r, None).unwrap_err().to_string().contains("doesn't publish a SHA-256"));
    let mut bare = (*r).clone();
    bare.sha256_asset_url = None;
    bare.tag = String::new();
    assert!(su::download(&client, &bare, None).unwrap_err().to_string().contains("doesn't publish a SHA-256"));
}

#[test]
fn a_limit_from_github_has_its_own_words() {
    let (dir, client) = fake("limit");
    serve(&dir, su::LATEST_PAGE, b"");
    extra(&dir, su::LATEST_PAGE, "status", "429");
    extra(&dir, su::LATEST_PAGE, "headers", "Retry-After: 700\r\n");
    let e = su::check(&client, "0.3", false).unwrap_err();
    assert_eq!(e.to_string(), "GitHub is limiting update checks right now. Try again in about 12 minutes.");
    extra(&dir, su::LATEST_PAGE, "status", "403");
    extra(&dir, su::LATEST_PAGE, "headers", "X-RateLimit-Remaining: 0\r\n");
    let e = su::check(&client, "0.3", false).unwrap_err().to_string();
    assert!(e.starts_with("GitHub is limiting update checks right now. Try again in about ") && !e.contains("account") && !e.contains("key"), "{e}");
}

#[test]
fn a_start_within_six_hours_uses_the_last_answer() {
    let newer = su::release_for_tag("v0.64", false).unwrap();
    let kept = su::Remembered::of(Some(&newer), false, 1_000_000);
    assert_eq!(su::Remembered::parse(&kept.to_text()), Some(kept.clone()));
    assert!(kept.fresh(false, 1_000_000 + su::CHECK_EVERY_SECS - 1));
    assert!(!kept.fresh(false, 1_000_000 + su::CHECK_EVERY_SECS), "six hours on, GitHub is asked again");
    assert!(!kept.fresh(true, 1_000_100), "another channel asks again");
    assert!(!kept.fresh(false, 999_999), "an answer from the future isn't trusted");
    assert!(matches!(kept.answer("0.63"), Check::Available(r) if r.version == "0.64"));
    assert_eq!(kept.answer("0.64"), Check::UpToDate, "after the update the remembered release is the running one");
    let none = su::Remembered::of(None, true, 5);
    assert_eq!(su::Remembered::parse(&none.to_text()), Some(none.clone()));
    assert_eq!(none.answer("0.1"), Check::UpToDate);
    assert_eq!(su::Remembered::parse("junk"), None);
    assert_eq!(su::Remembered::parse("5 0 0 v1.2.3"), None, "a tag that isn't a Turbine Hub version");
    assert_eq!(su::tag_from_url("https://github.com/Ravenor907/TurbineHub/releases"), None);
    assert_eq!(su::tag_from_url("https://github.com/Someone/Else/releases/tag/v9.9"), None);
}

#[test]
fn the_update_goes_where_turbine_is_installed() {
    let system = "[Application]\nname=io.github.ravenor907.TurbineHub\n\n[Instance]\ninstance-id=1\napp-path=/var/lib/flatpak/app/io.github.ravenor907.TurbineHub/x86_64/master/abc/files\n";
    let user = "[Instance]\napp-path=/home/deck/.local/share/flatpak/app/io.github.ravenor907.TurbineHub/x86_64/master/abc/files\n";
    assert_eq!(su::install_scope(system), "--system");
    assert_eq!(su::install_scope(user), "--user");
    assert_eq!(su::install_scope(""), "--user");
    // app-path outside [Instance] doesn't count.
    assert_eq!(su::install_scope("[Application]\napp-path=/var/lib/flatpak/app/x\n"), "--user");
    let a = su::install_args_to(Path::new("/tmp/u.flatpak"), "--system");
    assert_eq!(a[4], "--system");
    assert_eq!(su::install_args_to(Path::new("/tmp/u.flatpak"), "anything")[4], "--user");
}
