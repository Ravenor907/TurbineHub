//! PNG pictures, written and read with the engine's own `deflate` and `inflate`: enough to write the library
//! pictures `tile_art` draws (8 bits a channel, red, green and blue) and to read an icon for its color (the plain
//! kinds of PNG: 8 bits a channel, or a palette, not interlaced). Anything else reads as "can't tell", never as an
//! error the player sees.

use crate::deflate::zlib;
use crate::inflate::inflate;
use crate::zip::crc32;

const MAGIC: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// A picture in memory: four bytes a pixel (red, green, blue, how solid), row after row from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend(data);
    out.extend(&body);
    out.extend(crc32(&body).to_be_bytes());
}

/// Writes a picture of `width` by `height` from three bytes a pixel (red, green, blue). Each row is stored as the
/// difference to the pixel before or to the row above, whichever leaves more of it flat, so smooth pictures pack
/// small.
pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let stride = width as usize * 3;
    assert_eq!(rgb.len(), stride * height as usize, "three bytes a pixel");
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    let cost = |row: &[u8]| -> u64 { row.iter().map(|b| (*b as i8).unsigned_abs() as u64).sum() };
    let (mut sub, mut up) = (vec![0u8; stride], vec![0u8; stride]);
    for y in 0..height as usize {
        let row = &rgb[y * stride..(y + 1) * stride];
        for x in 0..stride {
            sub[x] = row[x].wrapping_sub(if x >= 3 { row[x - 3] } else { 0 });
            up[x] = row[x].wrapping_sub(if y > 0 { rgb[(y - 1) * stride + x] } else { 0 });
        }
        if cost(&up) <= cost(&sub) {
            raw.push(2);
            raw.extend_from_slice(&up);
        } else {
            raw.push(1);
            raw.extend_from_slice(&sub);
        }
    }
    let mut out = MAGIC.to_vec();
    let mut head = Vec::new();
    head.extend(width.to_be_bytes());
    head.extend(height.to_be_bytes());
    head.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &head);
    chunk(&mut out, b"IDAT", &zlib(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The size a PNG says it has, from its first bytes.
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != MAGIC || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let n = |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    Some((n(16), n(20)))
}

/// The largest side a picture may have to be read here (icons are far smaller).
const MAX_SIDE: u32 = 2048;

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (a as i32, b as i32, c as i32);
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Reads a PNG. None for anything that isn't one of the plain kinds (see the top of this file), is bigger than an
/// icon ever is, or is damaged.
pub fn decode(bytes: &[u8]) -> Option<Image> {
    let (width, height) = size(bytes)?;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let (mut depth, mut kind, mut interlace) = (0u8, 0u8, 0u8);
    let (mut data, mut palette, mut clear) = (Vec::new(), Vec::new(), Vec::new());
    let mut at = 8;
    while at + 12 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
        let body = bytes.get(at + 8..(at + 8).checked_add(len)?)?;
        match &bytes[at + 4..at + 8] {
            b"IHDR" if len >= 13 => (depth, kind, interlace) = (body[8], body[9], body[12]),
            b"PLTE" => palette = body.to_vec(),
            b"tRNS" => clear = body.to_vec(),
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    if interlace != 0 || data.len() < 6 {
        return None;
    }
    // Samples a pixel, and bits a sample.
    let samples = match kind {
        0 | 3 => 1usize,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    if !(depth == 8 || (kind == 3 && matches!(depth, 1 | 2 | 4))) {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    let stride = (w * samples * depth as usize).div_ceil(8);
    let step = if depth == 8 { samples } else { 1 };
    let raw = inflate(&data[2..], (stride + 1) * h).ok()?;
    if raw.len() != (stride + 1) * h {
        return None;
    }
    let mut pixels = vec![0u8; stride * h];
    for y in 0..h {
        let line = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        for x in 0..stride {
            let left = if x >= step { pixels[y * stride + x - step] } else { 0 };
            let above = if y > 0 { pixels[(y - 1) * stride + x] } else { 0 };
            let corner = if y > 0 && x >= step { pixels[(y - 1) * stride + x - step] } else { 0 };
            let v = line[1 + x];
            pixels[y * stride + x] = match line[0] {
                0 => v,
                1 => v.wrapping_add(left),
                2 => v.wrapping_add(above),
                3 => v.wrapping_add(((left as u16 + above as u16) / 2) as u8),
                4 => v.wrapping_add(paeth(left, above, corner)),
                _ => return None,
            };
        }
    }
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = &pixels[y * stride..(y + 1) * stride];
        for x in 0..w {
            let px = match kind {
                0 => [row[x], row[x], row[x], 255],
                2 => [row[x * 3], row[x * 3 + 1], row[x * 3 + 2], 255],
                3 => {
                    let i = if depth == 8 { row[x] as usize } else { ((row[x * depth as usize / 8] >> (8 - depth as usize - (x * depth as usize) % 8)) & ((1 << depth) - 1)) as usize };
                    let c = palette.get(i * 3..i * 3 + 3)?;
                    [c[0], c[1], c[2], clear.get(i).copied().unwrap_or(255)]
                }
                4 => [row[x * 2], row[x * 2], row[x * 2], row[x * 2 + 1]],
                _ => [row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]],
            };
            rgba.extend(px);
        }
    }
    Some(Image { width, height, rgba })
}

/// The biggest PNG picture inside a Windows .ico file, when it has one (newer icons keep their large sizes as
/// PNG; the old bitmap sizes aren't read).
pub fn from_ico(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < 6 || bytes[..4] != [0, 0, 1, 0] {
        return None;
    }
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    let mut best: Option<(u64, &[u8])> = None;
    for i in 0..count.min(64) {
        let e = bytes.get(6 + i * 16..6 + i * 16 + 16)?;
        let len = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        let at = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
        let Some(data) = at.checked_add(len).and_then(|end| bytes.get(at..end)) else { continue };
        if let Some((w, h)) = size(data)
            && best.is_none_or(|(area, _)| w as u64 * h as u64 > area)
        {
            best = Some((w as u64 * h as u64, data));
        }
    }
    best.map(|(_, d)| d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_picture_reads_back_the_same_and_packs_small() {
        // A soft slope with a hard edge in it: both ways of storing a row are used.
        let (w, h) = (97u32, 61u32);
        let mut rgb = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgb.extend(if x > 40 && y % 7 == 0 { [250, 3, 77] } else { [(x * 2) as u8, (y * 3) as u8, ((x + y) / 2) as u8] });
            }
        }
        let png = encode_rgb(w, h, &rgb);
        assert_eq!(size(&png), Some((w, h)));
        let back = decode(&png).unwrap();
        assert_eq!((back.width, back.height), (w, h));
        assert!(back.rgba.chunks(4).zip(rgb.chunks(3)).all(|(a, b)| a[..3] == *b && a[3] == 255));
        // A flat picture of a million bytes is a few kilobytes.
        let flat = encode_rgb(600, 600, &[11u8; 600 * 600 * 3]);
        assert!(flat.len() < 8_000, "{}", flat.len());
        assert_eq!(decode(&flat).unwrap().rgba[..4], [11, 11, 11, 255]);
        // The made-up machine's pictures (stored, not packed) read too.
        let stored = crate::fake::png(5, 4, [200, 30, 40]);
        assert_eq!(decode(&stored).unwrap().rgba[16..20], [200, 30, 40, 255]);
    }

    #[test]
    fn what_is_not_a_plain_png_reads_as_nothing() {
        assert_eq!(decode(b"MZ\x90\0 not a picture at all"), None);
        assert_eq!(decode(&[]), None);
        let good = encode_rgb(4, 4, &[9u8; 48]);
        assert_eq!(decode(&good[..good.len() - 20]), None);
        let mut huge = good.clone();
        huge[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        assert_eq!(decode(&huge), None);
        let mut laced = good.clone();
        laced[28] = 1;
        assert_eq!(decode(&laced), None);
    }

    #[test]
    fn an_icon_file_gives_its_biggest_png() {
        let small = encode_rgb(2, 2, &[1u8; 12]);
        let big = encode_rgb(8, 8, &[2u8; 192]);
        let mut ico = vec![0, 0, 1, 0, 3, 0];
        let mut at = 6 + 3 * 16;
        let bitmap = vec![40u8; 30];
        for data in [&small, &bitmap, &big] {
            ico.extend([0u8; 8]);
            ico.extend((data.len() as u32).to_le_bytes());
            ico.extend((at as u32).to_le_bytes());
            at += data.len();
        }
        for data in [&small, &bitmap, &big] {
            ico.extend(data.iter());
        }
        assert_eq!(from_ico(&ico), Some(&big[..]));
        assert_eq!(from_ico(&small), None);
        assert_eq!(from_ico(&ico[..40]), None);
    }
}
