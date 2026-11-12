//! What the explorer's tests share: a temp folder that stands for "/", the made-up machine in it, and a disk with a
//! second drive that is really a second drive (its own device number, and no rename across).

use super::fs::{Disk, Fs, Map, Meta, Space};
use super::places::{Places, Who};
use super::When;
use crate::fake::{self, Preset};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Thursday 12 Nov 2026, 10:31:05 on a clock one hour ahead of UTC.
pub const NOW: When = When { secs: 1_794_475_865, offset: 3600 };

/// A temp folder, removed again at the end of the test.
pub struct Temp(pub PathBuf);

impl Temp {
    pub fn new(tag: &str) -> Temp {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("turbinehub-files-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Temp(std::fs::canonicalize(&d).unwrap())
    }
    /// Writes a file at a real path.
    pub fn write(&self, real: &str, data: &[u8]) {
        let p = self.at(real);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }
    pub fn dir(&self, real: &str) {
        std::fs::create_dir_all(self.at(real)).unwrap();
    }
    pub fn link(&self, target: &str, real: &str) {
        let p = self.at(real);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, p).unwrap();
    }
    /// Where a real path is in the temp folder.
    pub fn at(&self, real: &str) -> PathBuf {
        self.0.join(real.trim_start_matches('/'))
    }
    pub fn has(&self, real: &str) -> bool {
        std::fs::symlink_metadata(self.at(real)).is_ok()
    }
    pub fn read(&self, real: &str) -> String {
        String::from_utf8_lossy(&std::fs::read(self.at(real)).unwrap()).into_owned()
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        // A test may have left a folder without permissions behind.
        let _ = std::process::Command::new("chmod").args(["-R", "u+rwx"]).arg(&self.0).output();
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A disk whose folder `drive` is another drive: its own device number, and renames across fail the way the
/// system makes them fail.
pub struct TwoDrives {
    pub disk: Disk,
    pub drive: PathBuf,
}

impl TwoDrives {
    fn on(&self, p: &Path) -> bool {
        p.starts_with(&self.drive)
    }
}

impl Fs for TwoDrives {
    fn disk(&self, real: &Path) -> Option<PathBuf> {
        self.disk.disk(real)
    }
    fn real(&self, disk: &Path) -> Option<PathBuf> {
        self.disk.real(disk)
    }
    fn space(&self, real: &Path) -> Option<Space> {
        self.disk.space(real)
    }
    fn meta(&self, p: &Path) -> io::Result<Meta> {
        let mut m = self.disk.meta(p)?;
        if self.on(p) {
            m.dev = 4242;
        }
        Ok(m)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.on(from) != self.on(to) {
            return Err(io::Error::new(io::ErrorKind::CrossesDevices, "another drive"));
        }
        self.disk.rename(from, to)
    }
}

/// The made-up handheld with the explorer's files, in a temp folder: the folder, the disk over it and its places.
pub fn machine(tag: &str) -> (Temp, Disk, Places) {
    let t = Temp::new(tag);
    fake::machine(&t.0, Preset::Handheld);
    fake::files(&t.0, &t.at(fake::FILES_HOME));
    let disk = Disk { map: Map::under(&t.0), room: Some(Space { free: 229_780_000_000, total: 512_000_000_000 }) };
    let places = places(&t, &disk);
    (t, disk, places)
}

/// The places of the machine in a temp folder, read the way the app reads them.
pub fn places(t: &Temp, fs: &dyn Fs) -> Places {
    let mut who = Who::read(fs, Path::new(fake::FILES_HOME));
    who.own = vec![PathBuf::from("/home/deck/.var/app/io.github.ravenor907.TurbineHub")];
    let hardware = crate::hardware::Machine::read(&t.0, "", fake::HOME_DEV);
    Places::scan(fs, who, &hardware.drives)
}
