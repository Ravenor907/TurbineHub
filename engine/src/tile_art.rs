//! Pictures for library entries that have none. A program added to Steam as a non-Steam game shows as a blank
//! grey tile until somebody gives it pictures. Turbine Hub draws them on the machine, from the entry's own name
//! and the color of its icon: a cover (600x900), a wide capsule (920x430) and a hero (1920x620), and an icon for
//! an entry that has none. Nothing is downloaded.
//!
//! The look is Turbine Hub's: its dark ground, a soft glow in a color taken from the entry's icon (a jade, brass or
//! other quiet tone picked by the name when no icon can be read), the icon itself, and the name set in a small
//! built-in face of square dots.
//!
//! Only what is missing is drawn: a picture the player set (in any format Steam reads) is never replaced, kind by
//! kind. Steam's grid folder gets the files; the entry's icon is set in Steam's list of non-Steam games, which is
//! why all of it happens only while Steam is closed (see `shortcuts::Steam`). Every file written is kept in a
//! record with its SHA-256, and Undo removes exactly those files, each only while it is still what Turbine Hub
//! wrote.

use crate::png::{self, Image};
use crate::sha256;
use crate::shortcuts::{self, Steam};
use crate::steam_art::{self, ArtRecord};
use crate::vdf::{self, Vdf};
use std::fs;
use std::path::{Path, PathBuf};

/// The pictures an entry needs to stop being blank: (the piece's name in `steam_art::PARTS`, width, height).
pub const KINDS: &[(&str, u32, u32)] = &[("capsule", 600, 900), ("wide", 920, 430), ("hero", 1920, 620)];
/// The side of an icon Turbine Hub draws.
pub const ICON_SIDE: u32 = 256;

/// One non-Steam game of the signed-in account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub appid: u32,
    pub name: String,
    /// The icon the entry names (a file's path), "" for none.
    pub icon: String,
    /// The pieces of `KINDS` it has no picture for.
    pub missing: Vec<&'static str>,
}

/// Steam's list of non-Steam games for the signed-in account, when it has one.
pub fn list_file(home: &Path) -> Option<PathBuf> {
    shortcuts::tile_file(home).filter(|f| f.is_file())
}

/// Is a picture for this piece in the grid folder, in any format Steam reads?
fn has(grid: &Path, appid: u32, part: &str) -> bool {
    steam_art::grid_name(appid, part).is_some_and(|name| ["png", "jpg", "jpeg", "webp", "PNG", "JPG"].iter().any(|ext| grid.join(name.replace(".png", &format!(".{ext}"))).exists()))
}

/// Every non-Steam game in that list with what it lacks. Turbine Hub's own entries are left out: they get
/// Turbine Hub's own art (see `ui::selftile`).
pub fn entries(home: &Path) -> Vec<Entry> {
    let Some(file) = list_file(home) else { return Vec::new() };
    let Some(root) = fs::read(&file).ok().filter(|b| b.len() <= 16 << 20).and_then(|b| vdf::parse_binary(&b).ok()) else { return Vec::new() };
    let grid = steam_art::grid_dir(&file);
    let text = |v: &Vdf, key: &str| v.get(key).and_then(Vdf::as_str).unwrap_or("").to_string();
    let mut out = Vec::new();
    for (_, e) in root.get("shortcuts").map(Vdf::entries).unwrap_or(&[]) {
        if !matches!(e, Vdf::Obj(_)) {
            continue;
        }
        let (name, exe) = (text(e, "AppName"), text(e, "Exe"));
        let s = shortcuts::Shortcut { appid: text(e, "appid").parse::<u32>().ok().filter(|a| *a != 0).unwrap_or_else(|| shortcuts::legacy_appid(&exe, &name)), name, exe, launch_options: text(e, "LaunchOptions") };
        if shortcuts::is_self(&s) || s.name.trim().is_empty() {
            continue;
        }
        let missing = KINDS.iter().map(|(part, _, _)| *part).filter(|part| !has(&grid, s.appid, part)).collect();
        out.push(Entry { appid: s.appid, name: s.name, icon: text(e, "icon"), missing });
    }
    out
}

/// The entries with at least one picture missing: what Fix works on.
pub fn to_fix(home: &Path) -> Vec<Entry> {
    entries(home).into_iter().filter(|e| !e.missing.is_empty()).collect()
}

