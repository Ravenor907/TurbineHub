//! Unpacking a .tar.gz (gzip, RFC 1952, around a tar archive), for the one download that comes that way: a Proton
//! build. Written here like `zip` and `inflate`, so nothing from outside touches a downloaded file.
//!
//! An archive is somebody else's list of file names, so every name is checked before anything is written:
//! - a name that starts at "/" or has ".." in it is refused,
//! - a link whose target starts at "/" or climbs out of the folder is refused,
//! - nothing is ever written through a link (every folder on the way to a file has to be a real folder),
//! - once everything is unpacked, every link is followed for real and has to end inside the folder,
//! - device files, pipes and other special entries are refused,
//! - the unpacked size and the number of entries are capped.
//!
//! The first refusal stops the unpacking with an error. The folder may be half filled then: callers unpack into a
//! scratch folder and move the result into place only when all of it worked (see `proton_get`).

use crate::inflate::{StreamError, inflate_stream};
use crate::zip::Crc32;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub enum TarError {
    Io(io::Error),
    /// Not a gzip file at all.
    NotGzip,
    /// The archive is cut off or its contents don't add up.
    Damaged(String),
    /// A name that would land outside the folder.
    UnsafePath(String),
    /// A link that points outside the folder, or a file that would be written through a link.
    UnsafeLink(String),
    /// More data or more entries than the limits allow.
    TooBig,
    /// A kind of entry Turbine Hub doesn't unpack (a device file, a pipe).
    Unsupported(String),
}

impl std::fmt::Display for TarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TarError::Io(e) => write!(f, "{e}"),
            TarError::NotGzip => write!(f, "the file isn't a .tar.gz archive"),
            TarError::Damaged(why) => write!(f, "the archive is damaged ({why})"),
            TarError::UnsafePath(p) => write!(f, "the archive holds a file that would land outside its folder ({p})"),
            TarError::UnsafeLink(p) => write!(f, "the archive holds a link that points outside its folder ({p})"),
            TarError::TooBig => write!(f, "the archive unpacks to more than is allowed"),
            TarError::Unsupported(p) => write!(f, "the archive holds something that isn't a file, folder or link ({p})"),
        }
    }
}

impl std::error::Error for TarError {}

impl From<io::Error> for TarError {
    fn from(e: io::Error) -> Self {
        TarError::Io(e)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Most bytes all files together may unpack to.
    pub total: u64,
    /// Most entries (files, folders, links) the archive may have.
    pub entries: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { total: 8 << 30, entries: 400_000 }
    }
}

/// What an unpacking wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unpacked {
    pub files: u64,
    pub folders: u64,
    pub links: u64,
    pub bytes: u64,
    /// The names at the top of the archive (a Proton build has exactly one: its folder).
    pub top: Vec<String>,
}

/// An archive name as a path inside the folder, or a refusal. "" and "." parts are dropped; a name that starts at
/// "/" or has a ".." part is refused.
pub fn safe_rel(name: &str) -> Result<PathBuf, TarError> {
    if name.contains('\0') || name.starts_with('/') {
        return Err(TarError::UnsafePath(name.to_string()));
    }
    let mut out = PathBuf::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(TarError::UnsafePath(name.to_string())),
            p => out.push(p),
        }
    }
    if out.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(TarError::UnsafePath(name.to_string()));
    }
    Ok(out)
}

/// Does a link at `rel` (inside the folder) with this target stay inside the folder, going by the names alone? A
/// target that starts at "/" never does; each ".." climbs one folder up from where the link is, and climbing above
/// the folder itself is outside.
pub fn link_stays_inside(rel: &Path, target: &str) -> bool {
    if target.is_empty() || target.contains('\0') || target.starts_with('/') {
        return false;
    }
    let mut depth = rel.components().count().saturating_sub(1);
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ => depth += 1,
        }
    }
    true
}

fn octal(field: &[u8]) -> Option<u64> {
    // Sizes too big for the octal field are written as a binary number with the top bit of the first byte set.
    if field.first().is_some_and(|b| b & 0x80 != 0) {
        if field[..field.len().saturating_sub(8)].iter().enumerate().any(|(i, b)| if i == 0 { b & 0x7f != 0 } else { *b != 0 }) {
            return None;
        }
        return Some(field[field.len().saturating_sub(8)..].iter().fold(0u64, |v, b| (v << 8) | *b as u64));
    }
    let text: Vec<u8> = field.iter().copied().filter(|b| *b != 0 && *b != b' ').collect();
    if text.len() > 21 {
        return None;
    }
    text.iter().try_fold(0u64, |v, b| if (b'0'..=b'7').contains(b) { Some((v << 3) | (b - b'0') as u64) } else { None })
}

