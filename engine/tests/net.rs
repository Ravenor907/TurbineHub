//! Network plumbing, offline: SHA-256 against NIST's published vectors, the curl arguments, error wording, and the
//! whole request path run against a stand-in curl script that serves files from a folder. Also Heroic's cover art,
//! which is found by the SHA-256 of the image URL.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use support::scratch;
use turbinehub_engine::net::{self, Body, Client, Expect, NetError, Request};
use turbinehub_engine::sha256::{self, Sha256};

/// A stand-in for curl: serves `served/<url as a file name>`, with optional .status, .headers, .effective and .exit
/// files next to it, and logs every URL it was asked for.
struct Fake {
    dir: PathBuf,
    client: Client,
}

const FAKE_CURL: &str = r#"#!/bin/sh
SERVED='@SERVED@'
out=; hdr=; url=; data=
while [ $# -gt 0 ]; do
  case "$1" in
    --output) out=$2; shift;;
    --dump-header) hdr=$2; shift;;
    --data-binary) data=$2; shift;;
    --) url=$2; shift;;
  esac
  shift
done
printf '%s\n' "$url" >> "$SERVED/requests.log"
if [ -n "$data" ]; then cp "${data#@}" "$SERVED/last_post"; fi
key=$(printf '%s' "$url" | tr -c 'A-Za-z0-9' '_')
eff=$url
if [ -f "$SERVED/$key.effective" ]; then eff=$(cat "$SERVED/$key.effective"); fi
if [ -f "$SERVED/$key.exit" ]; then printf '000 %s' "$eff"; exit "$(cat "$SERVED/$key.exit")"; fi
if [ ! -f "$SERVED/$key" ]; then printf 'HTTP/2 404\r\n\r\n' > "$hdr"; printf '404 %s' "$eff"; exit 22; fi
status=200
if [ -f "$SERVED/$key.status" ]; then status=$(cat "$SERVED/$key.status"); fi
{ printf 'HTTP/2 %s\r\n' "$status"; if [ -f "$SERVED/$key.headers" ]; then cat "$SERVED/$key.headers"; fi; printf '\r\n'; } > "$hdr"
if [ "$status" -ge 400 ]; then printf '%s %s' "$status" "$eff"; exit 22; fi
cp "$SERVED/$key" "$out"
printf '%s %s' "$status" "$eff"
"#;

fn key(url: &str) -> String {
    url.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

impl Fake {
    fn new(tag: &str) -> Fake {
        let dir = scratch(tag);
        let served = dir.join("served");
        fs::create_dir_all(&served).unwrap();
        let script = dir.join("curl");
        fs::write(&script, FAKE_CURL.replace("@SERVED@", &served.display().to_string())).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let mut client = Client::new("0.3", &dir.join("tmp"));
        client.program = script;
        Fake { dir, client }
    }
    fn serve(&self, url: &str, body: &[u8]) {
        fs::write(self.dir.join("served").join(key(url)), body).unwrap();
    }
    fn extra(&self, url: &str, ext: &str, text: &str) {
        fs::write(self.dir.join("served").join(format!("{}.{ext}", key(url))), text).unwrap();
    }
    fn requests(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("served/requests.log")).unwrap_or_default().lines().map(str::to_string).collect()
    }
    fn temp_files(&self) -> usize {
        fs::read_dir(self.dir.join("tmp")).map(|r| r.count()).unwrap_or(0)
    }
}

// ---- SHA-256 ----