/// Where the record of the files Turbine Hub wrote is kept.
pub fn record(folder: &Path) -> PathBuf {
    folder.join("tile-art.json")
}

/// How many files of the record are still there, exactly as Turbine Hub wrote them: what Undo would remove.
pub fn undoable(folder: &Path) -> usize {
    ArtRecord::load(&record(folder)).files.iter().filter(|(p, h)| sha256::file_hex(Path::new(p)).is_ok_and(|now| sha256::same_digest(&now, h))).count()
}

// ---- Drawing ----

/// Turbine Hub's dark ground and the color of its words.
const GROUND: [u8; 3] = [11, 10, 8];
const INK: [u8; 3] = [234, 223, 198];
/// The glow for an entry whose icon gives no color: quiet tones, picked by the entry's name so it keeps its own.
const TONES: &[[u8; 3]] = &[[111, 174, 140], [201, 152, 84], [110, 150, 212], [196, 112, 142], [150, 132, 214], [98, 180, 190]];

/// A picture being drawn: three bytes a pixel.
struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Canvas {
        Canvas { w, h, px: GROUND.repeat((w * h) as usize) }
    }

    /// Mixes a color into one pixel: `a` from 0 (nothing) to 1 (all of it).
    fn mix(&mut self, x: i64, y: i64, c: [u8; 3], a: f32) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 || a <= 0.0 {
            return;
        }
        let at = (y as usize * self.w as usize + x as usize) * 3;
        for (p, c) in self.px[at..at + 3].iter_mut().zip(c) {
            let old = *p as f32;
            *p = (old + (c as f32 - old) * a.min(1.0)).round() as u8;
        }
    }

    /// A soft round glow: strongest at its middle, gone at about its radius.
    fn glow(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: [u8; 3], strength: f32) {
        for y in 0..self.h as i64 {
            for x in 0..self.w as i64 {
                let (dx, dy) = ((x as f32 - cx) / rx, (y as f32 - cy) / ry);
                let d = dx * dx + dy * dy;
                if d < 1.6 {
                    self.mix(x, y, c, strength * (-3.2 * d).exp());
                }
            }
        }
    }

    fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: [u8; 3]) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.mix(xx, yy, c, 1.0);
            }
        }
    }

    /// The entry's icon, fitted into a square of `side` with its middle at (cx, cy), see-through parts kept.
    fn icon(&mut self, img: &Image, cx: i64, cy: i64, side: i64) {
        let (iw, ih) = (img.width as f32, img.height as f32);
        let scale = side as f32 / iw.max(ih);
        let (w, h) = ((iw * scale).round() as i64, (ih * scale).round() as i64);
        let at = |x: i64, y: i64| -> [f32; 4] {
            let (x, y) = (x.clamp(0, img.width as i64 - 1) as usize, y.clamp(0, img.height as i64 - 1) as usize);
            let p = &img.rgba[(y * img.width as usize + x) * 4..][..4];
            [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]
        };
        for y in 0..h {
            for x in 0..w {
                // Between the four nearest pixels of the icon, so a small icon grows smooth and not blocky.
                let (sx, sy) = ((x as f32 + 0.5) / scale - 0.5, (y as f32 + 0.5) / scale - 0.5);
                let (x0, y0) = (sx.floor() as i64, sy.floor() as i64);
                let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
                let mut sum = [0f32; 4];
                for (px, py, wgt) in [(x0, y0, (1.0 - fx) * (1.0 - fy)), (x0 + 1, y0, fx * (1.0 - fy)), (x0, y0 + 1, (1.0 - fx) * fy), (x0 + 1, y0 + 1, fx * fy)] {
                    let p = at(px, py);
                    let a = p[3] / 255.0 * wgt;
                    sum[0] += p[0] * a;
                    sum[1] += p[1] * a;
                    sum[2] += p[2] * a;
                    sum[3] += a;
                }
                if sum[3] > 0.003 {
                    let c = [(sum[0] / sum[3]).round() as u8, (sum[1] / sum[3]).round() as u8, (sum[2] / sum[3]).round() as u8];
                    self.mix(cx - w / 2 + x, cy - h / 2 + y, c, sum[3]);
                }
            }
        }
    }

    /// Lines of words with their middle at (cx, cy), each dot of a letter `scale` pixels wide.
    fn words(&mut self, lines: &[String], cx: i64, cy: i64, scale: i64, c: [u8; 3]) {
        let line_h = 10 * scale;
        let top = cy - (lines.len() as i64 * line_h - 3 * scale) / 2;
        for (n, line) in lines.iter().enumerate() {
            let width = line.chars().count() as i64 * 6 * scale - scale;
            let mut x = cx - width / 2;
            for ch in line.chars() {
                if let Some(rows) = glyph(ch) {
                    for (r, bits) in rows.iter().enumerate() {
                        for col in 0..5 {
                            if bits & (0b10000 >> col) != 0 {
                                self.rect(x + col * scale, top + n as i64 * line_h + r as i64 * scale, scale, scale, c);
                            }
                        }
                    }
                }
                x += 6 * scale;
            }
        }
    }

    fn png(&self) -> Vec<u8> {
        png::encode_rgb(self.w, self.h, &self.px)
    }
}

