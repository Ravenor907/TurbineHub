//! DEFLATE compression (RFC 1951), the other half of `inflate`: enough to write small PNG pictures and gzip files
//! without outside code. One block with the fixed codes and a plain search for repeats: not the smallest output
//! there is, but flat pictures (long runs of the same bytes) shrink to a few percent, and every byte it writes is
//! read back by `inflate` in the tests.

/// Writes bits the way DEFLATE wants them: the first bit goes into the lowest bit of the byte.
struct BitWriter {
    out: Vec<u8>,
    bit: u64,
    nbits: u32,
}

impl BitWriter {
    fn put(&mut self, value: u32, n: u32) {
        self.bit |= (value as u64) << self.nbits;
        self.nbits += n;
        while self.nbits >= 8 {
            self.out.push(self.bit as u8);
            self.bit >>= 8;
            self.nbits -= 8;
        }
    }

    /// A Huffman code: its bits go out highest first.
    fn code(&mut self, code: u32, n: u32) {
        self.put(code.reverse_bits() >> (32 - n), n);
    }

    /// One literal byte or length symbol (0 to 287) in the fixed code.
    fn symbol(&mut self, sym: u32) {
        match sym {
            0..=143 => self.code(0x30 + sym, 8),
            144..=255 => self.code(0x190 + sym - 144, 9),
            256..=279 => self.code(sym - 256, 7),
            _ => self.code(0xC0 + sym - 280, 8),
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.nbits > 0 {
            self.out.push(self.bit as u8);
        }
        self.out
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// How far back a repeat may reach, and how long it may be.
const WINDOW: usize = 32 << 10;
const MAX_LEN: usize = 258;
/// How many earlier places with the same first three bytes are tried for each repeat.
const TRIES: usize = 12;
const HASH_BITS: u32 = 15;

fn hash(d: &[u8], i: usize) -> usize {
    let v = (d[i] as u32) | ((d[i + 1] as u32) << 8) | ((d[i + 2] as u32) << 16);
    (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
}

/// Notes that the three bytes at `at` (with this hash) can be pointed at by later repeats.
fn remember(head: &mut [u32], prev: &mut [u32], h: usize, at: usize) {
    prev[at] = head[h];
    head[h] = at as u32;
}

/// Compresses `data` into raw DEFLATE.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter { out: Vec::with_capacity(data.len() / 4 + 16), bit: 0, nbits: 0 };
    // The last block, with the fixed codes.
    w.put(1, 1);
    w.put(1, 2);
    const NONE: u32 = u32::MAX;
    let mut head = vec![NONE; 1 << HASH_BITS];
    let mut prev = vec![NONE; data.len()];
    let mut i = 0;
    while i < data.len() {
        let (mut best_len, mut best_dist) = (0usize, 0usize);
        if i + 3 <= data.len() {
            let h = hash(data, i);
            let mut at = head[h];
            let most = MAX_LEN.min(data.len() - i);
            for _ in 0..TRIES {
                if at == NONE || i - at as usize > WINDOW {
                    break;
                }
                let j = at as usize;
                let len = data[j..].iter().zip(&data[i..i + most]).take_while(|(a, b)| a == b).count();
                if len > best_len {
                    (best_len, best_dist) = (len, i - j);
                    if len == most {
                        break;
                    }
                }
                at = prev[j];
            }
            remember(&mut head, &mut prev, h, i);
        }
        if best_len >= 3 {
            let l = LEN_BASE.iter().rposition(|b| *b as usize <= best_len).unwrap_or(0);
            w.symbol(257 + l as u32);
            w.put((best_len - LEN_BASE[l] as usize) as u32, LEN_EXTRA[l] as u32);
            let d = DIST_BASE.iter().rposition(|b| *b as usize <= best_dist).unwrap_or(0);
            w.code(d as u32, 5);
            w.put((best_dist - DIST_BASE[d] as usize) as u32, DIST_EXTRA[d] as u32);
            // The bytes the repeat covers are places later repeats can point at too.
            for k in (i + 1..i + best_len).take_while(|k| k + 3 <= data.len()) {
                remember(&mut head, &mut prev, hash(data, k), k);
            }
            i += best_len;
        } else {
            w.symbol(data[i] as u32);
            i += 1;
        }
    }
    w.symbol(256);
    w.finish()
}

/// Adler-32, the check zlib puts after its data.
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    // 5552 bytes is the most that can be added before the sums have to be brought back under 65521.
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

/// `data` as a zlib stream (what a PNG keeps its pixels in).
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    out.extend(deflate(data));
    out.extend(adler32(data).to_be_bytes());
    out
}

/// `data` as a gzip file (no name, no time in it, so the same bytes give the same file).
pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
    out.extend(deflate(data));
    out.extend(crate::zip::crc32(data).to_le_bytes());
    out.extend((data.len() as u32).to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inflate::inflate;

    fn round(data: &[u8]) -> usize {
        let packed = deflate(data);
        assert_eq!(inflate(&packed, data.len()).unwrap(), data, "{} bytes", data.len());
        packed.len()
    }

    #[test]
    fn what_it_writes_inflates_back_to_the_same_bytes() {
        assert_eq!(round(b""), 2);
        round(b"a");
        round(b"ab");
        round(b"abc");
        round(b"abcabcabcabcabcabcabcabc");
        // Every byte value, so every literal code is used.
        let all: Vec<u8> = (0..=255u8).collect();
        round(&all);
        // A long flat run shrinks to almost nothing; repeats of every length and far back are found.
        assert!(round(&vec![7u8; 200_000]) < 2_000);
        let mut mixed = Vec::new();
        let mut seed = 12345u32;
        for n in 0..4000usize {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            mixed.push((seed >> 16) as u8);
            if n % 7 == 0 {
                let from = mixed.len().saturating_sub(1 + (seed as usize >> 8) % 40_000);
                let len = 3 + (seed as usize >> 4) % 300;
                for k in 0..len {
                    mixed.push(mixed[from + k % (mixed.len() - from)]);
                }
            }
        }
        assert!(round(&mixed) < mixed.len());
    }

    #[test]
    fn the_checks_match_known_values() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
        let z = zlib(b"hello hello hello");
        assert_eq!((&z[..2], &z[z.len() - 4..]), (&[0x78u8, 0x01][..], &adler32(b"hello hello hello").to_be_bytes()[..]));
        assert_eq!(inflate(&z[2..z.len() - 4], 100).unwrap(), b"hello hello hello");
        let g = gzip(b"abc");
        assert_eq!((&g[..3], &g[g.len() - 8..g.len() - 4], &g[g.len() - 4..]), (&[0x1f, 0x8b, 8][..], &crate::zip::crc32(b"abc").to_le_bytes()[..], &3u32.to_le_bytes()[..]));
    }
}