fn text(field: &[u8]) -> String {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// What the bytes after a header are.
enum Body {
    /// Nothing follows, or what follows is passed over.
    Skip,
    File(File),
    /// A longer name for the next entry ('L'), a longer link target ('K'), or a pax record list ('x').
    Meta(u8, Vec<u8>),
}

/// Takes the unpacked tar stream a piece at a time and writes its entries under `dest`.
struct TarSink<'a> {
    dest: &'a Path,
    limits: Limits,
    header: Vec<u8>,
    /// Bytes of the current entry's data still to come, then the padding up to the next 512.
    remaining: u64,
    padding: u64,
    body: Body,
    long_name: Option<String>,
    long_link: Option<String>,
    /// Saw the empty block that ends an archive.
    ended: bool,
    crc: Crc32,
    stream_bytes: u64,
    entries: u64,
    done: Unpacked,
    /// Folders known to be real folders (never links).
    real: HashSet<PathBuf>,
    links: Vec<PathBuf>,
    err: Option<TarError>,
}

impl<'a> TarSink<'a> {
    fn new(dest: &'a Path, limits: Limits) -> Self {
        TarSink { dest, limits, header: Vec::with_capacity(512), remaining: 0, padding: 0, body: Body::Skip, long_name: None, long_link: None, ended: false, crc: Crc32::new(), stream_bytes: 0, entries: 0, done: Unpacked::default(), real: HashSet::new(), links: Vec::new(), err: None }
    }

    /// Makes sure every folder on the way to `rel` is a real folder, making the missing ones. A link on the way is
    /// a refusal: writing through it could land anywhere.
    fn real_parent(&mut self, rel: &Path) -> Result<(), TarError> {
        let Some(parent) = rel.parent() else { return Ok(()) };
        let mut at = PathBuf::new();
        for c in parent.components() {
            at.push(c);
            if self.real.contains(&at) {
                continue;
            }
            let full = self.dest.join(&at);
            match fs::symlink_metadata(&full) {
                Ok(m) if m.file_type().is_symlink() => return Err(TarError::UnsafeLink(rel.display().to_string())),
                Ok(m) if m.is_dir() => {}
                Ok(_) => return Err(TarError::Damaged(format!("{} is a file and a folder", at.display()))),
                Err(_) => {
                    fs::create_dir(&full)?;
                    self.done.folders += 1;
                }
            }
            self.real.insert(at.clone());
        }
        Ok(())
    }

