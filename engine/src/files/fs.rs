//! The one door to the disk. Everything the explorer reads or changes goes through `Fs`, with real paths (the ones
//! SteamOS itself uses, like /home/deck/Downloads). `Map` says where the app finds each real path: at its own place
//! on a plain system, under /run/host for the system folders inside the Flatpak, under a temp folder in tests.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

/// A path with "." and ".." worked out by its text alone and no doubled or trailing slashes. Always a full path.
pub fn clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::from("/");
    for c in p.components() {
        match c {
            Component::Normal(n) => out.push(n),
            Component::ParentDir => {
                out.pop();
            }
            _ => {}
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Folder,
    Link,
    /// A pipe, socket or device: shown, never copied.
    Other,
}

/// What the disk says about one name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Meta {
    pub kind: NodeKind,
    pub size: u64,
    /// Seconds since 1970.
    pub modified: u64,
    /// When it was made, where the file system keeps that.
    pub created: Option<u64>,
    /// The permission bits (with the sticky and set-id bits).
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    /// The file system it is on: two names with the same number can be renamed into each other.
    pub dev: u64,
}

impl Meta {
    fn of(m: &fs::Metadata) -> Meta {
        let t = m.file_type();
        let kind = if t.is_symlink() {
            NodeKind::Link
        } else if t.is_dir() {
            NodeKind::Folder
        } else if t.is_file() {
            NodeKind::File
        } else {
            NodeKind::Other
        };
        let created = m.created().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
        Meta { kind, size: m.len(), created, modified: m.mtime().max(0) as u64, mode: m.mode() & 0o7777, uid: m.uid(), gid: m.gid(), dev: m.dev() }
    }
    pub fn is_dir(&self) -> bool {
        self.kind == NodeKind::Folder
    }
}

/// Room on a drive, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Space {
    pub free: u64,
    pub total: u64,
}

/// Where the app finds each real path.
#[derive(Debug, Clone, Default)]
pub struct Map {
    /// (real folder, where it is for the app). The longest match wins.
    pairs: Vec<(PathBuf, PathBuf)>,
    /// Real folders the app can't reach at all.
    closed: Vec<PathBuf>,
    /// Folders inside a closed one that it can reach after all.
    open: Vec<PathBuf>,
    /// Nothing outside `pairs` exists (tests and the made-up machine).
    only: bool,
}

/// The host's system folders a Flatpak finds under /run/host.
pub const HOST_FOLDERS: &[&str] = &["usr", "etc", "bin", "sbin", "lib", "lib32", "lib64"];

impl Map {
    /// Every path is where it says it is.
    pub fn plain() -> Map {
        Map::default()
    }

    /// Inside the Flatpak with `--filesystem=host`: the host's /usr and /etc (and the links into /usr) are under
    /// /run/host, the home folder and the drives are at their own paths, and the rest of the top of the disk is the
    /// sandbox's own and says nothing about the machine.
    pub fn sandbox() -> Map {
        Map {
            pairs: HOST_FOLDERS.iter().map(|n| (PathBuf::from(format!("/{n}")), PathBuf::from(format!("/run/host/{n}")))).collect(),
            closed: ["/app", "/boot", "/dev", "/efi", "/proc", "/root", "/run", "/sys", "/tmp", "/var", "/.flatpak-info"].iter().map(PathBuf::from).collect(),
            open: ["/run/media", "/var/lib/flatpak"].iter().map(PathBuf::from).collect(),
            only: false,
        }
    }

    /// Everything lives under `root`, which stands for "/".
    pub fn under(root: &Path) -> Map {
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Map { pairs: vec![(PathBuf::from("/"), root)], closed: Vec::new(), open: Vec::new(), only: true }
    }

    /// And this real folder is found at `disk`.
    pub fn with(mut self, real: &Path, disk: &Path) -> Map {
        let disk = fs::canonicalize(disk).unwrap_or_else(|_| disk.to_path_buf());
        self.pairs.push((clean(real), disk));
        self
    }

    /// The map for where the app runs now.
    pub fn here() -> Map {
        if in_sandbox() { Map::sandbox() } else { Map::plain() }
    }

    /// Where the app finds this real path, or None when it can't reach it.
    pub fn disk(&self, real: &Path) -> Option<PathBuf> {
        if !real.is_absolute() {
            return None;
        }
        let real = clean(real);
        let best = self.pairs.iter().filter(|(r, _)| real.starts_with(r)).max_by_key(|(r, _)| r.components().count());
        if let Some((r, d)) = best {
            let rest = real.strip_prefix(r).ok()?;
            return Some(if rest.as_os_str().is_empty() { d.clone() } else { d.join(rest) });
        }
        let shut = self.closed.iter().any(|c| real.starts_with(c)) && !self.open.iter().any(|o| real.starts_with(o));
        (!shut && !self.only).then_some(real)
    }

