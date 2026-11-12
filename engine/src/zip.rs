//! Reads zip archives (including ZIP64) and unpacks them safely: nothing outside the target folder, no links, no
//! archive bombs, every file checked against its CRC-32. Nothing inside an archive is ever run.

use crate::inflate::{inflate_stream, InflateError, StreamError};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub enum ZipError {
    Io(std::io::Error),
    NotZip,
    Damaged(&'static str),
    Unsupported(String),
    UnsafePath(String),
    TooBig(String),
    Inflate(String, InflateError),
    Checksum(String),
}

impl std::fmt::Display for ZipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZipError::Io(e) => write!(f, "couldn't read or write a file ({e})"),
            ZipError::NotZip => f.write_str("this isn't a zip file"),
            ZipError::Damaged(what) => write!(f, "the zip file is damaged ({what})"),
            ZipError::Unsupported(what) => write!(f, "this zip uses something Turbine Hub can't open yet ({what})"),
            ZipError::UnsafePath(p) => write!(f, "the zip tries to put a file outside its folder ({p}), so it wasn't opened"),
            ZipError::TooBig(p) => write!(f, "{p} unpacks to far more than it should, so the zip wasn't opened"),
            ZipError::Inflate(p, e) => write!(f, "{p}: {e}"),
            ZipError::Checksum(p) => write!(f, "{p} doesn't match its checksum, so the download is probably damaged"),
        }
    }
}

impl std::error::Error for ZipError {}

impl From<std::io::Error> for ZipError {
    fn from(e: std::io::Error) -> Self {
        ZipError::Io(e)
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// Path inside the archive, '/' separated, already checked to be safe.
    pub name: String,
    pub is_dir: bool,
    method: u16,
    crc: u32,
    pub compressed: u64,
    pub size: u64,
    /// Where the file's data starts (after its local header), checked to lie inside the archive.
    data_offset: u64,
    is_link: bool,
}

pub struct Limits {
    /// Most bytes all files together may unpack to.
    pub total: u64,
    /// Most a single file may grow compared to its compressed size, once it is larger than 64 MiB.
    pub ratio: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { total: 200 << 30, ratio: 400 }
    }
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn u64le(b: &[u8], at: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(a)
}

/// Code page 437, which zips use for names when they don't say UTF-8 (upper half only).
const CP437: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";

fn decode_name(raw: &[u8], utf8: bool) -> String {
    if utf8 {
        return String::from_utf8_lossy(raw).into_owned();
    }
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.to_string(); // plain ASCII, or UTF-8 without the flag (common)
    }
    let table: Vec<char> = CP437.chars().collect();
    raw.iter().map(|&b| if b < 0x80 { b as char } else { table[(b - 0x80) as usize] }).collect()
}

/// Turns an archive path into a safe relative path, or refuses it.
pub fn safe_relative(name: &str) -> Result<PathBuf, ZipError> {
    let clean = name.replace('\\', "/");
    if clean.contains('\0') || clean.starts_with('/') || clean.chars().nth(1) == Some(':') {
        return Err(ZipError::UnsafePath(name.to_string()));
    }
    let mut out = PathBuf::new();
    for part in clean.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(ZipError::UnsafePath(name.to_string())),
            p => out.push(p),
        }
    }
    for c in out.components() {
        if !matches!(c, Component::Normal(_)) {
            return Err(ZipError::UnsafePath(name.to_string()));
        }
    }
    Ok(out)
}

pub struct Zip {
    file: File,
    pub entries: Vec<Entry>,
}

