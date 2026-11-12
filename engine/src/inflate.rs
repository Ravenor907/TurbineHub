//! DEFLATE decompression (RFC 1951), enough for zip archives. Written here so the engine has no outside code in the
//! path every downloaded file goes through. Output is capped so a hostile archive can't fill the disk or memory.
//! `inflate_stream` unpacks straight from a reader to a writer, so files of any size need only a few hundred KiB.

use std::io::{self, Read, Write};

#[derive(Debug, PartialEq)]
pub enum InflateError {
    Truncated,
    BadBlock,
    BadCode,
    BadDistance,
    TooLarge,
}

impl std::fmt::Display for InflateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            InflateError::Truncated => "the compressed data ends early",
            InflateError::BadBlock => "the compressed data is damaged (bad block)",
            InflateError::BadCode => "the compressed data is damaged (bad code)",
            InflateError::BadDistance => "the compressed data is damaged (bad distance)",
            InflateError::TooLarge => "the file unpacks to more than its stated size",
        })
    }
}

impl std::error::Error for InflateError {}

/// What can stop a streaming inflate: damaged data, or the source or the sink failing.
#[derive(Debug)]
pub enum StreamError {
    Inflate(InflateError),
    Read(io::Error),
    Write(io::Error),
}

impl From<InflateError> for StreamError {
    fn from(e: InflateError) -> Self {
        StreamError::Inflate(e)
    }
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Inflate(e) => e.fmt(f),
            StreamError::Read(e) => write!(f, "couldn't read the compressed data ({e})"),
            StreamError::Write(e) => write!(f, "couldn't write the unpacked data ({e})"),
        }
    }
}

impl std::error::Error for StreamError {}

/// How much compressed input is read from the source at a time.
const IN_CHUNK: usize = 32 << 10;

/// Bit reader over any byte source. Bytes are pulled one at a time from a small buffer, so after `align` the next
/// byte is exactly the next unread byte of the stream (stored blocks depend on that).
struct Bits<R> {
    src: R,
    buf: Vec<u8>,
    pos: usize,
    end: usize,
    bit: u32,
    nbits: u32,
}

impl<R: Read> Bits<R> {
    fn new(src: R) -> Self {
        Bits { src, buf: vec![0; IN_CHUNK], pos: 0, end: 0, bit: 0, nbits: 0 }
    }

    fn byte(&mut self) -> Result<u8, StreamError> {
        if self.pos == self.end {
            loop {
                match self.src.read(&mut self.buf) {
                    Ok(0) => return Err(InflateError::Truncated.into()),
                    Ok(n) => {
                        self.pos = 0;
                        self.end = n;
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(StreamError::Read(e)),
                }
            }
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn need(&mut self, n: u32) -> Result<(), StreamError> {
        while self.nbits < n {
            let b = self.byte()?;
            self.bit |= (b as u32) << self.nbits;
            self.nbits += 8;
        }
        Ok(())
    }

    fn take(&mut self, n: u32) -> Result<u32, StreamError> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Ok(v)
    }

    fn align(&mut self) {
        self.bit = 0;
        self.nbits = 0;
    }
}

/// The 32 KiB history that back-references reach into.
const WINDOW: usize = 32 << 10;
/// Unpacked bytes are handed to the sink once this many are waiting.
const OUT_CHUNK: usize = 64 << 10;

/// Output side: keeps the last 32 KiB for back-references and hands everything to the sink in chunks, so memory
/// stays the same whatever the size of the file.
struct Out<'a> {
    sink: &'a mut dyn Write,
    /// History (the last WINDOW bytes already written) followed by bytes not written yet.
    win: Vec<u8>,
    /// Index in `win` of the first byte not written yet.
    pending: usize,
    total: u64,
    limit: u64,
}

impl<'a> Out<'a> {
    fn new(sink: &'a mut dyn Write, limit: u64) -> Self {
        Out { sink, win: Vec::with_capacity(WINDOW + OUT_CHUNK + 258), pending: 0, total: 0, limit }
    }

    fn room(&mut self, len: usize) -> Result<(), StreamError> {
        if self.total.saturating_add(len as u64) > self.limit {
            return Err(InflateError::TooLarge.into());
        }
        if self.win.len() + len > WINDOW + OUT_CHUNK {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), StreamError> {
        self.sink.write_all(&self.win[self.pending..]).map_err(StreamError::Write)?;
        let keep_from = self.win.len().saturating_sub(WINDOW);
        self.win.drain(..keep_from);
        self.pending = self.win.len();
        Ok(())
    }

    fn byte(&mut self, b: u8) -> Result<(), StreamError> {
        self.room(1)?;
        self.win.push(b);
        self.total += 1;
        Ok(())
    }

    fn copy(&mut self, back: usize, len: usize) -> Result<(), StreamError> {
        // The window always holds every byte so far, or at least the last WINDOW of them; `back` is at most WINDOW.
        if back > self.win.len() {
            return Err(InflateError::BadDistance.into());
        }
        self.room(len)?;
        let start = self.win.len() - back;
        if back >= len {
            self.win.extend_from_within(start..start + len);
        } else {
            for k in 0..len {
                let b = self.win[start + k];
                self.win.push(b);
            }
        }
        self.total += len as u64;
        Ok(())
    }
}

/// Canonical Huffman decoding table: counts per length and symbols in order.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Self, InflateError> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        // Over-subscribed sets are invalid; incomplete ones are allowed (a single code is legal).
        let mut left: i32 = 1;
        for &c in counts.iter().skip(1) {
            left <<= 1;
            left -= c as i32;
            if left < 0 {
                return Err(InflateError::BadCode);
            }
        }
        let mut offs = [0u16; 16];
        for i in 1..15 {
            offs[i + 1] = offs[i] + counts[i];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { counts, symbols })
    }

