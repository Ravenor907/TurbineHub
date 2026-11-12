//! The archives in tests/data were made with Python's zipfile and zlib, an independent implementation.

mod support;

use std::fs;
use std::path::PathBuf;
use support::{data, scratch, write_zip};
use turbinehub_engine::zip::{Limits, Zip, ZipError};

#[test]
fn unpacks_every_kind_of_entry() {
    let out = scratch("good");
    let dest = out.join("mod");
    let written = Zip::open(&data("good.zip")).unwrap().extract_all(&dest, &Limits::default()).unwrap();
    assert_eq!(written.len(), 7);
    let readme = fs::read(dest.join("Data/readme.txt")).unwrap();
    assert_eq!(readme, "Skyrim mod readme line. ".repeat(400).into_bytes());
    assert_eq!(fs::read(dest.join("Data/stored.esp")).unwrap(), b"TES4 plugin bytes");
    assert_eq!(fs::read(dest.join("Data/tiny.txt")).unwrap(), b"hi");
    assert_eq!(fs::read(dest.join("Data/empty.txt")).unwrap(), b"");
    assert_eq!(fs::read(dest.join("Data/\u{dc}n\u{ef}code name.txt")).unwrap(), b"unicode");
    assert_eq!(fs::metadata(dest.join("Data/SKSE/Plugins/test.dll")).unwrap().len(), 70000);
    assert!(fs::metadata(dest.join("Data/mixed.txt")).unwrap().len() > 100_000);
    assert!(!out.join(".mod.unpacking").exists());
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn reads_zip64() {
    let out = scratch("z64");
    let written = Zip::open(&data("zip64.zip")).unwrap().extract_all(&out.join("m"), &Limits::default()).unwrap();
    assert_eq!(written.len(), 2);
    assert_eq!(fs::read(out.join("m/small.txt")).unwrap(), b"small");
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn refuses_escaping_paths() {
    assert!(matches!(Zip::open(&data("evil.zip")), Err(ZipError::UnsafePath(_))));
}

#[test]
fn skips_links() {
    let out = scratch("link");
    let written = Zip::open(&data("link.zip")).unwrap().extract_all(&out.join("m"), &Limits::default()).unwrap();
    assert_eq!(written, vec![PathBuf::from("real.txt")]);
    assert!(!out.join("m/link").exists());
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn notices_damage_and_leaves_nothing_behind() {
    let out = scratch("crc");
    let r = Zip::open(&data("badcrc.zip")).unwrap().extract_all(&out.join("m"), &Limits::default());
    assert!(matches!(r, Err(ZipError::Checksum(_))), "{r:?}");
    assert!(!out.join("m").exists() && !out.join(".m.unpacking").exists());
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn refuses_bombs() {
    let out = scratch("bomb");
    let r = Zip::open(&data("bomb.zip")).unwrap().extract_all(&out.join("m"), &Limits::default());
    assert!(matches!(r, Err(ZipError::TooBig(_))), "{r:?}");
    // And with a lower total cap, before reading anything.
    let small = Limits { total: 1 << 20, ratio: 400 };
    let r = Zip::open(&data("bomb.zip")).unwrap().extract_all(&out.join("m"), &small);
    assert!(matches!(r, Err(ZipError::TooBig(_))));
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn not_a_zip() {
    assert!(matches!(Zip::open(&data("../scan.rs")), Err(ZipError::NotZip)));
}

#[test]
fn many_entries_unpack() {
    let out = scratch("many");
    let names: Vec<String> = (0..20_000).map(|i| format!("BepInEx/plugins/p{}/f{i}.txt", i % 50)).collect();
    let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), b"x" as &[u8])).collect();
    write_zip(&out.join("t.zip"), &entries);
    // Counted in this thread's CPU time: the clock also counts waits for a disk other tests keep busy, which made
    // this check fail on the build machine now and then while the unpacking itself was quick.
    let cpu = || {
        fs::read_to_string("/proc/thread-self/schedstat")
            .ok()
            .and_then(|s| s.split_whitespace().next().and_then(|n| n.parse::<u64>().ok()))
            .map(std::time::Duration::from_nanos)
    };
    let (c0, t) = (cpu(), std::time::Instant::now());
    let limits = Limits { total: 64 << 20, ratio: 400 };
    let written = Zip::open(&out.join("t.zip")).unwrap().extract_all(&out.join("m"), &limits).unwrap();
    let took = match (c0, cpu()) {
        (Some(a), Some(b)) => b.saturating_sub(a),
        _ => t.elapsed(),
    };
    assert_eq!(written.len(), 20_000);
    assert!(took.as_secs() < 60, "{took:?} of CPU time");
    let _ = fs::remove_dir_all(&out);
}