impl Zip {
    pub fn open(path: &Path) -> Result<Zip, ZipError> {
        let mut file = File::open(path)?;
        let len = file.seek(SeekFrom::End(0))?;
        if len < 22 {
            return Err(ZipError::NotZip);
        }
        // The end record is in the last 64 KiB + 22 bytes (the comment can be up to 65535 bytes).
        let tail_len = len.min(65_557) as usize;
        let mut tail = vec![0u8; tail_len];
        file.seek(SeekFrom::Start(len - tail_len as u64))?;
        file.read_exact(&mut tail)?;
        let eocd = (0..=tail_len - 22).rev().find(|&i| u32le(&tail, i) == 0x0605_4b50).ok_or(ZipError::NotZip)?;
        let mut count = u16le(&tail, eocd + 10) as u64;
        let mut cd_size = u32le(&tail, eocd + 12) as u64;
        let mut cd_off = u32le(&tail, eocd + 16) as u64;
        // ZIP64: a locator sits just before the end record.
        if eocd >= 20 && u32le(&tail, eocd - 20) == 0x0706_4b50 {
            let rec = u64le(&tail, eocd - 12);
            let mut r = [0u8; 56];
            file.seek(SeekFrom::Start(rec))?;
            file.read_exact(&mut r)?;
            if u32le(&r, 0) != 0x0606_4b50 {
                return Err(ZipError::Damaged("ZIP64 end record"));
            }
            count = u64le(&r, 32);
            cd_size = u64le(&r, 40);
            cd_off = u64le(&r, 48);
        }
        if cd_off.checked_add(cd_size).map(|e| e > len).unwrap_or(true) || count > 2_000_000 {
            return Err(ZipError::Damaged("directory"));
        }
        let mut cd = vec![0u8; cd_size as usize];
        file.seek(SeekFrom::Start(cd_off))?;
        file.read_exact(&mut cd)?;
        // Every directory entry takes at least 46 bytes, so the directory's size caps how many there can be.
        let mut entries = Vec::with_capacity(count.min(cd_size / 46) as usize);
        let mut names = HashSet::new();
        let mut ranges = Vec::new();
        let mut p = 0usize;
        for _ in 0..count {
            if p + 46 > cd.len() || u32le(&cd, p) != 0x0201_4b50 {
                return Err(ZipError::Damaged("directory entry"));
            }
            let made_by_os = cd[p + 5];
            let flags = u16le(&cd, p + 8);
            let method = u16le(&cd, p + 10);
            let crc = u32le(&cd, p + 16);
            let mut compressed = u32le(&cd, p + 20) as u64;
            let mut size = u32le(&cd, p + 24) as u64;
            let name_len = u16le(&cd, p + 28) as usize;
            let extra_len = u16le(&cd, p + 30) as usize;
            let comment_len = u16le(&cd, p + 32) as usize;
            let ext_attr = u32le(&cd, p + 38);
            let mut local_offset = u32le(&cd, p + 42) as u64;
            let end = p + 46 + name_len + extra_len + comment_len;
            if end > cd.len() {
                return Err(ZipError::Damaged("directory entry"));
            }
            let raw_name = &cd[p + 46..p + 46 + name_len];
            let extra = &cd[p + 46 + name_len..p + 46 + name_len + extra_len];
            // ZIP64 sizes and offset, in that order, only for fields that are 0xFFFFFFFF.
            let mut q = 0;
            while q + 4 <= extra.len() {
                let id = u16le(extra, q);
                let sz = u16le(extra, q + 2) as usize;
                if q + 4 + sz > extra.len() {
                    break;
                }
                if id == 0x0001 {
                    let mut f = q + 4;
                    let lim = q + 4 + sz;
                    if size == 0xFFFF_FFFF && f + 8 <= lim {
                        size = u64le(extra, f);
                        f += 8;
                    }
                    if compressed == 0xFFFF_FFFF && f + 8 <= lim {
                        compressed = u64le(extra, f);
                        f += 8;
                    }
                    if local_offset == 0xFFFF_FFFF && f + 8 <= lim {
                        local_offset = u64le(extra, f);
                    }
                }
                q += 4 + sz;
            }
            let name = decode_name(raw_name, flags & 0x0800 != 0);
            if flags & 0x0001 != 0 {
                return Err(ZipError::Unsupported(format!("password protection on {name}")));
            }
            let rel = safe_relative(&name)?;
            let is_dir = name.ends_with('/') || name.ends_with('\\');
            // Unix-made zips keep the file type in the high bits: 0o120000 is a symbolic link.
            let is_link = made_by_os == 3 && (ext_attr >> 16) & 0o170000 == 0o120000;
            let clean = rel.to_string_lossy().replace('\\', "/");
            // Two entries landing on the same path (also when only upper/lower case differs, as on exFAT and in
            // games that ignore case) would silently replace each other.
            if !clean.is_empty() && !names.insert(clean.to_lowercase()) {
                return Err(ZipError::Damaged("two files with the same name"));
            }
            // The local header must be inside the archive, before the directory, and agree with the directory.
            let header_end = local_offset.checked_add(30).filter(|&e| e <= cd_off).ok_or(ZipError::Damaged("file header"))?;
            let mut h = [0u8; 30];
            file.seek(SeekFrom::Start(local_offset))?;
            file.read_exact(&mut h)?;
            if u32le(&h, 0) != 0x0403_4b50 {
                return Err(ZipError::Damaged("file header"));
            }
            let local_name_len = u16le(&h, 26) as u64;
            let local_extra_len = u16le(&h, 28) as u64;
            let data_offset = header_end + local_name_len + local_extra_len;
            let data_end = data_offset.checked_add(compressed).filter(|&e| e <= cd_off).ok_or(ZipError::Damaged("file size"))?;
            if u16le(&h, 8) != method || local_name_len != name_len as u64 {
                return Err(ZipError::Damaged("file header doesn't match the directory"));
            }
            let mut local_name = vec![0u8; name_len];
            file.read_exact(&mut local_name)?;
            if local_name != raw_name {
                return Err(ZipError::Damaged("file header doesn't match the directory"));
            }
            ranges.push((local_offset, data_end));
            entries.push(Entry { name: clean, is_dir, method, crc, compressed, size, data_offset, is_link });
            p = end;
        }
        // Each file's header and data must have its own bytes: entries sharing data are a known archive bomb.
        ranges.sort_unstable();
        if ranges.windows(2).any(|w| w[1].0 < w[0].1) {
            return Err(ZipError::Damaged("files overlap"));
        }
        Ok(Zip { file, entries })
    }