    fn header_done(&mut self) -> Result<(), TarError> {
        let h = std::mem::take(&mut self.header);
        if h.iter().all(|b| *b == 0) {
            self.ended = true;
            return Ok(());
        }
        let sum: u64 = h.iter().enumerate().map(|(i, b)| if (148..156).contains(&i) { 32 } else { *b as u64 }).sum();
        if octal(&h[148..156]) != Some(sum) {
            return Err(TarError::Damaged("a header's checksum is wrong".into()));
        }
        let size = octal(&h[124..136]).ok_or_else(|| TarError::Damaged("a size can't be read".into()))?;
        let kind = h[156];
        let mut name = text(&h[0..100]);
        if &h[257..263] == b"ustar\0" {
            let prefix = text(&h[345..500]);
            if !prefix.is_empty() {
                name = format!("{prefix}/{name}");
            }
        }
        let mut link = text(&h[157..257]);
        let mode = octal(&h[100..108]).unwrap_or(0o644) as u32;
        self.remaining = size;
        self.padding = (512 - size % 512) % 512;
        self.body = Body::Skip;
        match kind {
            b'L' | b'K' | b'x' => {
                if size > 1 << 20 {
                    return Err(TarError::Damaged("a name is too long".into()));
                }
                self.body = Body::Meta(kind, Vec::with_capacity(size as usize));
                return Ok(());
            }
            // Settings for the whole archive (a comment, usually): nothing in them names a file.
            b'g' => return Ok(()),
            _ => {}
        }
        if let Some(n) = self.long_name.take() {
            name = n;
        }
        if let Some(l) = self.long_link.take() {
            link = l;
        }
        self.entries += 1;
        if self.entries > self.limits.entries {
            return Err(TarError::TooBig);
        }
        let rel = safe_rel(&name)?;
        if let Some(Component::Normal(first)) = rel.components().next() {
            let first = first.to_string_lossy().into_owned();
            if !self.done.top.contains(&first) {
                self.done.top.push(first);
            }
        }
        let is_dir = kind == b'5' || (matches!(kind, b'0' | 0) && name.ends_with('/'));
        if rel.as_os_str().is_empty() {
            // "./" itself: the folder everything goes into.
            return if is_dir { Ok(()) } else { Err(TarError::UnsafePath(name)) };
        }
        let full = self.dest.join(&rel);
        self.real_parent(&rel)?;
        let there = fs::symlink_metadata(&full).ok();
        if is_dir {
            match there {
                Some(m) if m.file_type().is_symlink() => return Err(TarError::UnsafeLink(name)),
                Some(m) if m.is_dir() => {}
                Some(_) => return Err(TarError::Damaged(format!("{name} is there twice"))),
                None => {
                    fs::create_dir(&full)?;
                    self.done.folders += 1;
                }
            }
            self.real.insert(rel);
            return Ok(());
        }
        if there.is_some() {
            return Err(TarError::Damaged(format!("{name} is there twice")));
        }
        match kind {
            b'0' | 0 | b'7' => {
                if self.done.bytes.saturating_add(size) > self.limits.total {
                    return Err(TarError::TooBig);
                }
                // The owner can always read and write it (so it can be removed again), others never more than read
                // and run: no set-user bits, nothing others can write.
                let f = fs::OpenOptions::new().write(true).create_new(true).mode((mode & 0o755) | 0o600).open(&full)?;
                self.done.files += 1;
                self.done.bytes += size;
                self.body = Body::File(f);
            }
            b'2' => {
                if !link_stays_inside(&rel, &link) {
                    return Err(TarError::UnsafeLink(format!("{name} to {link}")));
                }
                std::os::unix::fs::symlink(&link, &full)?;
                self.links.push(full);
                self.done.links += 1;
                // A link has no data, whatever its header says.
                self.remaining = 0;
                self.padding = 0;
            }
            b'1' => {
                // A second name for a file already unpacked.
                let from_rel = safe_rel(&link)?;
                self.real_parent(&from_rel)?;
                let from = self.dest.join(&from_rel);
                if !fs::symlink_metadata(&from).is_ok_and(|m| m.is_file()) {
                    return Err(TarError::Damaged(format!("{name} is a second name for a file that isn't there")));
                }
                if fs::hard_link(&from, &full).is_err() {
                    fs::copy(&from, &full)?;
                }
                self.done.files += 1;
                self.remaining = 0;
                self.padding = 0;
            }
            _ => return Err(TarError::Unsupported(name)),
        }
        Ok(())
    }

    /// The data of an entry is complete.
    fn body_done(&mut self) -> Result<(), TarError> {
        match std::mem::replace(&mut self.body, Body::Skip) {
            Body::Meta(b'L', bytes) => self.long_name = Some(text(&bytes)),
            Body::Meta(b'K', bytes) => self.long_link = Some(text(&bytes)),
            Body::Meta(_, bytes) => {
                // Pax records: "<length> <key>=<value>\n", the length counting the whole record.
                let mut rest = &bytes[..];
                while !rest.is_empty() {
                    let space = rest.iter().position(|b| *b == b' ').ok_or_else(|| TarError::Damaged("a pax record can't be read".into()))?;
                    let len: usize = std::str::from_utf8(&rest[..space]).ok().and_then(|t| t.parse().ok()).filter(|l| *l > space + 1 && *l <= rest.len()).ok_or_else(|| TarError::Damaged("a pax record can't be read".into()))?;
                    let record = String::from_utf8_lossy(&rest[space + 1..len]).into_owned();
                    let record = record.strip_suffix('\n').unwrap_or(&record);
                    match record.split_once('=') {
                        Some(("path", v)) => self.long_name = Some(v.to_string()),
                        Some(("linkpath", v)) => self.long_link = Some(v.to_string()),
                        // A size the header's field can't hold: files that big are over every limit here anyway.
                        Some(("size", _)) => return Err(TarError::TooBig),
                        _ => {}
                    }
                    rest = &rest[len..];
                }
            }
            Body::File(f) => drop(f),
            Body::Skip => {}
        }
        Ok(())
    }