    /// The real path of something the app found at `disk`.
    pub fn real(&self, disk: &Path) -> Option<PathBuf> {
        let best = self.pairs.iter().filter(|(_, d)| disk.starts_with(d)).max_by_key(|(_, d)| d.components().count());
        if let Some((r, d)) = best {
            let rest = disk.strip_prefix(d).ok()?;
            return Some(if rest.as_os_str().is_empty() { r.clone() } else { r.join(rest) });
        }
        (!self.only && disk.is_absolute()).then(|| disk.to_path_buf())
    }
}

/// Is this program running inside a Flatpak?
pub fn in_sandbox() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn unreachable() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "Turbine Hub can't reach this place")
}

/// Every way the explorer touches the disk. Paths are real paths. A type only has to say where things are and how
/// much room there is; the rest is the same everywhere (and a test may swap any of it).
pub trait Fs {
    /// Where the app finds this real path.
    fn disk(&self, real: &Path) -> Option<PathBuf>;
    /// The real path of something the app found.
    fn real(&self, disk: &Path) -> Option<PathBuf>;
    /// Free and total room on the drive that holds this path.
    fn space(&self, real: &Path) -> Option<Space>;

    fn at(&self, real: &Path) -> io::Result<PathBuf> {
        self.disk(real).ok_or_else(unreachable)
    }
    /// The name itself: a link is a link.
    fn meta(&self, p: &Path) -> io::Result<Meta> {
        fs::symlink_metadata(self.at(p)?).map(|m| Meta::of(&m))
    }
    /// What the name leads to, links followed.
    fn follow(&self, p: &Path) -> io::Result<Meta> {
        self.meta(&self.canonical(p)?)
    }
    /// The names in a folder, as they come. Names the app can't reach are left out.
    fn names(&self, p: &Path) -> io::Result<Vec<OsString>> {
        let mut out = Vec::new();
        for e in fs::read_dir(self.at(p)?)? {
            // One entry that can't be read never hides the others.
            let Ok(e) = e else { continue };
            let name = e.file_name();
            if self.disk(&p.join(&name)).is_some() {
                out.push(name);
            }
        }
        Ok(out)
    }
    /// What a link says, as written.
    fn link_target(&self, p: &Path) -> io::Result<PathBuf> {
        fs::read_link(self.at(p)?)
    }
    /// The real path with every link followed, worked out in real paths (so a link to /usr leads to the machine's
    /// /usr wherever the app finds that). Fails when a piece isn't there.
    fn canonical(&self, p: &Path) -> io::Result<PathBuf> {
        let parts = |p: &Path| -> Vec<OsString> {
            p.components().rev().filter_map(|c| match c {
                Component::Normal(n) => Some(n.to_os_string()),
                Component::ParentDir => Some("..".into()),
                _ => None,
            }).collect()
        };
        let mut todo = parts(p);
        let mut out = PathBuf::from("/");
        let mut hops = 0;
        while let Some(c) = todo.pop() {
            if c == ".." {
                out.pop();
                continue;
            }
            let next = out.join(&c);
            if self.meta(&next)?.kind != NodeKind::Link {
                out = next;
                continue;
            }
            hops += 1;
            if hops > 40 {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "links that lead in a circle"));
            }
            let target = self.link_target(&next)?;
            if target.is_absolute() {
                out = PathBuf::from("/");
            }
            todo.extend(parts(&target));
        }
        Ok(out)
    }
    /// The first `max` bytes of a file.
    fn head(&self, p: &Path, max: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        fs::File::open(self.at(p)?)?.take(max as u64).read_to_end(&mut out)?;
        Ok(out)
    }
    fn make_dir(&self, p: &Path) -> io::Result<()> {
        fs::create_dir(self.at(p)?)
    }
    fn make_dirs(&self, p: &Path) -> io::Result<()> {
        fs::create_dir_all(self.at(p)?)
    }
    /// Renames without ever replacing: fails when `to` is there already.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let (from, to) = (self.at(from)?, self.at(to)?);
        if fs::symlink_metadata(&to).is_ok() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "there is something with that name already"));
        }
        fs::rename(from, to)
    }
    /// Copies one plain file to a name that isn't taken, with its permissions and date. `tick` hears about every
    /// piece written and stops the copy by answering false (what was written is removed again).
    fn copy_file(&self, from: &Path, to: &Path, tick: &mut dyn FnMut(u64) -> bool) -> io::Result<u64> {
        let (src, dst) = (self.at(from)?, self.at(to)?);
        let mut input = fs::File::open(&src)?;
        let meta = input.metadata()?;
        let mut output = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&dst)?;
        let mut buf = vec![0u8; 1 << 20];
        let mut total = 0u64;
        let copied = (|| -> io::Result<()> {
            loop {
                let n = input.read(&mut buf)?;
                if n == 0 {
                    return Ok(());
                }
                output.write_all(&buf[..n])?;
                total += n as u64;
                if !tick(n as u64) {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"));
                }
            }
        })();
        if let Err(e) = copied {
            drop(output);
            let _ = fs::remove_file(&dst);
            return Err(e);
        }
        let _ = output.set_modified(UNIX_EPOCH + Duration::from_secs(meta.mtime().max(0) as u64));
        let _ = output.set_permissions(fs::Permissions::from_mode(meta.mode() & 0o777));
        Ok(total)
    }
    fn make_link(&self, target: &Path, at: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(target, self.at(at)?)
    }
    /// Removes one file or one link (the link, never what it points to).
    fn remove_file(&self, p: &Path) -> io::Result<()> {
        fs::remove_file(self.at(p)?)
    }
    /// Removes one empty folder.
    fn remove_dir(&self, p: &Path) -> io::Result<()> {
        fs::remove_dir(self.at(p)?)
    }
    /// Writes a new file; fails when the name is taken.
    fn create_new(&self, p: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut f = fs::OpenOptions::new().write(true).create_new(true).open(self.at(p)?)?;
        f.write_all(bytes)?;
        f.sync_all()
    }
    /// Writes a small file whole.
    fn write(&self, p: &Path, bytes: &[u8]) -> io::Result<()> {
        fs::write(self.at(p)?, bytes)
    }
    fn set_mode(&self, p: &Path, mode: u32) -> io::Result<()> {
        fs::set_permissions(self.at(p)?, fs::Permissions::from_mode(mode & 0o7777))
    }
    fn set_modified(&self, p: &Path, secs: u64) -> io::Result<()> {
        fs::File::open(self.at(p)?)?.set_modified(UNIX_EPOCH + Duration::from_secs(secs))
    }
}

