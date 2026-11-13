//! Rough input for every parser: the engine and the helper read files and answers they did not write, and a
//! damaged one must end in an error, never in a crash.

use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};
use turbinehub_engine as e;
use turbinehub_helper as h;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
    fn below(&mut self, n: usize) -> usize { if n == 0 { 0 } else { (self.next() % n as u64) as usize } }
}
const TOKENS: &[&[u8]] = &[b"{", b"}", b"[", b"]", b"\"", b"\\", b"\\u", b"\\ud800", b"\n", b"//", b":", b",", b"-", b"1e999", b"0x", b"\x00", b"\xff", b"\xff\xff\xff\xff", b"\x7f\xff\xff\xff", b"\x00\x00\x00\x00", b"[madvise", b"]", b"mV", b"OD_RANGE:", b"VDDGFX_OFFSET:", b"null", b"true", b"-0", b".", b"e", b"\xf0\x9f", b"#include", b"\t"];
fn mutate(r: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut v = seed.to_vec();
    for _ in 0..1 + r.below(6) {
        match r.below(8) {
            0 if !v.is_empty() => { let i = r.below(v.len()); v[i] ^= 1 << r.below(8); }
            1 if !v.is_empty() => { let i = r.below(v.len()); v.truncate(i); }
            2 => { let i = r.below(v.len() + 1); let t = TOKENS[r.below(TOKENS.len())]; v.splice(i..i, t.iter().copied()); }
            3 if !v.is_empty() => { let i = r.below(v.len()); v[i] = r.next() as u8; }
            4 if v.len() > 2 => { let a = r.below(v.len()); let b = a + r.below(v.len() - a); let chunk: Vec<u8> = v[a..b].to_vec(); let i = r.below(v.len()); v.splice(i..i, chunk); }
            5 if v.len() > 2 => { let a = r.below(v.len()); let b = a + r.below((v.len() - a).min(16) + 1); v.drain(a..b.min(v.len())); }
            6 if v.len() > 4 => { let i = r.below(v.len() - 4); let val: [u8; 4] = [[0xff; 4], [0, 0, 0, 0x80], [0xfe, 0xff, 0xff, 0x7f], [1, 0, 0, 0]][r.below(4)]; v[i..i + 4].copy_from_slice(&val); }
            _ => { let i = r.below(v.len() + 1); let n = r.below(5); for _ in 0..n { v.insert(i, r.next() as u8); } }
        }
    }
    v
}
fn tar_seed() -> Vec<u8> {
    fn header(name: &str, size: usize, kind: u8, link: &str) -> Vec<u8> {
        let mut b = vec![0u8; 512];
        b[..name.len()].copy_from_slice(name.as_bytes());
        b[100..108].copy_from_slice(b"0000755\0");
        b[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
        b[136..148].copy_from_slice(b"00000000000\0");
        b[156] = kind;
        b[157..157 + link.len()].copy_from_slice(link.as_bytes());
        b[257..263].copy_from_slice(b"ustar\0");
        b[148..156].copy_from_slice(b"        ");
        let sum: u32 = b.iter().map(|x| *x as u32).sum();
        b[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        b
    }
    let mut t = header("GE/", 0, b'5', "");
    t.extend(header("GE/proton", 5, b'0', ""));
    t.extend(b"hello"); t.extend(vec![0u8; 507]);
    t.extend(header("GE/link", 0, b'2', "proton"));
    t.extend(vec![0u8; 1024]);
    t
}
fn run(name: &str, seeds: &[Vec<u8>], iters: usize, f: &(dyn Fn(&[u8]) + Sync)) -> usize {
    let found = std::sync::Mutex::new(Vec::<Vec<u8>>::new());
    std::thread::scope(|s| {
        for t in 0..8u64 {
            let found = &found;
            s.spawn(move || {
                let mut r = Rng(0x9E3779B97F4A7C15 ^ (t + 1).wrapping_mul(0xD1B54A32D192ED03));
                for _ in 0..iters / 8 {
                    let pick = r.below(seeds.len()); let input = mutate(&mut r, &seeds[pick]);
                    if catch_unwind(AssertUnwindSafe(|| f(&input))).is_err() {
                        let mut g = found.lock().unwrap();
                        if g.len() < 3 { g.push(input); }
                    }
                }
            });
        }
    });
    let g = found.into_inner().unwrap();
    for i in &g {
        eprintln!("Crashed in {name}: len {} hex {} text {:?}", i.len(), i.iter().take(300).map(|b| format!("{b:02x}")).collect::<String>(), String::from_utf8_lossy(&i[..i.len().min(200)]));
    }
    g.len()
}
fn text(b: &[u8]) -> String { String::from_utf8_lossy(b).into_owned() }

/// Every reader of outside data (Steam's files, downloads, what the kernel and the helper's caller say) is fed
/// damaged copies of good input: flipped bits, cut ends, repeated and missing pieces, odd numbers. None may crash.
/// ROUGH_INPUT_N sets how many per reader (20,000 unless set; millions for a long run).
#[test]
fn damaged_input_never_crashes_a_reader() {
    // A panic is what this test looks for: it is counted and its input shown, not printed as a crash each time.
    std::panic::set_hook(Box::new(|_| {}));
    let n: usize = std::env::var("ROUGH_INPUT_N").ok().and_then(|v| v.parse().ok()).unwrap_or(20000);
    let json_seeds: Vec<Vec<u8>> = [r#"{"a":[1,2.5,-3e4,true,null,"x\né😀"],"b":{"c":{}}}"#, r#"{"kind":"sysfs","path":"/sys/class/power_supply/BAT0/charge_control_end_threshold","value":"80"}"#, r#"{"next":8,"entries":[{"id":7,"change":{"kind":"sysctl","key":"vm.swappiness","value":180},"before":{"value":"100"},"time":1,"steamos":"3.8"}]}"#, r#"{"tier":"gold","total":2107,"score":0.79}"#, r#"[{"tag_name":"GE-Proton10-98","assets":[{"name":"a.tar.gz","browser_download_url":"https://x/y","size":12}]}]"#].iter().map(|s| s.as_bytes().to_vec()).collect();
    let vdf_seeds: Vec<Vec<u8>> = ["\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/deck/.local/share/Steam\"\n\t\t\"apps\"\n\t\t{\n\t\t\t\"228980\"\t\t\"1\"\n\t\t}\n\t}\n}\n", "\"AppState\"\n{\n\t\"appid\"\t\"1086940\"\n\t\"name\"\t\"Baldur's \\\"Gate\\\" 3\" // c\n\t\"SizeOnDisk\" \"1\" [$WIN32]\n}"].iter().map(|s| s.as_bytes().to_vec()).collect();
    let mut bin = vec![0u8]; bin.extend(b"shortcuts\0"); bin.push(0); bin.extend(b"0\0"); bin.push(2); bin.extend(b"appid\0"); bin.extend([1, 2, 3, 0x80]); bin.push(1); bin.extend(b"AppName\0Turbine Hub\0"); bin.push(1); bin.extend(b"Exe\0\"/usr/bin/flatpak\"\0"); bin.push(0); bin.extend(b"tags\0"); bin.push(1); bin.extend(b"0\0fav\0"); bin.extend([8, 8, 8, 8]);
    let png = e::png::encode_rgb(5, 3, &[7u8; 45]);
    let gz = e::deflate::gzip(&b"hello hello hello hello turbine turbine hub".repeat(20));
    let z = e::deflate::zlib(b"abcabcabcabcabcabc");
    let raw = e::deflate::deflate(&[5u8; 3000]);
    let od = [h::gpu::OD_RX9000_V612, h::gpu::OD_RX7000, h::gpu::OD_RX6000].map(|s| s.as_bytes().to_vec());
    let mut bad = 0;
    bad += run("json", &json_seeds, n, &|b| { let _ = e::json::parse(&text(b)); });
    bad += run("helper-json", &json_seeds, n, &|b| { if let Ok(j) = h::json::parse(&text(b)) { let _ = h::policy::Setting::from_json(&j); let _ = h::ops::change_id(&j); } });
    bad += run("ledger", &json_seeds, n, &|b| { if let Ok(l) = h::ledger::Ledger::parse(&text(b)) { let _ = l.to_text(); let _ = l.settings(); } });
    bad += run("vdf", &vdf_seeds, n, &|b| { let _ = e::vdf::parse(&text(b)); });
    bad += run("vdf-bin", &[bin], n, &|b| { let _ = e::vdf::parse_binary(b); if let Ok(d) = e::vdf::parse_binary_doc(b) { let _ = e::vdf::write_binary_doc(&d); } let _ = e::shortcuts::parse(b); });
    bad += run("png", &[png], n, &|b| { let _ = e::png::size(b); let _ = e::png::decode(b); let _ = e::png::from_ico(b); });
    bad += run("inflate", &[raw, z, gz], n, &|b| { let _ = e::inflate::inflate(b, 1 << 20); let mut out = Vec::new(); let _ = e::inflate::inflate_stream(Cursor::new(b), &mut out, 1 << 20); });
    let dir = std::env::temp_dir().join(format!("th-rough-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    bad += run("tar", &[tar_seed()], n / 4, &|b| { let d = dir.join(format!("{:?}", std::thread::current().id()).replace(['(', ')'], "")); let _ = std::fs::remove_dir_all(&d); let _ = std::fs::create_dir_all(&d); let _ = e::tar::unpack_tar(Cursor::new(b), &d, e::tar::Limits { total: 1 << 20, entries: 100 }); });
    bad += run("edid", &[vec![0u8; 128], { let mut v = vec![0, 255, 255, 255, 255, 255, 255, 0]; v.extend((0..248).map(|i| (i * 7) as u8)); v }], n, &|b| { let _ = e::hardware::parse_edid(b); });
    bad += run("hid", &[vec![0x06, 0x31, 0xff, 0x09, 0x80, 0xa1, 0x01, 0x85, 0x5d, 0xc0, 0x05, 0x59, 0x09, 0x01, 0xa1, 0x01, 0xc0]], n, &|b| { let _ = h::hid::collections(b); let _ = h::hid::role(&text(b), b); let _ = h::hid::ids(&text(b)); });
    bad += run("od", &od, n, &|b| { let o = h::gpu::parse_od(&text(b)); let _ = o.check(-30); let _ = h::gpu::parse_mask(&text(b)); let _ = h::gpu::parse_armed(&text(b)); let _ = h::gpu::parse_tripped(&text(b)); });
    bad += run("misc-text", &[json_seeds[3].clone(), b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nretry-after: 3\r\n\r\n".to_vec(), b"abc123  TurbineHub.flatpak\n".to_vec(), b"mode=veteran\nscale=100\n".to_vec()], n, &|b| { let t = text(b); let _ = e::protondb::parse(&t); let _ = e::net::parse_headers(&t); let _ = e::selfupdate::parse_sha256_file(&t); let _ = e::settings::Settings::parse(&t); let _ = e::switches::GameSwitches::parse(&t); let _ = e::library::parse_valve_report(&t); let _ = e::library::parse_anticheat(&t); let _ = e::library::parse_fixes(&t); let _ = e::snapshots::parse_backup(&t); let _ = e::selfupdate::Remembered::parse(&t); let _ = e::tar::safe_rel(&t); let _ = e::zip::safe_relative(&t); let _ = h::ops::live_word(&t); let _ = h::ops::windows_entries(&t); });
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(bad, 0);
}

/// Brackets nested millions deep end in an error or a value, never in a stack that runs out.
#[test]
fn deep_nesting_never_runs_out_of_stack() {
    for n in [1_000usize, 100_000, 2_000_000] {
        let _ = e::json::parse(&"[".repeat(n));
        let _ = e::json::parse(&"{\"a\":".repeat(n));
        let _ = h::json::parse(&"[".repeat(n));
        let _ = e::vdf::parse(&"\"a\"{".repeat(n));
        let mut b = Vec::new(); for _ in 0..n { b.extend([0u8, b'a', 0]); }
        let _ = e::vdf::parse_binary(&b);
    }
}