    fn decode<R: Read>(&self, bits: &mut Bits<R>) -> Result<u16, StreamError> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.take(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(InflateError::BadCode.into())
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289,
    16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Decompresses raw DEFLATE data. `limit` is the most bytes the output may reach.
pub fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>, InflateError> {
    let mut out: Vec<u8> = Vec::with_capacity(limit.min(1 << 16));
    match inflate_stream(data, &mut out, limit as u64) {
        Ok(_) => Ok(out),
        Err(StreamError::Inflate(e)) => Err(e),
        // A slice never fails to read and a Vec never fails to write, short of running out of memory.
        Err(StreamError::Read(_)) => Err(InflateError::Truncated),
        Err(StreamError::Write(_)) => Err(InflateError::TooLarge),
    }
}

/// Decompresses raw DEFLATE data from `src` into `sink`, a chunk at a time, without holding more than a 32 KiB
/// window and small buffers. `limit` is the most bytes the output may reach. Returns how many bytes were written.
/// Stops after the last block; any input after it is left unread.
pub fn inflate_stream(src: impl Read, sink: &mut dyn Write, limit: u64) -> Result<u64, StreamError> {
    let mut bits = Bits::new(src);
    let mut out = Out::new(sink, limit);
    loop {
        let last = bits.take(1)? == 1;
        match bits.take(2)? {
            0 => {
                bits.align();
                let hdr = [bits.byte()?, bits.byte()?, bits.byte()?, bits.byte()?];
                let len = u16::from_le_bytes([hdr[0], hdr[1]]) as usize;
                let nlen = u16::from_le_bytes([hdr[2], hdr[3]]) as usize;
                if len != (!nlen & 0xFFFF) {
                    return Err(InflateError::BadBlock.into());
                }
                for _ in 0..len {
                    let b = bits.byte()?;
                    out.byte(b)?;
                }
            }
            1 => {
                let mut l = [0u8; 288];
                l[..144].fill(8);
                l[144..256].fill(9);
                l[256..280].fill(7);
                l[280..].fill(8);
                let lit = Huffman::new(&l)?;
                let dist = Huffman::new(&[5u8; 30])?;
                block(&mut bits, &mut out, &lit, &dist)?;
            }
            2 => {
                let hlit = bits.take(5)? as usize + 257;
                let hdist = bits.take(5)? as usize + 1;
                let hclen = bits.take(4)? as usize + 4;
                const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut cl = [0u8; 19];
                for &i in ORDER.iter().take(hclen) {
                    cl[i] = bits.take(3)? as u8;
                }
                let clh = Huffman::new(&cl)?;
                let mut lengths = vec![0u8; hlit + hdist];
                let mut i = 0;
                while i < hlit + hdist {
                    let sym = clh.decode(&mut bits)?;
                    let (val, rep) = match sym {
                        0..=15 => (sym as u8, 1),
                        16 => {
                            let prev = *lengths.get(i.wrapping_sub(1)).ok_or(InflateError::BadBlock)?;
                            (prev, 3 + bits.take(2)? as usize)
                        }
                        17 => (0, 3 + bits.take(3)? as usize),
                        18 => (0, 11 + bits.take(7)? as usize),
                        _ => return Err(InflateError::BadCode.into()),
                    };
                    if i + rep > hlit + hdist {
                        return Err(InflateError::BadBlock.into());
                    }
                    lengths[i..i + rep].fill(val);
                    i += rep;
                }
                if lengths[256] == 0 {
                    return Err(InflateError::BadBlock.into());
                }
                let lit = Huffman::new(&lengths[..hlit])?;
                let dist = Huffman::new(&lengths[hlit..])?;
                block(&mut bits, &mut out, &lit, &dist)?;
            }
            _ => return Err(InflateError::BadBlock.into()),
        }
        if last {
            out.flush()?;
            return Ok(out.total);
        }
    }
}

fn block<R: Read>(bits: &mut Bits<R>, out: &mut Out, lit: &Huffman, dist: &Huffman) -> Result<(), StreamError> {
    loop {
        let sym = lit.decode(bits)? as usize;
        if sym < 256 {
            out.byte(sym as u8)?;
        } else if sym == 256 {
            return Ok(());
        } else {
            let i = sym - 257;
            if i >= 29 {
                return Err(InflateError::BadCode.into());
            }
            let len = LEN_BASE[i] as usize + bits.take(LEN_EXTRA[i] as u32)? as usize;
            let d = dist.decode(bits)? as usize;
            if d >= 30 {
                return Err(InflateError::BadDistance.into());
            }
            let back = DIST_BASE[d] as usize + bits.take(DIST_EXTRA[d] as u32)? as usize;
            out.copy(back, len)?;
        }
    }
}