/// The disk as the app sees it.
#[derive(Debug, Clone, Default)]
pub struct Disk {
    pub map: Map,
    /// Room to report for every drive (tests and the made-up machine); None asks the system.
    pub room: Option<Space>,
}

impl Disk {
    /// The disk of the machine the app runs on.
    pub fn here() -> Disk {
        Disk { map: Map::here(), room: None }
    }
    /// A temp folder standing for "/".
    pub fn under(root: &Path) -> Disk {
        Disk { map: Map::under(root), room: None }
    }
}

/// What `df -B1 --output=avail,size <path>` printed, as room.
pub fn space_from_df(text: &str) -> Option<Space> {
    let mut nums = text.lines().nth(1)?.split_whitespace().map(|n| n.parse::<u64>());
    Some(Space { free: nums.next()?.ok()?, total: nums.next()?.ok()? })
}

impl Fs for Disk {
    fn disk(&self, real: &Path) -> Option<PathBuf> {
        self.map.disk(real)
    }
    fn real(&self, disk: &Path) -> Option<PathBuf> {
        self.map.real(disk)
    }
    fn space(&self, real: &Path) -> Option<Space> {
        if self.room.is_some() {
            return self.room;
        }
        // std has no call for this; df is in the Flatpak's runtime and on every system.
        let out = std::process::Command::new("df").args(["-B1", "--output=avail,size"]).arg(self.disk(real)?).output().ok()?;
        space_from_df(&String::from_utf8_lossy(&out.stdout))
    }
}

/// Removes a file, a link or a whole folder. A link is removed as a link: nothing it points to is ever touched.
pub fn remove_tree(fs: &dyn Fs, p: &Path) -> io::Result<()> {
    if fs.meta(p)?.kind != NodeKind::Folder {
        return fs.remove_file(p);
    }
    for name in fs.names(p)? {
        remove_tree(fs, &p.join(name))?;
    }
    fs.remove_dir(p)
}

