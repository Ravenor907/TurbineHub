//! SHA-512 (FIPS 180-4), written here like `sha256` so the engine needs no outside code to check a download whose
//! publisher gives a .sha512sum beside it (GE-Proton does).

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const K: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc, 0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
    0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2, 0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
    0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65, 0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4, 0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
    0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df, 0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30, 0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8, 0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec, 0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
    0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178, 0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c, 0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

const H0: [u64; 8] = [0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1, 0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179];

/// A running SHA-512: feed it bytes with `update`, then `finish`.
#[derive(Clone)]
pub struct Sha512 {
    h: [u64; 8],
    buf: [u8; 128],
    used: usize,
    len: u128,
}

impl Default for Sha512 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha512 {
    pub fn new() -> Self {
        Sha512 { h: H0, buf: [0; 128], used: 0, len: 0 }
    }

    fn block(h: &mut [u64; 8], b: &[u8]) {
        let mut w = [0u64; 80];
        for (i, word) in w.iter_mut().take(16).enumerate() {
            let mut x = [0u8; 8];
            x.copy_from_slice(&b[i * 8..i * 8 + 8]);
            *word = u64::from_be_bytes(x);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut bb, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
        for i in 0..80 {
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & bb) ^ (a & c) ^ (bb & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = bb;
            bb = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, bb, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u128);
        if self.used > 0 {
            let take = (128 - self.used).min(data.len());
            self.buf[self.used..self.used + take].copy_from_slice(&data[..take]);
            self.used += take;
            data = &data[take..];
            if self.used < 128 {
                return;
            }
            let buf = self.buf;
            Self::block(&mut self.h, &buf);
            self.used = 0;
        }
        let mut chunks = data.chunks_exact(128);
        for c in &mut chunks {
            Self::block(&mut self.h, c);
        }
        let rest = chunks.remainder();
        self.buf[..rest.len()].copy_from_slice(rest);
        self.used = rest.len();
    }

    pub fn finish(mut self) -> [u8; 64] {
        let bits = self.len.wrapping_mul(8);
        self.update(&[0x80]);
        while self.used != 112 {
            self.update(&[0]);
        }
        self.update(&bits.to_be_bytes());
        let mut out = [0u8; 64];
        for (i, v) in self.h.iter().enumerate() {
            out[i * 8..i * 8 + 8].copy_from_slice(&v.to_be_bytes());
        }
        out
    }
}

/// SHA-512 of some bytes as lowercase hex.
pub fn digest_hex(data: &[u8]) -> String {
    let mut h = Sha512::new();
    h.update(data);
    crate::sha256::hex(&h.finish())
}

/// SHA-512 of a file as lowercase hex, read in pieces so files of any size need little memory.
pub fn file_hex(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha512::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(crate::sha256::hex(&h.finish()))
}

/// Is this a SHA-512 written as 128 hex digits (either case)?
pub fn is_hex_digest(s: &str) -> bool {
    s.len() == 128 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Do two hex digests name the same hash (case doesn't matter)?
pub fn same_digest(a: &str, b: &str) -> bool {
    is_hex_digest(a) && is_hex_digest(b) && a.eq_ignore_ascii_case(b)
}

/// The hash a .sha512sum file gives for `name` ("<hash>  <name>" lines, as sha512sum writes them; a "*" before the
/// name marks binary mode). A file with one line that names no file, or another file, gives nothing: a hash for
/// some other file must never pass for this one.
pub fn from_sum_file(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let mut w = line.split_whitespace();
        let (Some(hash), Some(file)) = (w.next(), w.next()) else { continue };
        let file = file.trim_start_matches('*');
        if is_hex_digest(hash) && file.rsplit('/').next() == Some(name) {
            return Some(hash.to_ascii_lowercase());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // The vectors FIPS 180-4's examples and NIST's test data give for SHA-512.
    const EMPTY: &str = "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e";
    const ABC: &str = "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f";
    const TWO_BLOCKS: &str = "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909";
    const MILLION_A: &str = "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973ebde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b";

    #[test]
    fn matches_the_official_vectors() {
        assert_eq!(digest_hex(b""), EMPTY);
        assert_eq!(digest_hex(b"abc"), ABC);
        assert_eq!(digest_hex(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"), TWO_BLOCKS);
        // A million times "a", fed in uneven pieces so every path through `update` runs.
        let mut h = Sha512::new();
        let piece = [b'a'; 1000];
        let mut left = 1_000_000;
        for n in [1usize, 127, 128, 129, 255, 1000].iter().cycle() {
            if left == 0 {
                break;
            }
            let take = (*n).min(left);
            h.update(&piece[..take]);
            left -= take;
        }
        assert_eq!(crate::sha256::hex(&h.finish()), MILLION_A);
    }

    #[test]
    fn a_file_hashes_like_its_bytes_and_sum_files_are_read_by_name() {
        let f = std::env::temp_dir().join(format!("turbinehub-sha512-{}", std::process::id()));
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(file_hex(&f).unwrap(), ABC);
        let _ = std::fs::remove_file(&f);
        assert!(same_digest(ABC, &ABC.to_uppercase()) && !same_digest(ABC, EMPTY) && !same_digest("abc", "abc"));
        let sums = format!("{EMPTY}  other.tar.gz\n{}  GE-Proton10-99.tar.gz\n", ABC.to_uppercase());
        assert_eq!(from_sum_file(&sums, "GE-Proton10-99.tar.gz").as_deref(), Some(ABC));
        assert_eq!(from_sum_file(&format!("{ABC} *./GE-Proton10-99.tar.gz"), "GE-Proton10-99.tar.gz").as_deref(), Some(ABC));
        assert_eq!(from_sum_file(&sums, "GE-Proton10-98.tar.gz"), None);
        assert_eq!(from_sum_file(ABC, "GE-Proton10-99.tar.gz"), None);
        assert_eq!(from_sum_file("<html>not found</html>", "x"), None);
    }
}