    fn take(&mut self, mut buf: &[u8]) -> Result<(), TarError> {
        while !buf.is_empty() {
            if self.ended {
                return Ok(());
            }
            if self.remaining > 0 {
                let n = (self.remaining.min(buf.len() as u64)) as usize;
                match &mut self.body {
                    Body::File(f) => f.write_all(&buf[..n])?,
                    Body::Meta(_, bytes) => bytes.extend_from_slice(&buf[..n]),
                    Body::Skip => {}
                }
                self.remaining -= n as u64;
                buf = &buf[n..];
                if self.remaining == 0 {
                    self.body_done()?;
                }
            } else if self.padding > 0 {
                let n = (self.padding.min(buf.len() as u64)) as usize;
                self.padding -= n as u64;
                buf = &buf[n..];
            } else {
                let n = (512 - self.header.len()).min(buf.len());
                self.header.extend_from_slice(&buf[..n]);
                buf = &buf[n..];
                if self.header.len() == 512 {
                    self.header_done()?;
                    if self.remaining == 0 {
                        self.body_done()?;
                    }
                }
            }
        }
        Ok(())
    }

    /// The stream is over: nothing may be half read, and every link has to end inside the folder for real.
    fn finish(mut self) -> Result<Unpacked, TarError> {
        if let Some(e) = self.err.take() {
            return Err(e);
        }
        if self.remaining > 0 || (!self.ended && !self.header.is_empty()) {
            return Err(TarError::Damaged("it ends early".into()));
        }
        let root = fs::canonicalize(self.dest)?;
        for l in &self.links {
            // A link to something that isn't there leads nowhere; its target was checked by name already.
            if let Ok(real) = fs::canonicalize(l)
                && !real.starts_with(&root)
            {
                return Err(TarError::UnsafeLink(l.strip_prefix(self.dest).unwrap_or(l).display().to_string()));
            }
        }
        Ok(self.done)
    }
}