    /// Reads one file's contents.
    pub fn read(&mut self, e: &Entry, limits: &Limits) -> Result<Vec<u8>, ZipError> {
        let mut out = Vec::with_capacity(e.size.min(1 << 24) as usize);
        self.unpack(e, limits, &mut out)?;
        Ok(out)
    }

    /// Unpacks one file into `sink` a chunk at a time, checking its size and CRC-32 on the way. Memory use stays at a
    /// few hundred KiB whatever the file's size. On an error, part of the file may already be in `sink`.
    pub fn unpack(&mut self, e: &Entry, limits: &Limits, sink: &mut dyn Write) -> Result<(), ZipError> {
        if e.size > (64 << 20) && e.size / e.compressed.max(1) > limits.ratio {
            return Err(ZipError::TooBig(e.name.clone()));
        }
        self.file.seek(SeekFrom::Start(e.data_offset))?;
        let mut sink = CrcWriter { inner: sink, crc: Crc32::new() };
        match e.method {
            0 => {
                if e.compressed != e.size {
                    return Err(ZipError::Damaged("file size"));
                }
                let mut buf = vec![0u8; COPY_CHUNK];
                let mut left = e.size;
                while left > 0 {
                    let n = left.min(COPY_CHUNK as u64) as usize;
                    self.file.read_exact(&mut buf[..n])?;
                    sink.write_all(&buf[..n])?;
                    left -= n as u64;
                }
            }
            8 => {
                let src = (&mut self.file).take(e.compressed);
                let written = inflate_stream(src, &mut sink, e.size).map_err(|err| match err {
                    StreamError::Inflate(i) => ZipError::Inflate(e.name.clone(), i),
                    StreamError::Read(io) | StreamError::Write(io) => ZipError::Io(io),
                })?;
                if written != e.size {
                    return Err(ZipError::Damaged("file size"));
                }
            }
            m => return Err(ZipError::Unsupported(format!("compression method {m} on {}", e.name))),
        }
        if sink.crc.finish() != e.crc {
            return Err(ZipError::Checksum(e.name.clone()));
        }
        Ok(())
    }