/// How many files and bytes a file or folder holds. Links count as themselves.
pub fn measure(fs: &dyn Fs, p: &Path) -> (u64, u64) {
    let Ok(m) = fs.meta(p) else { return (0, 0) };
    if m.kind != NodeKind::Folder {
        return (1, if m.kind == NodeKind::File { m.size } else { 0 });
    }
    let mut total = (1, 0);
    for name in fs.names(p).unwrap_or_default() {
        let (f, b) = measure(fs, &p.join(name));
        total = (total.0 + f, total.1 + b);
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_works_out_dots_and_slashes() {
        assert_eq!(clean(Path::new("/home/deck/../deck//Downloads/./a/")), PathBuf::from("/home/deck/Downloads/a"));
        assert_eq!(clean(Path::new("/../..")), PathBuf::from("/"));
        assert_eq!(clean(Path::new("/")), PathBuf::from("/"));
    }

    #[test]
    fn the_sandbox_finds_the_system_under_run_host_and_the_rest_in_place() {
        let m = Map::sandbox();
        assert_eq!(m.disk(Path::new("/usr/share/fonts")), Some("/run/host/usr/share/fonts".into()));
        assert_eq!(m.disk(Path::new("/etc")), Some("/run/host/etc".into()));
        assert_eq!(m.disk(Path::new("/etc/os-release")), Some("/run/host/etc/os-release".into()));
        assert_eq!(m.disk(Path::new("/bin/ls")), Some("/run/host/bin/ls".into()));
        assert_eq!(m.disk(Path::new("/home/deck/Downloads/a b.txt")), Some("/home/deck/Downloads/a b.txt".into()));
        assert_eq!(m.disk(Path::new("/run/media/deck/SDCARD")), Some("/run/media/deck/SDCARD".into()));
        assert_eq!(m.disk(Path::new("/var/lib/flatpak/app")), Some("/var/lib/flatpak/app".into()));
        assert_eq!(m.disk(Path::new("/")), Some("/".into()));
        // The sandbox's own: not the machine's, so not shown as if it were.
        for p in ["/var/log", "/tmp/x", "/proc/1", "/app/bin", "/run/user/1000", "/sys/class", "/dev/sda"] {
            assert_eq!(m.disk(Path::new(p)), None, "{p}");
        }
        // Dots never get around the map.
        assert_eq!(m.disk(Path::new("/home/../usr/bin")), Some("/run/host/usr/bin".into()));
        assert_eq!(m.disk(Path::new("usr/bin")), None);
        // And back.
        assert_eq!(m.real(Path::new("/run/host/usr/share/fonts")), Some("/usr/share/fonts".into()));
        assert_eq!(m.real(Path::new("/run/host/etc")), Some("/etc".into()));
        assert_eq!(m.real(Path::new("/home/deck/x")), Some("/home/deck/x".into()));
        for p in ["/usr/lib/os-release", "/etc/fstab", "/home/deck/Désktop/ü", "/lib64/x", "/run/media/deck/A B"] {
            assert_eq!(m.real(&m.disk(Path::new(p)).unwrap()), Some(PathBuf::from(p)), "{p}");
        }
    }

    #[test]
    fn a_plain_system_maps_to_itself_and_a_temp_root_holds_everything() {
        let m = Map::plain();
        assert_eq!(m.disk(Path::new("/usr/bin")), Some("/usr/bin".into()));
        assert_eq!(m.real(Path::new("/proc/1")), Some("/proc/1".into()));
        let t = Map::under(Path::new("/nowhere/root")).with(Path::new("/home/deck"), Path::new("/nowhere/home"));
        assert_eq!(t.disk(Path::new("/usr/bin")), Some("/nowhere/root/usr/bin".into()));
        assert_eq!(t.disk(Path::new("/home/deck/Desktop")), Some("/nowhere/home/Desktop".into()));
        assert_eq!(t.disk(Path::new("/home/deck")), Some("/nowhere/home".into()));
        assert_eq!(t.disk(Path::new("/home")), Some("/nowhere/root/home".into()));
        assert_eq!(t.real(Path::new("/nowhere/home/Desktop/a")), Some("/home/deck/Desktop/a".into()));
        assert_eq!(t.real(Path::new("/nowhere/root")), Some("/".into()));
        assert_eq!(t.real(Path::new("/usr/bin")), None);
    }

    #[test]
    fn df_output_reads_as_room() {
        assert_eq!(space_from_df("  Avail 1B-blocks\n229780000000 512000000000\n"), Some(Space { free: 229_780_000_000, total: 512_000_000_000 }));
        assert_eq!(space_from_df("df: no such file\n"), None);
    }
}