/// The built-in face: capitals, digits and a few marks, each five dots wide and seven high (one row a byte, the
/// leftmost dot the highest of five bits).
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c {
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x0C, 0x04, 0x08],
        ':' => [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00],
        '\'' => [0x04, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00],
        '!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04],
        '?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        '&' => [0x0C, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0D],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        '/' => [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        ' ' => [0; 7],
        _ => return None,
    })
}

/// A name in the letters the built-in face has: capitals, accents dropped from the letters that carry them, the
/// long dashes as a hyphen, and whatever else it has no letter for left out.
pub fn plain_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_uppercase) {
        let c = match c {
            'À'..='Å' => 'A',
            'Ç' => 'C',
            'È'..='Ë' => 'E',
            'Ì'..='Ï' => 'I',
            'Ñ' => 'N',
            'Ò'..='Ö' | 'Ø' => 'O',
            'Ù'..='Ü' => 'U',
            'Ý' => 'Y',
            '_' | '\t' => ' ',
            '\u{2010}'..='\u{2015}' => '-',
            '\u{2018}' | '\u{2019}' => '\'',
            c => c,
        };
        if glyph(c).is_some() {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Breaks a name into at most `max_lines` lines of at most `per_line` letters, at its spaces where it can. None
/// when it doesn't fit; with `force` a word that is too long is cut and whatever is left over is dropped.
fn wrap(name: &str, per_line: usize, max_lines: usize, force: bool) -> Option<Vec<String>> {
    let mut lines: Vec<String> = Vec::new();
    for word in name.split(' ') {
        let mut word: Vec<char> = word.chars().collect();
        if word.len() > per_line && !force {
            return None;
        }
        while !word.is_empty() {
            let take = word.len().min(per_line);
            let piece: String = word.drain(..take).collect();
            match lines.last_mut() {
                Some(last) if last.chars().count() + 1 + piece.chars().count() <= per_line => {
                    last.push(' ');
                    last.push_str(&piece);
                }
                _ => lines.push(piece),
            }
        }
    }
    if lines.len() > max_lines {
        if !force {
            return None;
        }
        lines.truncate(max_lines);
    }
    Some(lines)
}

/// The biggest size of letters (from `biggest` down to `smallest` pixels a dot) at which the name fits `width` in
/// `max_lines` lines, with its lines.
fn fit(name: &str, width: i64, max_lines: usize, biggest: i64, smallest: i64) -> (i64, Vec<String>) {
    for scale in (smallest..=biggest).rev() {
        let per_line = ((width + scale) / (6 * scale)).max(1) as usize;
        if let Some(lines) = wrap(name, per_line, max_lines, scale == smallest) {
            return (scale, lines);
        }
    }
    (smallest, Vec::new())
}

/// The color an icon is mostly in: its solid pixels, the colorful ones counting more than the grey ones, brought
/// to a brightness that glows on the dark ground. None for an icon without solid pixels.
pub fn icon_color(img: &Image) -> Option<[u8; 3]> {
    let (mut sum, mut total) = ([0f64; 3], 0f64);
    for p in img.rgba.chunks_exact(4).filter(|p| p[3] >= 128) {
        let (hi, lo) = (p[..3].iter().max().copied().unwrap_or(0), p[..3].iter().min().copied().unwrap_or(0));
        let weight = 1.0 + (hi - lo) as f64 / 8.0;
        for (s, v) in sum.iter_mut().zip(p) {
            *s += *v as f64 * weight;
        }
        total += weight;
    }
    if total <= 0.0 {
        return None;
    }
    let c = [sum[0] / total, sum[1] / total, sum[2] / total];
    let hi = c[0].max(c[1]).max(c[2]);
    // A black icon has no color to take; it gets a quiet grey glow.
    if hi < 8.0 {
        return Some([120, 120, 120]);
    }
    let lift = 210.0 / hi;
    Some([(c[0] * lift).round().min(255.0) as u8, (c[1] * lift).round().min(255.0) as u8, (c[2] * lift).round().min(255.0) as u8])
}

/// The quiet tone an entry gets when its icon gives no color: always the same one for the same name.
pub fn tone_for(name: &str) -> [u8; 3] {
    TONES[crate::zip::crc32(name.as_bytes()) as usize % TONES.len()]
}

/// Reads an entry's icon when it is a PNG, or a Windows .ico that keeps a PNG inside. Program files (.exe) and
/// the old bitmap icons aren't read: the entry then gets its tone by name.
pub fn read_icon(path: &str) -> Option<Image> {
    let path = path.trim().trim_matches('"');
    if path.is_empty() {
        return None;
    }
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > 8 << 20 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    png::decode(png::from_ico(&bytes).unwrap_or(&bytes))
}

/// A lighter shade of a color, for the thin rule under a name.
fn lighter(c: [u8; 3]) -> [u8; 3] {
    c.map(|v| (v as u16 + (255 - v as u16) / 3) as u8)
}

/// Draws one picture for an entry and answers it as a PNG. `kind`: "capsule", "wide", "hero" or "icon".
pub fn draw(kind: &str, name: &str, color: [u8; 3], icon: Option<&Image>) -> Vec<u8> {
    let name = plain_name(name);
    match kind {
        "capsule" => {
            let mut c = Canvas::new(600, 900);
            c.glow(300.0, 330.0, 430.0, 470.0, color, 0.55);
            c.glow(520.0, 860.0, 330.0, 260.0, color, 0.16);
            let (scale, lines) = fit(&name, 500, 4, 10, 3);
            let text_y = if icon.is_some() { 620 } else { 450 };
            if let Some(img) = icon {
                c.icon(img, 300, 320, 220);
            }
            c.words(&lines, 300, text_y, scale, INK);
            if !lines.is_empty() {
                c.rect(300 - 36, text_y + lines.len() as i64 * 5 * scale + 2 * scale, 72, 4, lighter(color));
            }
            c.png()
        }
        "wide" => {
            let mut c = Canvas::new(920, 430);
            c.glow(460.0, 150.0, 560.0, 320.0, color, 0.55);
            let (scale, lines) = fit(&name, 800, 2, 8, 3);
            let text_y = if icon.is_some() { 310 } else { 215 };
            if let Some(img) = icon {
                c.icon(img, 460, 140, 150);
            }
            c.words(&lines, 460, text_y, scale, INK);
            if !lines.is_empty() {
                c.rect(460 - 36, text_y + lines.len() as i64 * 5 * scale + 2 * scale, 72, 4, lighter(color));
            }
            c.png()
        }
        "hero" => {
            // Steam sets the entry's name over the hero itself, so this one is the ground and its glow only.
            let mut c = Canvas::new(1920, 620);
            c.glow(1380.0, 180.0, 1000.0, 560.0, color, 0.5);
            c.glow(260.0, 620.0, 700.0, 380.0, color, 0.14);
            c.png()
        }
        _ => {
            let mut c = Canvas::new(ICON_SIDE, ICON_SIDE);
            c.glow(128.0, 110.0, 190.0, 190.0, color, 0.6);
            // The first letters of the first two words.
            let initials: String = name.split(' ').filter_map(|w| w.chars().find(|ch| ch.is_alphanumeric())).take(2).collect();
            let scale = if initials.chars().count() > 1 { 18 } else { 24 };
            c.words(&[initials], 128, 128, scale, INK);
            c.png()
        }
    }
}

// ---- Fix and Undo ----

/// What a Fix did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fixed {
    /// The entries that got at least one picture.
    pub entries: Vec<String>,
    /// How many files were written.
    pub files: usize,
}

/// Draws and writes what the entries lack. Only while Steam is closed. A picture that appeared since the list was
/// made (the player set one meanwhile) is left as it is.
pub fn fix(home: &Path, folder: &Path, steam: Steam) -> Result<Fixed, String> {
    let todo = to_fix(home);
    if todo.is_empty() {
        return Ok(Fixed::default());
    }
    steam.may_write()?;
    let file = list_file(home).ok_or("Steam's list of non-Steam games wasn't found. Start Steam once and sign in.")?;
    let grid = steam_art::grid_dir(&file);
    let mut rec = ArtRecord::load(&record(folder));
    let mut done = Fixed::default();
    let mut icons: Vec<(u32, String)> = Vec::new();
    let write = |rec: &mut ArtRecord, dest: &Path, bytes: &[u8]| -> Result<(), String> {
        crate::write_atomic(dest, bytes).map_err(|e| format!("Couldn't write a picture into Steam's folder for library pictures: {e}"))?;
        let key = dest.to_string_lossy().into_owned();
        rec.files.retain(|(p, _)| *p != key);
        rec.files.push((key, sha256::digest_hex(bytes)));
        Ok(())
    };
    let mut result = Ok(());
    'all: for e in &todo {
        let icon = read_icon(&e.icon);
        let color = icon.as_ref().and_then(icon_color).unwrap_or_else(|| tone_for(&e.name));
        let before = done.files;
        for part in &e.missing {
            let Some(name) = steam_art::grid_name(e.appid, part) else { continue };
            if has(&grid, e.appid, part) {
                continue;
            }
            if let Err(why) = write(&mut rec, &grid.join(name), &draw(part, &e.name, color, icon.as_ref())) {
                result = Err(why);
                break 'all;
            }
            done.files += 1;
        }
        // An entry without an icon of its own gets one, set in Steam's list below.
        if e.icon.trim().is_empty()
            && let Some(name) = steam_art::grid_name(e.appid, "icon")
            && !has(&grid, e.appid, "icon")
        {
            let dest = grid.join(name);
            if let Err(why) = write(&mut rec, &dest, &draw("icon", &e.name, color, None)) {
                result = Err(why);
                break 'all;
            }
            done.files += 1;
            icons.push((e.appid, dest.to_string_lossy().into_owned()));
        }
        if done.files > before {
            done.entries.push(e.name.clone());
        }
    }
    // The record first, whatever happened: Undo has to know every file that was written.
    rec.save(&record(folder)).map_err(|e| format!("Couldn't keep the list of pictures written: {e}"))?;
    result?;
    if !icons.is_empty() {
        shortcuts::set_icons(&file, &|appid, now| if now.trim().is_empty() { icons.iter().find(|(id, _)| *id == appid).map(|(_, p)| p.clone()) } else { None }, steam)?;
    }
    Ok(done)
}