#[test]
fn sha256_nist_vectors() {
    // FIPS 180-2 / NIST CSRC example values.
    assert_eq!(sha256::digest_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(sha256::digest_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(
        sha256::digest_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    assert_eq!(
        sha256::digest_hex(
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
        ),
        "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
    );
    let mut h = Sha256::new();
    let block = [b'a'; 1000];
    for _ in 0..1000 {
        h.update(&block);
    }
    assert_eq!(sha256::hex(&h.finish()), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
}

#[test]
fn sha256_pieces_match_one_shot() {
    let data: Vec<u8> = (0..5000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
    let whole = sha256::digest_hex(&data);
    for split in [0, 1, 55, 56, 63, 64, 65, 127, 128, 129, 4999] {
        let mut h = Sha256::new();
        h.update(&data[..split]);
        h.update(&data[split..]);
        assert_eq!(sha256::hex(&h.finish()), whole, "split at {split}");
    }
    // Every length around the padding boundaries.
    for len in 50..140 {
        let mut a = Sha256::new();
        for b in &data[..len] {
            a.update(std::slice::from_ref(b));
        }
        assert_eq!(sha256::hex(&a.finish()), sha256::digest_hex(&data[..len]));
    }
    let dir = scratch("shafile");
    let f = dir.join("x.bin");
    fs::write(&f, &data).unwrap();
    assert_eq!(sha256::file_hex(&f).unwrap(), whole);
    assert!(sha256::same_digest(&whole.to_uppercase(), &whole));
    assert!(!sha256::same_digest("abc", "abc"));
}

// ---- Pure parts ----

#[test]
fn curl_gets_fixed_safe_arguments() {
    let req = Request::get("https://store.steampowered.com/x").header("Accept", "application/json").max_bytes(1234);
    let a = net::curl_args(&req, "TurbineHub/0.3", Path::new("/t/out"), Path::new("/t/hdr"));
    let has = |pair: [&str; 2]| a.windows(2).any(|w| w[0] == pair[0] && w[1] == pair[1]);
    assert!(a.contains(&"--fail".to_string()));
    assert!(a.contains(&"--location".to_string()));
    assert!(has(["--proto", "=https"]));
    assert!(has(["--proto-redir", "=https"]));
    assert!(has(["--max-filesize", "1234"]));
    assert!(has(["--user-agent", "TurbineHub/0.3"]));
    assert!(has(["--header", "Accept: application/json"]));
    assert!(has(["--output", "/t/out"]));
    assert!(has(["--max-time", "60"]));
    assert_eq!(&a[a.len() - 2..], &["--".to_string(), "https://store.steampowered.com/x".to_string()]);
    assert!(!a.iter().any(|x| x == "--data-binary"));

    let post = Request::get("https://store.steampowered.com/y").post_file(Path::new("/t/body"), "application/octet-stream").max_time(0);
    assert_eq!(post.body, Body::File("/t/body".into(), "application/octet-stream".into()));
    let a = net::curl_args(&post, "UA", Path::new("/o"), Path::new("/h"));
    assert!(a.windows(2).any(|w| w[0] == "--data-binary" && w[1] == "@/t/body"));
    assert!(a.windows(2).any(|w| w[0] == "--header" && w[1] == "Content-Type: application/octet-stream"));
    assert!(!a.iter().any(|x| x == "--max-time"), "big downloads have no total time limit");
    assert!(a.windows(2).any(|w| w[0] == "--speed-time"), "but a stalled one still ends");
}

#[test]
fn only_https_from_known_hosts() {
    for ok in [
        "https://store.steampowered.com/saleaction/ajaxgetdeckappcompatibilityreport?nAppID=1086940",
        "https://plugins.deckbrew.xyz/plugins",
        "https://cdn.tzatzikiweeb.moe/file/steam-deck-homebrew/versions/x.zip",
        "https://dl.flathub.org/repo/appstream/x86_64/icons/128x128/com.discordapp.Discord.png",
        "https://api.github.com/repos/a/b",
        "https://objects.githubusercontent.com/x",
        "https://release-assets.githubusercontent.com/github-production-release-asset/1/2",
        "https://raw.githubusercontent.com/AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json",
        "https://github.com/Ravenor907/TurbineHub/releases/download/v0.3/x.flatpak",
        "https://codeload.github.com/a/b/zip/refs/heads/main",
        "HTTPS://Store.SteamPowered.COM/x",
        "https://store.steampowered.com:443/x",
    ] {
        assert!(net::allowed_url(ok), "{ok}");
    }
    for bad in [
        "http://store.steampowered.com/x",
        "https://evil.example.com/",
        "https://store.steampowered.com.evil.com/",
        "https://evilstore.steampowered.com/",
        "https://store.steampowered.com@evil.com/",
        "https://.flathub.org/",
        "file:///etc/passwd",
        "https://",
        "ftp://store.steampowered.com/",
        "https://store steampowered.com/",
        "https://evil.githubusercontent.com/x",
        "https://someone.github.io/x",
        "https://githubusercontent.com/x",
        "https://flathub.org.evil.com/x",
        "https://evilflathub.org/x",
        "https://thunderstore.io/x",
        "https://evil-github.com/x",
    ] {
        assert!(!net::allowed_url(bad), "{bad}");
    }
    assert_eq!(net::https_host("https://user@api.github.com:8443/x?y").as_deref(), Some("api.github.com"));
    // A whole-domain entry is never put on a domain where anyone can have a subdomain, and every entry is lowercase.
    for a in net::ALLOWED_HOSTS {
        assert_eq!(*a, a.to_ascii_lowercase());
        if let Some(base) = a.strip_prefix('.') {
            assert!(base.contains('.'), "{a} is a whole top-level domain");
            for shared in net::SHARED_DOMAINS {
                assert!(base != *shared && !shared.ends_with(&format!(".{base}")), "{a} would let in {shared}");
            }
        }
    }
    assert_eq!(net::url_encode("a b/c&d"), "a%20b%2Fc%26d");
}

#[test]
fn errors_are_plain_sentences() {
    use NetError::*;
    assert_eq!(net::classify(0, 200, 10), Ok(()));
    assert_eq!(net::classify(22, 404, 10), Err(NotFound));
    assert_eq!(net::classify(22, 403, 10), Err(Denied(403)));
    assert_eq!(net::classify(22, 429, 10), Err(RateLimited));
    assert_eq!(net::classify(22, 500, 10), Err(Http(500)));
    assert_eq!(net::classify(6, 0, 10), Err(Offline));
    assert_eq!(net::classify(7, 0, 10), Err(Offline));
    assert_eq!(net::classify(28, 0, 10), Err(Timeout));
    assert_eq!(net::classify(63, 0, 10), Err(TooBig { limit: 10 }));
    assert!(matches!(net::classify(60, 0, 10), Err(Failed(_))));
    let all = [
        Offline,
        NotFound,
        Denied(401),
        RateLimited,
        TooBig { limit: 5 << 20 },
        Timeout,
        NotAllowed("evil.com".into()),
        Mismatch("x".into()),
        Http(502),
        Cancelled,
    ];
    for e in all {
        let s = e.to_string();
        assert!(s.ends_with('.'), "{s}");
        assert!(s.chars().next().unwrap().is_uppercase(), "{s}");
        assert!(!s.contains('\u{2014}') && !s.contains('\u{2013}'), "{s}");
    }
    assert_eq!(Offline.to_string(), "No internet connection. Check Wi-Fi and try again.");
    assert!(TooBig { limit: 5 << 20 }.to_string().contains("5.0 MB"));
    assert_eq!(net::human_size(512), "512 bytes");
    assert_eq!(net::human_size(3 << 30), "3.0 GB");
}

#[test]
fn headers_of_the_last_answer_count() {
    let text = "HTTP/1.1 302 Found\r\nLocation: https://x\r\nX-A: 1\r\n\r\nHTTP/2 200\r\ncontent-length: 42\r\nX-RL-Hourly-Remaining: 99\r\n\r\n";
    let (status, h) = net::parse_headers(text);
    assert_eq!(status, 200);
    assert_eq!(h, vec![("content-length".into(), "42".into()), ("x-rl-hourly-remaining".into(), "99".into())]);
}

#[test]
fn user_agent_names_the_version() {
    assert_eq!(Client::new("0.3", Path::new("/tmp")).user_agent, "TurbineHub/0.3");
}

// ---- The request path through the stand-in curl ----

#[test]
fn gets_text_and_headers() {
    let f = Fake::new("get");
    f.serve("https://api.github.com/ok", b"{\"a\":1}");
    f.extra("https://api.github.com/ok", "headers", "X-RL-Daily-Remaining: 7\r\nContent-Length: 7\r\n");
    let resp = f.client.send(&Request::get("https://api.github.com/ok"), None).unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.header("x-rl-daily-remaining"), Some("7"));
    assert_eq!(resp.effective_url, "https://api.github.com/ok");
    assert_eq!(resp.take_text().unwrap(), "{\"a\":1}");
    assert_eq!(f.temp_files(), 0, "temp files are cleaned up");
    assert_eq!(f.client.get_text("https://api.github.com/ok", &[("A", "b")]).unwrap(), "{\"a\":1}");
}

#[test]
fn failures_become_net_errors_and_leave_no_files() {
    let f = Fake::new("fail");
    assert_eq!(f.client.get_text("https://api.github.com/missing", &[]), Err(NetError::NotFound));
    let u = "https://api.github.com/limited";
    f.serve(u, b"slow down");
    f.extra(u, "status", "429");
    // A limit or refusal from GitHub is its own failure, with the minutes it says to wait.
    f.extra(u, "headers", "Retry-After: 61\r\n");
    assert_eq!(f.client.get_text(u, &[]), Err(NetError::GitHubLimit(2)));
    assert_eq!(NetError::GitHubLimit(2).to_string(), "GitHub is limiting requests right now. Try again in about 2 minutes.");
    assert_eq!(NetError::GitHubLimit(1).to_string(), "GitHub is limiting requests right now. Try again in about 1 minute.");
    f.extra(u, "status", "403");
    f.extra(u, "headers", "X-RateLimit-Remaining: 0\r\n");
    assert_eq!(f.client.get_text(u, &[]), Err(NetError::GitHubLimit(15)));
    let h = |k: &str, v: &str| vec![(k.to_string(), v.to_string())];
    assert_eq!(net::github_limit(&NetError::Denied(403), "https://api.github.com/x", &h("x-ratelimit-reset", "1600"), 1000), Some(10));
    assert_eq!(net::github_limit(&NetError::Denied(403), "https://api.github.com/x", &h("x-ratelimit-reset", "900000"), 1000), Some(60));
    assert_eq!(net::github_limit(&NetError::Denied(403), "https://store.steampowered.com/x", &[], 1000), None);
    assert_eq!(net::github_limit(&NetError::NotFound, "https://github.com/x", &[], 1000), None);
    let other = "https://store.steampowered.com/limited";
    f.serve(other, b"slow down");
    f.extra(other, "status", "429");
    assert_eq!(f.client.get_text(other, &[]), Err(NetError::RateLimited));
    f.extra(other, "status", "403");
    let refused = f.client.get_text(other, &[]).unwrap_err().to_string();
    assert!(!refused.contains("account") && !refused.contains("key"), "{refused}");
    let u = "https://api.github.com/offline";
    f.extra(u, "exit", "6");
    assert_eq!(f.client.get_text(u, &[]), Err(NetError::Offline));
    let u = "https://api.github.com/slow";
    f.extra(u, "exit", "28");
    assert_eq!(f.client.get_text(u, &[]), Err(NetError::Timeout));
    let u = "https://api.github.com/huge";
    f.extra(u, "exit", "63");
    assert_eq!(f.client.get_text(u, &[]), Err(NetError::TooBig { limit: 32 << 20 }));
    // A redirect that ends on a host Turbine Hub doesn't know is refused and its file deleted.
    let u = "https://github.com/redirected";
    f.serve(u, b"payload");
    f.extra(u, "effective", "https://evil.example.com/payload");
    assert_eq!(f.client.get_text(u, &[]), Err(NetError::NotAllowed("evil.example.com".into())));
    // A host that isn't allowed is never asked.
    let before = f.requests().len();
    assert!(matches!(f.client.get_text("https://evil.example.com/x", &[]), Err(NetError::NotAllowed(_))));
    assert!(matches!(f.client.get_text("http://store.steampowered.com/x", &[]), Err(NetError::NotAllowed(_))));
    assert_eq!(f.requests().len(), before);
    assert_eq!(f.temp_files(), 0);
}

#[test]
fn bodies_over_the_cap_are_stopped() {
    let f = Fake::new("cap");
    let u = "https://api.github.com/big";
    f.serve(u, &vec![b'x'; 5000]);
    let r = f.client.send(&Request::get(u).max_bytes(4000), None);
    assert_eq!(r, Err(NetError::TooBig { limit: 4000 }));
    assert_eq!(f.temp_files(), 0);
}

#[test]
fn downloads_are_checked() {
    let f = Fake::new("dl");
    let u = "https://dl.flathub.org/file.zip";
    let body = b"PK\x03\x04 pretend zip".to_vec();
    f.serve(u, &body);
    let sha = sha256::digest_hex(&body);
    let mut calls = Vec::new();
    let mut progress = |got: u64, total: Option<u64>| {
        calls.push((got, total));
        true
    };
    let d = f
        .client
        .download(u, &Expect { max_bytes: 1000, size: Some(body.len() as u64), sha256: Some(sha.to_uppercase()) }, Some(&mut progress))
        .unwrap();
    assert_eq!(fs::read(&d.path).unwrap(), body);
    assert_eq!(d.sha256, sha);
    assert_eq!(calls.last(), Some(&(body.len() as u64, Some(body.len() as u64))));
    fs::remove_file(&d.path).unwrap();

    let wrong_size = f.client.download(u, &Expect { max_bytes: 1000, size: Some(3), sha256: None }, None);
    assert!(matches!(wrong_size, Err(NetError::Mismatch(_))));
    let wrong_hash = f.client.download(u, &Expect { max_bytes: 1000, size: None, sha256: Some("0".repeat(64)) }, None);
    assert_eq!(wrong_hash, Err(NetError::Mismatch("its SHA-256 is different".into())));
    assert!(wrong_hash.unwrap_err().to_string().contains("deleted"));
    assert_eq!(f.temp_files(), 0, "a bad download is deleted");

    let mut stop = |_: u64, _: Option<u64>| false;
    assert_eq!(f.client.download(u, &Expect::up_to(1000), Some(&mut stop)), Err(NetError::Cancelled));
    assert_eq!(f.temp_files(), 0);
}

#[test]
fn posts_send_the_file() {
    let f = Fake::new("post");
    let u = "https://store.steampowered.com/upload/";
    f.serve(u, b"{\"key\":\"k\"}");
    let body = f.dir.join("body.txt");
    fs::write(&body, "hello upload").unwrap();
    let r = f.client.send(&Request::get(u).post_file(&body, "application/octet-stream"), None).unwrap();
    assert_eq!(r.take_text().unwrap(), "{\"key\":\"k\"}");
    assert_eq!(fs::read_to_string(f.dir.join("served/last_post")).unwrap(), "hello upload");
}

#[test]
fn missing_curl_is_explained() {
    let mut c = Client::new("0.3", &scratch("nocurl"));
    c.program = "/nonexistent/curl".into();
    let e = c.get_text("https://api.github.com/x", &[]).unwrap_err();
    assert_eq!(e.to_string(), "Couldn't find curl, which Turbine Hub uses to download.");
}

// ---- Heroic cover art (found by the SHA-256 of the image URL) ----

#[test]
fn heroic_cover_art_comes_from_its_image_cache() {
    use turbinehub_engine::games::Store;
    use turbinehub_engine::heroic;
    let home = scratch("heroic-art");
    let root = home.join(".config/heroic");
    fs::create_dir_all(root.join("legendaryConfig/legendary")).unwrap();
    fs::create_dir_all(root.join("store_cache")).unwrap();
    fs::create_dir_all(root.join("images-cache")).unwrap();
    let game_dir = home.join("Games/Fortnite");
    fs::create_dir_all(&game_dir).unwrap();
    fs::write(
        root.join("legendaryConfig/legendary/installed.json"),
        format!("{{\"Fortnite\":{{\"title\":\"Fortnite\",\"install_path\":{},\"platform\":\"Windows\"}}}}", turbinehub_engine::json::quote(&game_dir.to_string_lossy())),
    )
    .unwrap();
    let lib = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/heroic_legendary_library.json");
    fs::copy(&lib, root.join("store_cache/legendary_library.json")).unwrap();

    // Without cached files there is no art.
    let scan = turbinehub_engine::scan_games(&home);
    let g = scan.games.iter().find(|g| g.store_id == "Fortnite").unwrap();
    assert_eq!(g.cover, None);

    // Heroic's card asks for the square art with its Epic size suffix; the file is named by that URL's SHA-256.
    let square = "https://cdn1.epicgames.com/offer/fn/square.jpg?h=400&resize=1&w=300";
    let wide = "https://cdn1.epicgames.com/offer/fn/wide.jpg";
    let tall_file = root.join("images-cache").join(sha256::digest_hex(square.as_bytes()));
    fs::write(&tall_file, b"jpeg").unwrap();
    fs::write(root.join("images-cache").join(sha256::digest_hex(wide.as_bytes())), b"jpeg").unwrap();
    assert_eq!(heroic::image_cache_file(&root, square), tall_file);
    let scan = turbinehub_engine::scan_games(&home);
    let g = scan.games.iter().find(|g| g.store_id == "Fortnite").unwrap();
    assert_eq!(g.cover.as_deref(), Some(tall_file.as_path()));
    assert!(g.header.as_ref().unwrap().ends_with(sha256::digest_hex(wide.as_bytes())));
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);

    // GOG and hand-added games use art_square as is.
    let (t, _) = heroic::cached_art(&root, Store::Gog, "https://cdn1.epicgames.com/offer/fn/square.jpg", "");
    assert_eq!(t, None);
    fs::write(root.join("images-cache").join(sha256::digest_hex(b"https://images.gog.com/a.jpg")), b"x").unwrap();
    let (t, w) = heroic::cached_art(&root, Store::Gog, "https://images.gog.com/a.jpg", "fallback");
    assert!(t.is_some());
    assert_eq!(w, None);
    let art = heroic::library_art(&root);
    assert_eq!(art.len(), 1);
    assert_eq!(art[0].0, Store::Epic);
}

#[test]
fn a_blocked_or_down_site_is_never_the_keys_fault() {
    use turbinehub_engine::net::{site_down, NetError};
    let h = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
    // A protection page standing in for the API's answer.
    assert!(site_down(&NetError::Denied(403), &h(&[("cf-mitigated", "challenge")]), b""));
    assert!(site_down(&NetError::Denied(403), &h(&[("content-type", "text/html; charset=UTF-8"), ("cf-ray", "8f1")]), b"<!DOCTYPE html>"));
    assert!(site_down(&NetError::Denied(403), &h(&[("server", "cloudflare")]), b"  <html><head>"));
    // The site down.
    assert!(site_down(&NetError::Http(503), &[], b""));
    assert!(site_down(&NetError::Http(502), &h(&[("content-type", "text/html")]), b""));
    // The API's own refusal (JSON) stays the key's or the account's.
    assert!(!site_down(&NetError::Denied(401), &h(&[("content-type", "application/json"), ("cf-ray", "8f1")]), br#"{"message":"Please provide a valid API Key"}"#));
    assert!(!site_down(&NetError::Denied(403), &[], br#"{"message":"forbidden"}"#));
    assert!(!site_down(&NetError::NotFound, &h(&[("cf-mitigated", "challenge")]), b""));
    assert!(!site_down(&NetError::Http(429), &[], b""));
    assert_eq!(NetError::Down.to_string(), "The site isn't answering right now. Try again in a few minutes.");
}