    /// Unpacks everything into `dest` (which must not exist yet). Unpacks into a temporary folder next to it first and
    /// renames it at the end, so `dest` is either complete or absent. Links are skipped. Returns the files written.
    /// Files are streamed to disk, so memory use doesn't grow with their size.
    pub fn extract_all(&mut self, dest: &Path, limits: &Limits) -> Result<Vec<PathBuf>, ZipError> {
        let total = self.entries.iter().filter(|e| !e.is_dir).try_fold(0u64, |sum, e| sum.checked_add(e.size));
        if total.map(|t| t > limits.total).unwrap_or(true) {
            return Err(ZipError::TooBig("The archive".into()));
        }
        if fs::symlink_metadata(dest).is_ok() {
            return Err(ZipError::Io(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "the destination folder already exists")));
        }
        let parent = dest.parent().ok_or(ZipError::Damaged("destination"))?;
        fs::create_dir_all(parent)?;
        let tmp = parent.join(format!(
            ".{}.unpacking",
            dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp)?;
        let result = (|| {
            let mut written = Vec::new();
            let entries = self.entries.clone();
            for e in &entries {
                let rel = safe_relative(&e.name)?;
                if rel.as_os_str().is_empty() {
                    continue;
                }
                let target = tmp.join(&rel);
                if e.is_dir {
                    fs::create_dir_all(&target)?;
                    continue;
                }
                if e.is_link {
                    continue;
                }
                if let Some(p) = target.parent() {
                    fs::create_dir_all(p)?;
                }
                let mut f = File::create(&target)?;
                self.unpack(e, limits, &mut f)?;
                written.push(rel);
            }
            // Every file reaches the disk before the folder gets its name, flushed in one pass after all the writes:
            // a flush between writes made each file wait for the disk (thousands of waits for a big mod, each as
            // slow as the disk is busy).
            for rel in &written {
                File::open(tmp.join(rel))?.sync_all()?;
            }
            fs::rename(&tmp, dest)?;
            Ok(written)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&tmp);
        }
        result
    }
}

/// How much stored (uncompressed) data is copied at a time.
const COPY_CHUNK: usize = 64 << 10;

/// Passes bytes through to `inner` and keeps a running CRC-32 of them.
struct CrcWriter<'a> {
    inner: &'a mut dyn Write,
    crc: Crc32,
}

impl Write for CrcWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.crc.update(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// A CRC-32 worked out a piece at a time: `update` with each chunk, then `finish`.
#[derive(Clone, Copy, Debug)]
pub struct Crc32(u32);

impl Default for Crc32 {
    fn default() -> Self {
        Crc32::new()
    }
}

impl Crc32 {
    pub fn new() -> Self {
        Crc32(0xFFFF_FFFF)
    }

    pub fn update(&mut self, data: &[u8]) {
        let t = crc_table();
        let mut c = self.0;
        for &b in data {
            c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        self.0 = c;
    }

    pub fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

fn crc_table() -> &'static [u32; 256] {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *v = c;
        }
        t
    })
}

/// CRC-32 (IEEE), as zip uses it.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = Crc32::new();
    c.update(data);
    c.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_known_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        let mut c = Crc32::new();
        c.update(b"1234");
        c.update(b"");
        c.update(b"56789");
        assert_eq!(c.finish(), 0xCBF4_3926);
    }

    #[test]
    fn refuses_unsafe_paths() {
        for bad in ["../x", "a/../../x", "/etc/passwd", "C:/x", "a\\..\\..\\x"] {
            assert!(safe_relative(bad).is_err(), "{bad}");
        }
        assert_eq!(safe_relative("Data\\meshes/./a.nif").unwrap(), PathBuf::from("Data/meshes/a.nif"));
    }

    #[test]
    fn decodes_cp437_names() {
        assert_eq!(decode_name(&[b'a', 0x81, b'b'], false), "aüb");
    }
}