/// Takes back what Fix wrote: every file in the record that is still exactly what Turbine Hub wrote (one the
/// player replaced since stays), and the icons it set in Steam's list. Only while Steam is closed. Answers how
/// many files were removed.
pub fn undo(home: &Path, folder: &Path, steam: Steam) -> Result<usize, String> {
    let mut rec = ArtRecord::load(&record(folder));
    if rec.files.is_empty() {
        return Ok(0);
    }
    steam.may_write()?;
    let ours: Vec<String> = rec.files.iter().map(|(p, _)| p.clone()).collect();
    if let Some(file) = list_file(home) {
        shortcuts::set_icons(&file, &|_, now| ours.iter().any(|p| p == now).then(String::new), steam)?;
    }
    let mut removed = 0;
    for (path, hash) in std::mem::take(&mut rec.files) {
        let p = Path::new(&path);
        if sha256::file_hex(p).is_ok_and(|now| sha256::same_digest(&now, &hash)) {
            match fs::remove_file(p) {
                Ok(()) => removed += 1,
                Err(_) => rec.files.push((path, hash)),
            }
        }
    }
    rec.save(&record(folder)).map_err(|e| format!("Couldn't keep the list of pictures written: {e}"))?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::NewShortcut;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("turbinehub-tileart-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn entry(name: &str, icon: &str) -> NewShortcut {
        NewShortcut { name: name.into(), exe: format!("\"/home/deck/Games/{name}/start.sh\""), start_dir: "\"/home/deck/Games/\"".into(), launch_options: String::new(), flatpak_id: String::new(), icon: icon.into() }
    }

    /// A made-up userdata: three entries of the player's (one with an icon to read, one with a cover already),
    /// and Turbine Hub's own.
    fn home(tag: &str) -> (PathBuf, PathBuf, PathBuf, Vec<u32>) {
        let d = dir(tag);
        let home = d.join("home");
        let cfg = home.join(".local/share/Steam/userdata/1001/config");
        fs::create_dir_all(cfg.join("grid")).unwrap();
        let file = cfg.join("shortcuts.vdf");
        let icon = d.join("battle.png");
        fs::write(&icon, crate::fake::png(32, 32, [30, 90, 220])).unwrap();
        let list = [entry("Battle.net", &icon.to_string_lossy()), entry("Hollow Knight: Silksong", ""), entry("Épée & Co_op", "")];
        for s in &list {
            shortcuts::add_shortcut(&file, s, "none", Steam::Closed).unwrap();
        }
        shortcuts::add_shortcut(&file, &crate::ui::selftile::tile(), crate::ui::selftile::MARKER, Steam::Closed).unwrap();
        let ids: Vec<u32> = list.iter().map(NewShortcut::appid).collect();
        // The player's own cover for the third, as a .jpg.
        fs::write(cfg.join(format!("grid/{}p.jpg", ids[2])), b"the player's own").unwrap();
        (d, home, cfg.join("grid"), ids)
    }

    #[test]
    fn the_entries_without_pictures_are_found() {
        let (d, home, _, ids) = home("list");
        let all = entries(&home);
        assert_eq!(all.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["Battle.net", "Hollow Knight: Silksong", "Épée & Co_op"]);
        assert_eq!(all.iter().map(|e| e.appid).collect::<Vec<_>>(), ids);
        assert_eq!(all[0].missing, vec!["capsule", "wide", "hero"]);
        assert_eq!(all[2].missing, vec!["wide", "hero"]);
        assert!(all[0].icon.ends_with("battle.png") && all[1].icon.is_empty());
        assert_eq!(to_fix(&home).len(), 3);
        assert!(entries(&d.join("nobody")).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn fix_writes_only_what_is_missing_and_undo_removes_exactly_that() {
        let (d, home, grid, ids) = home("fix");
        let folder = d.join("turbinehub");
        let list = home.join(".local/share/Steam/userdata/1001/config/shortcuts.vdf");
        let before = fs::read(&list).unwrap();
        // Three answers about Steam: only "closed" writes anything.
        assert!(fix(&home, &folder, Steam::Running).unwrap_err().starts_with("Close Steam first"));
        assert!(fix(&home, &folder, Steam::Unknown).unwrap_err().starts_with("Turbine Hub couldn't check whether Steam is running"));
        assert_eq!(fs::read_dir(&grid).unwrap().count(), 1);
        assert_eq!(fs::read(&list).unwrap(), before);
        let done = fix(&home, &folder, Steam::Closed).unwrap();
        assert_eq!(done.entries, vec!["Battle.net", "Hollow Knight: Silksong", "Épée & Co_op"]);
        // 3 for the first, 3 and an icon for the second, 2 and an icon for the third.
        assert_eq!(done.files, 3 + 4 + 3);
        for (id, part, w, h) in [(ids[0], "p", 600, 900), (ids[0], "", 920, 430), (ids[0], "_hero", 1920, 620), (ids[1], "_icon", 256, 256), (ids[2], "", 920, 430)] {
            let bytes = fs::read(grid.join(format!("{id}{part}.png"))).unwrap();
            assert_eq!(png::size(&bytes), Some((w, h)), "{id}{part}");
            assert!(bytes.len() < 400_000, "{id}{part} is {} bytes", bytes.len());
        }
        // The player's own cover stays, and no cover of Turbine Hub's goes beside it; an entry with an icon keeps it.
        assert_eq!(fs::read(grid.join(format!("{}p.jpg", ids[2]))).unwrap(), b"the player's own");
        assert!(!grid.join(format!("{}p.png", ids[2])).exists() && !grid.join(format!("{}_icon.png", ids[0])).exists());
        let now = entries(&home);
        assert!(now.iter().all(|e| e.missing.is_empty()) && to_fix(&home).is_empty());
        assert!(now[0].icon.ends_with("battle.png"));
        assert_eq!(now[1].icon, grid.join(format!("{}_icon.png", ids[1])).to_string_lossy());
        // The glow takes the icon's blue; without an icon the tone comes from the name.
        let cover = png::decode(&fs::read(grid.join(format!("{}p.png", ids[0]))).unwrap()).unwrap();
        let mid = &cover.rgba[(120 * 600 + 300) * 4..][..3];
        assert!(mid[2] > mid[0] + 30 && mid[2] > mid[1], "{mid:?}");
        // A second go has nothing to do. Steam's backup of the list was kept before the icons were set.
        assert_eq!(fix(&home, &folder, Steam::Closed).unwrap(), Fixed::default());
        assert_eq!(fs::read(shortcuts::backup_file(&list)).unwrap(), before);
        assert_eq!(undoable(&folder), 10);
        // The player replaces one of the pictures: Undo leaves that one, and takes the rest and the icons back.
        let replaced = grid.join(format!("{}.png", ids[0]));
        fs::write(&replaced, b"the player's new banner").unwrap();
        assert!(undo(&home, &folder, Steam::Running).is_err());
        assert_eq!(undo(&home, &folder, Steam::Closed).unwrap(), 9);
        let mut left: Vec<String> = fs::read_dir(&grid).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        let mut want = vec![format!("{}.png", ids[0]), format!("{}p.jpg", ids[2])];
        want.sort();
        assert_eq!(left, want);
        assert_eq!(fs::read(&replaced).unwrap(), b"the player's new banner");
        assert!(entries(&home).iter().all(|e| e.icon.is_empty() || e.icon.ends_with("battle.png")));
        assert_eq!((undoable(&folder), undo(&home, &folder, Steam::Closed).unwrap()), (0, 0));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn names_are_set_in_the_built_in_face() {
        assert_eq!(plain_name("Épée & Co_op"), "EPEE & CO OP");
        assert_eq!(plain_name("Hollow Knight: Silksong\u{2122}"), "HOLLOW KNIGHT: SILKSONG");
        assert_eq!(plain_name("Half\u{2013}Life  2"), "HALF-LIFE 2");
        assert_eq!(plain_name("\u{30bc}\u{30eb}\u{30c0}"), "");
        // Every letter the face claims has dots, and none is wider than five.
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,:'-!?&+/()".chars() {
            let g = glyph(c).unwrap();
            assert!(g.iter().any(|r| *r != 0) && g.iter().all(|r| *r < 32), "{c}");
        }
        assert_eq!(wrap("HOLLOW KNIGHT: SILKSONG", 9, 4, false).unwrap(), vec!["HOLLOW", "KNIGHT:", "SILKSONG"]);
        assert_eq!(wrap("HOLLOW KNIGHT: SILKSONG", 7, 4, false), None);
        assert_eq!(wrap("SUPERCALIFRAGILISTIC", 8, 2, true).unwrap(), vec!["SUPERCAL", "IFRAGILI"]);
        // A short name is set big, a long one smaller, and both stay inside the picture's width.
        let (big, lines) = fit("DOOM", 500, 4, 10, 3);
        assert_eq!((big, lines), (10, vec!["DOOM".to_string()]));
        let (small, lines) = fit("THE ELDER SCROLLS V: SKYRIM SPECIAL EDITION", 500, 4, 10, 3);
        assert!(small < 10 && lines.len() <= 4 && lines.iter().all(|l| l.chars().count() as i64 * 6 * small - small <= 500), "{small} {lines:?}");
        // A name the face has no letters for still gives a picture: the ground and the glow.
        assert_eq!(png::size(&draw("wide", "\u{30bc}\u{30eb}\u{30c0}", tone_for("x"), None)), Some((920, 430)));
        assert_eq!(tone_for("Battle.net"), tone_for("Battle.net"));
    }

    #[test]
    fn the_glow_takes_the_icons_color() {
        let solid = |c: [u8; 4]| Image { width: 2, height: 2, rgba: c.repeat(4) };
        let blue = icon_color(&solid([20, 60, 140, 255])).unwrap();
        assert!(blue[2] == 210 && blue[2] > blue[1] && blue[1] > blue[0], "{blue:?}");
        assert_eq!(icon_color(&solid([200, 10, 10, 0])), None);
        assert_eq!(icon_color(&solid([0, 0, 0, 255])), Some([120, 120, 120]));
        // Grey and a little red: the red counts for more.
        let mut mixed = solid([128, 128, 128, 255]);
        mixed.rgba[..4].copy_from_slice(&[230, 20, 20, 255]);
        let c = icon_color(&mixed).unwrap();
        assert!(c[0] > c[1] + 40, "{c:?}");
        assert!(read_icon("").is_none() && read_icon("/nowhere/icon.png").is_none());
    }
}