impl Write for TarSink<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.crc.update(buf);
        self.stream_bytes += buf.len() as u64;
        match self.take(buf) {
            Ok(()) => Ok(buf.len()),
            Err(e) => {
                self.err = Some(e);
                Err(io::Error::other("the archive was refused"))
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Unpacks a plain tar stream into `dest` (made when missing). For the rules see the top of this file.
pub fn unpack_tar(mut src: impl Read, dest: &Path, limits: Limits) -> Result<Unpacked, TarError> {
    fs::create_dir_all(dest)?;
    let mut sink = TarSink::new(dest, limits);
    let mut buf = vec![0u8; 64 << 10];
    loop {
        let n = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(TarError::Io(e)),
        };
        if sink.write(&buf[..n]).is_err() {
            break;
        }
    }
    sink.finish()
}

/// Reads past a gzip file's header, leaving the reader at the compressed data.
fn gzip_header(r: &mut impl Read) -> Result<(), TarError> {
    let mut h = [0u8; 10];
    r.read_exact(&mut h).map_err(|_| TarError::NotGzip)?;
    if h[0] != 0x1f || h[1] != 0x8b || h[2] != 8 || h[3] & 0xE0 != 0 {
        return Err(TarError::NotGzip);
    }
    let cut = |_| TarError::Damaged("it ends early".into());
    let flags = h[3];
    if flags & 4 != 0 {
        let mut len = [0u8; 2];
        r.read_exact(&mut len).map_err(cut)?;
        io::copy(&mut r.take(u16::from_le_bytes(len) as u64), &mut io::sink())?;
    }
    // The original file name, then a comment: each ends with a zero.
    for bit in [8u8, 16] {
        if flags & bit != 0 {
            let mut b = [1u8; 1];
            while b[0] != 0 {
                r.read_exact(&mut b).map_err(cut)?;
            }
        }
    }
    if flags & 2 != 0 {
        let mut crc = [0u8; 2];
        r.read_exact(&mut crc).map_err(cut)?;
    }
    Ok(())
}

/// Unpacks a .tar.gz file into `dest` (made when missing), and checks the gzip file's own checksum and length of
/// what was unpacked at the end. For the rules see the top of this file. On an error the folder may be half
/// filled: unpack into a scratch folder.
pub fn unpack_tar_gz(archive: &Path, dest: &Path, limits: Limits) -> Result<Unpacked, TarError> {
    let mut file = File::open(archive)?;
    let size = file.metadata()?.len();
    if size < 18 {
        return Err(TarError::NotGzip);
    }
    // The last eight bytes: the CRC-32 of the unpacked data and its length (the low 32 bits).
    let mut tail = [0u8; 8];
    file.seek(SeekFrom::End(-8))?;
    file.read_exact(&mut tail)?;
    file.seek(SeekFrom::Start(0))?;
    let mut r = BufReader::with_capacity(256 << 10, file);
    gzip_header(&mut r)?;
    fs::create_dir_all(dest)?;
    let mut sink = TarSink::new(dest, limits);
    // The tar stream is the files plus a header and padding for each: a generous cap of its own.
    let cap = limits.total.saturating_add(limits.entries.saturating_mul(2048)).saturating_add(1 << 20);
    match inflate_stream(&mut r, &mut sink, cap) {
        Ok(_) => {}
        Err(StreamError::Write(e)) => return Err(sink.err.take().unwrap_or(TarError::Io(e))),
        Err(StreamError::Read(e)) => return Err(TarError::Io(e)),
        Err(StreamError::Inflate(crate::inflate::InflateError::TooLarge)) => return Err(TarError::TooBig),
        Err(StreamError::Inflate(e)) => return Err(TarError::Damaged(e.to_string())),
    }
    let (crc, len) = (sink.crc.finish(), sink.stream_bytes);
    if crc.to_le_bytes() != tail[..4] || (len as u32).to_le_bytes() != tail[4..] {
        return Err(TarError::Damaged("its checksum doesn't match".into()));
    }
    sink.finish()
}

/// Builds small tar archives for the tests here and in `proton_get`.
#[cfg(test)]
pub(crate) mod testing {
    /// One entry: its kind byte ('0' file, '5' folder, '2' link, '1' second name, 'L', 'x' and so on), name, link
    /// target and data.
    pub fn entry(kind: u8, name: &str, link: &str, data: &[u8]) -> Vec<u8> {
        let mut h = vec![0u8; 512];
        h[..name.len().min(100)].copy_from_slice(&name.as_bytes()[..name.len().min(100)]);
        h[100..107].copy_from_slice(if kind == b'5' || name.ends_with("proton") { b"0000755" } else { b"0000644" });
        h[108..115].copy_from_slice(b"0001750");
        h[116..123].copy_from_slice(b"0001750");
        h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        h[136..147].copy_from_slice(b"14000000000");
        h[156] = kind;
        h[157..157 + link.len().min(100)].copy_from_slice(&link.as_bytes()[..link.len().min(100)]);
        h[257..265].copy_from_slice(b"ustar  \0");
        h[148..156].fill(b' ');
        let sum: u32 = h.iter().map(|b| *b as u32).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        h.extend_from_slice(data);
        h.resize(512 + data.len().div_ceil(512) * 512, 0);
        h
    }

    /// A whole archive from entries, with the two empty blocks that end one.
    pub fn archive(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut out: Vec<u8> = entries.concat();
        out.resize(out.len() + 1024, 0);
        out
    }

    /// A small made-up Proton build: its folder with the files `compat_tool::is_proton` looks for, a file in a
    /// deeper folder with a name too long for a plain header, and a link that stays inside.
    pub fn proton(name: &str) -> Vec<u8> {
        let long = format!("{name}/files/share/{}/readme.txt", "deep/".repeat(24));
        let mut long_data = long.clone().into_bytes();
        long_data.push(0);
        archive(&[
            entry(b'5', &format!("{name}/"), "", b""),
            entry(b'0', &format!("{name}/proton"), "", b"#!/usr/bin/env python3\n"),
            entry(b'0', &format!("{name}/toolmanifest.vdf"), "", b"\"manifest\"\n{\n  \"commandline\" \"/proton %verb%\"\n}\n"),
            entry(b'0', &format!("{name}/compatibilitytool.vdf"), "", format!("\"compatibilitytools\"\n{{\n  \"compat_tools\"\n  {{\n    \"{name}\"\n    {{\n      \"install_path\" \".\"\n      \"display_name\" \"{name}\"\n    }}\n  }}\n}}\n").as_bytes()),
            entry(b'0', &format!("{name}/version"), "", format!("1700000000 {name}\n").as_bytes()),
            entry(b'5', &format!("{name}/files/lib/"), "", b""),
            entry(b'0', &format!("{name}/files/lib/wine.so"), "", &[7u8; 3000]),
            entry(b'2', &format!("{name}/files/lib64"), "lib", b""),
            entry(b'2', &format!("{name}/files/lib/back"), "../../version", b""),
            entry(b'L', "././@LongLink", "", &long_data),
            entry(b'0', "cut", "", b"found by its long name\n"),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{archive, entry, proton};
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-tar-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn unpack(tag: &str, entries: &[Vec<u8>]) -> (PathBuf, Result<Unpacked, TarError>) {
        let d = dir(tag);
        let out = d.join("out");
        let r = unpack_tar(&archive(entries)[..], &out, Limits::default());
        (d, r)
    }

    #[test]
    fn a_small_build_unpacks_with_its_folders_links_and_long_names() {
        let d = dir("good");
        let gz = d.join("GE-Proton10-99.tar.gz");
        fs::write(&gz, crate::deflate::gzip(&proton("GE-Proton10-99"))).unwrap();
        let out = d.join("out");
        let done = unpack_tar_gz(&gz, &out, Limits::default()).unwrap();
        assert_eq!(done.top, vec!["GE-Proton10-99"]);
        assert_eq!((done.files, done.links), (6, 2));
        let top = out.join("GE-Proton10-99");
        assert!(crate::compat_tool::is_proton(&top));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(top.join("proton")).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(fs::metadata(top.join("version")).unwrap().permissions().mode() & 0o777, 0o644);
        assert_eq!(fs::read(top.join("files/lib64/wine.so")).unwrap(), vec![7u8; 3000]);
        assert_eq!(fs::read_to_string(top.join("files/lib/back")).unwrap(), "1700000000 GE-Proton10-99\n");
        assert_eq!(fs::read_to_string(top.join(format!("files/share/{}readme.txt", "deep/".repeat(24)))).unwrap(), "found by its long name\n");
        // A changed byte in the packed data, a cut-off file and a file that isn't gzip are all refused.
        let mut bad = fs::read(&gz).unwrap();
        let n = bad.len();
        bad[n - 6] ^= 1;
        fs::write(d.join("bad.tar.gz"), &bad).unwrap();
        assert!(matches!(unpack_tar_gz(&d.join("bad.tar.gz"), &d.join("bad"), Limits::default()), Err(TarError::Damaged(_))));
        fs::write(d.join("cut.tar.gz"), &fs::read(&gz).unwrap()[..n / 2]).unwrap();
        assert!(unpack_tar_gz(&d.join("cut.tar.gz"), &d.join("cut"), Limits::default()).is_err());
        fs::write(d.join("not.tar.gz"), b"<html>Not Found</html>, and then some more words").unwrap();
        assert!(matches!(unpack_tar_gz(&d.join("not.tar.gz"), &d.join("not"), Limits::default()), Err(TarError::NotGzip)));
        // The limits hold.
        assert!(matches!(unpack_tar_gz(&gz, &d.join("small"), Limits { total: 2000, entries: 100 }), Err(TarError::TooBig)));
        assert!(matches!(unpack_tar_gz(&gz, &d.join("few"), Limits { total: 1 << 20, entries: 3 }), Err(TarError::TooBig)));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn names_that_would_land_outside_are_refused() {
        for (tag, name) in [("abs", "/etc/turbinehub-test"), ("up", "../outside"), ("mid", "a/../../outside"), ("deep", "a/b/../../../outside")] {
            let (d, r) = unpack(tag, &[entry(b'0', name, "", b"x")]);
            assert!(matches!(r, Err(TarError::UnsafePath(_))), "{name}: {r:?}");
            assert!(!d.join("outside").exists());
            let _ = fs::remove_dir_all(&d);
        }
        // The same through a long name and through a pax record.
        let (d, r) = unpack("long", &[entry(b'L', "././@LongLink", "", b"../outside\0"), entry(b'0', "harmless", "", b"x")]);
        assert!(matches!(r, Err(TarError::UnsafePath(_))), "{r:?}");
        let _ = fs::remove_dir_all(&d);
        let (d, r) = unpack("pax", &[entry(b'x', "pax", "", b"19 path=../outside\n"), entry(b'0', "harmless", "", b"x")]);
        assert!(matches!(r, Err(TarError::UnsafePath(_))), "{r:?}");
        assert!(!d.join("outside").exists());
        let _ = fs::remove_dir_all(&d);
        // A pax record with a good name is used.
        let (d, r) = unpack("paxok", &[entry(b'x', "pax", "", b"20 path=dir/renamed\n"), entry(b'0', "short", "", b"x")]);
        assert_eq!(r.unwrap().files, 1);
        assert!(d.join("out/dir/renamed").is_file());
        let _ = fs::remove_dir_all(&d);
        assert!(safe_rel("a/./b//c").unwrap() == Path::new("a/b/c") && safe_rel("a\0b").is_err());
    }

    #[test]
    fn links_that_point_outside_are_refused() {
        for (tag, at, target) in [("link-abs", "top/link", "/etc"), ("link-up", "top/link", "../.."), ("link-up2", "link", ".."), ("mix", "top/a/link", "../x/../../../etc"), ("empty", "top/link", "")] {
            let (d, r) = unpack(tag, &[entry(b'5', "top/", "", b""), entry(b'2', at, target, b"")]);
            assert!(matches!(r, Err(TarError::UnsafeLink(_))), "{at} to {target}: {r:?}");
            let _ = fs::remove_dir_all(&d);
        }
        assert!(link_stays_inside(Path::new("top/files/lib64"), "lib") && link_stays_inside(Path::new("top/a/b"), "../../top/c") && link_stays_inside(Path::new("top/a"), ".."));
        assert!(!link_stays_inside(Path::new("top"), ".."));
        // A file under a link is never written: the link could lead anywhere by then.
        let (d, r) = unpack("through", &[entry(b'5', "top/", "", b""), entry(b'5', "top/real/", "", b""), entry(b'2', "top/way", "real", b""), entry(b'0', "top/way/file", "", b"x")]);
        assert!(matches!(r, Err(TarError::UnsafeLink(_))), "{r:?}");
        assert!(!d.join("out/top/real/file").exists());
        let _ = fs::remove_dir_all(&d);
        // Each link is fine by its name alone, but followed for real the second one ends above the folder: the
        // first goes up to the folder itself, and ".." after it leaves it.
        let (d, r) = unpack("chain", &[entry(b'5', "a/", "", b""), entry(b'5', "a/b/", "", b""), entry(b'2', "a/b/root", "../..", b""), entry(b'2', "sneak", "a/b/root/..", b"")]);
        assert!(matches!(r, Err(TarError::UnsafeLink(_))), "{r:?}");
        let _ = fs::remove_dir_all(&d);
        // A link is never replaced by a file, and a second name only names a real file inside.
        let (d, r) = unpack("twice", &[entry(b'2', "name", "elsewhere", b""), entry(b'0', "name", "", b"x")]);
        assert!(matches!(r, Err(TarError::Damaged(_))), "{r:?}");
        let _ = fs::remove_dir_all(&d);
        let (d, r) = unpack("hard", &[entry(b'1', "copy", "../../etc/hostname", b"")]);
        assert!(matches!(r, Err(TarError::UnsafePath(_))), "{r:?}");
        let _ = fs::remove_dir_all(&d);
        let (d, r) = unpack("hardok", &[entry(b'0', "one", "", b"same"), entry(b'1', "two", "one", b"")]);
        assert_eq!(r.unwrap().files, 2);
        assert_eq!(fs::read(d.join("out/two")).unwrap(), b"same");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn special_entries_and_broken_headers_are_refused() {
        for kind in *b"346S" {
            let (d, r) = unpack("special", &[entry(kind, "thing", "", b"")]);
            assert!(matches!(r, Err(TarError::Unsupported(_))), "{}: {r:?}", kind as char);
            let _ = fs::remove_dir_all(&d);
        }
        let d = dir("broken");
        let mut bytes = archive(&[entry(b'0', "file", "", b"data")]);
        bytes[0] = b'g';
        assert!(matches!(unpack_tar(&bytes[..], &d.join("sum"), Limits::default()), Err(TarError::Damaged(_))));
        let whole = archive(&[entry(b'0', "file", "", &[1u8; 2000])]);
        assert!(matches!(unpack_tar(&whole[..700], &d.join("cut"), Limits::default()), Err(TarError::Damaged(_))));
        assert_eq!(octal(b"0000644\0"), Some(0o644));
        assert_eq!(octal(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]), Some(256));
        assert_eq!(octal(b"12x"), None);
        let _ = fs::remove_dir_all(&d);
    }
}
