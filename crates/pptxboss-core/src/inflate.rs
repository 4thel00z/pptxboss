//! A DEFLATE decoder (RFC 1951) for buffers held whole in memory: the
//! compressed bytes of a ZIP entry, decoded in one call into a `Vec`.
//!
//! Huffman codes are read through a 10-bit lookup table, with a canonical
//! code walk for the rare longer codes, so the per-block table build stays
//! cheap for the small XML parts a package is made of. Matches copy out of
//! the output vector itself; there is no separate window.

use std::sync::OnceLock;

use crate::error::{Error, Result};

const MAX_BITS: usize = 15;
/// Table widths, as many bits as the frequent codes of each alphabet need.
const LITERAL_BITS: u32 = 9;
const DISTANCE_BITS: u32 = 8;
const CODE_LENGTH_BITS: u32 = 7;
const FAST_SIZE: usize = 1 << LITERAL_BITS;
const MAX_SYMBOLS: usize = 288;
const END_OF_BLOCK: u16 = 256;

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// DEFLATE expands at most this much (a 258-byte match per 2-bit code),
/// which bounds the reservation a corrupt size hint can ask for.
const MAX_RATIO: usize = 1032;
/// The most one part may inflate to.
pub const MAX_PART_BYTES: usize = 128 << 20;
/// How far past its compressed size a parsed part may inflate.
pub const MAX_INFLATE_RATIO: usize = 100;
/// Output up to this size is exempt from the ratio, and counted in the deck limit.
pub const RATIO_EXEMPT_BYTES: usize = 1 << 20;
/// The most a deck's parsed parts may inflate to in all.
pub const MAX_DECK_BYTES: usize = 512 << 20;

/// The output limit for a part parsed as XML: `MAX_INFLATE_RATIO` times its
/// compressed size, within `RATIO_EXEMPT_BYTES` and `MAX_PART_BYTES`.
pub fn ratio_limit(compressed: usize) -> usize {
    compressed
        .saturating_mul(MAX_INFLATE_RATIO)
        .clamp(RATIO_EXEMPT_BYTES, MAX_PART_BYTES)
}

/// The output limit for all of a deck's parsed parts together: the ratio times
/// the file size, within `RATIO_EXEMPT_BYTES` and `MAX_DECK_BYTES`.
pub fn deck_limit(file: usize) -> usize {
    file.saturating_mul(MAX_INFLATE_RATIO)
        .clamp(RATIO_EXEMPT_BYTES, MAX_DECK_BYTES)
}
/// The order in which code length code lengths are transmitted (3.2.7).
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn truncated() -> Error {
    Error::Inflate("truncated deflate stream".into())
}

fn invalid(msg: &str) -> Error {
    Error::Inflate(msg.into())
}

fn past_limit(limit: usize) -> Error {
    Error::Inflate(format!("output passes the inflate limit of {limit} bytes"))
}

/// A little-endian bit reader over the whole input. Failures are recorded
/// in `failure` and checked once per symbol, keeping `Result` out of the
/// decode loop.
struct Bits<'a> {
    data: &'a [u8],
    /// Next byte to load into the accumulator.
    pos: usize,
    acc: u64,
    /// Valid low bits of `acc`.
    count: u32,
    failure: Option<&'static str>,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            acc: 0,
            count: 0,
            failure: None,
        }
    }

    /// Tops the accumulator up to at least 56 bits while input remains.
    #[inline(always)]
    fn refill(&mut self) {
        if self.pos + 8 <= self.data.len() {
            let mut word = [0u8; 8];
            word.copy_from_slice(&self.data[self.pos..self.pos + 8]);
            self.acc |= u64::from_le_bytes(word) << self.count;
            let bytes = (63 - self.count) >> 3;
            self.pos += bytes as usize;
            self.count += bytes * 8;
            return;
        }
        while self.count < 56 && self.pos < self.data.len() {
            self.acc |= u64::from(self.data[self.pos]) << self.count;
            self.pos += 1;
            self.count += 8;
        }
    }

    /// The next `n` bits (n <= 32) without consuming them; zeros past the end of input.
    #[inline(always)]
    fn peek(&self, n: u32) -> usize {
        (self.acc & ((1u64 << n) - 1)) as usize
    }

    #[inline(always)]
    fn consume(&mut self, n: u32) {
        if n > self.count {
            self.fail("truncated deflate stream");
            self.count = 0;
            self.acc = 0;
            return;
        }
        self.acc >>= n;
        self.count -= n;
    }

    #[inline(always)]
    fn take(&mut self, n: u32) -> usize {
        let value = self.peek(n);
        self.consume(n);
        value
    }

    #[inline(always)]
    fn fail(&mut self, msg: &'static str) {
        if self.failure.is_none() {
            self.failure = Some(msg);
        }
    }

    fn check(&self) -> Result<()> {
        match self.failure {
            Some(msg) => Err(Error::Inflate(msg.into())),
            None => Ok(()),
        }
    }

    /// Drops the rest of the current byte and returns buffered whole bytes to the input.
    fn align_to_byte(&mut self) {
        let partial = self.count % 8;
        self.acc >>= partial;
        self.count -= partial;
        self.pos -= (self.count / 8) as usize;
        self.acc = 0;
        self.count = 0;
    }
}

/// Returned by [`Huffman::decode`] for a bit pattern that is no code.
const INVALID: u16 = u16::MAX;

/// A canonical Huffman code: a lookup table for codes up to `bits` long,
/// plus the first code, count and symbols of every longer length.
struct Huffman {
    /// Width of the lookup table in bits.
    bits: u32,
    /// `(1 << bits) - 1`, the accumulator mask for a lookup.
    mask: u64,
    /// Indexed by the next `bits` bits of input: `symbol << 4 | length`, or 0.
    fast: [u16; FAST_SIZE],
    count: [u16; MAX_BITS + 1],
    /// The first canonical code of each length.
    first: [u32; MAX_BITS + 1],
    /// Where each length's symbols start in `long_symbols`.
    long_start: [u16; MAX_BITS + 2],
    /// Symbols whose code is longer than `bits`, ordered by length then value.
    long_symbols: [u16; MAX_SYMBOLS],
}

impl Huffman {
    const EMPTY: Self = Self {
        bits: LITERAL_BITS,
        mask: (1 << LITERAL_BITS) - 1,
        fast: [0; FAST_SIZE],
        count: [0; MAX_BITS + 1],
        first: [0; MAX_BITS + 1],
        long_start: [0; MAX_BITS + 2],
        long_symbols: [0; MAX_SYMBOLS],
    };

    /// Builds the code from per-symbol code lengths (0 = unused) in one pass
    /// over the symbols, with a `bits`-wide lookup table; rejects
    /// over-subscribed sets, tolerates incomplete ones.
    fn build(&mut self, lengths: &[u8], bits: u32) -> Result<()> {
        let mut count = [0u16; MAX_BITS + 1];
        for &len in lengths {
            count[usize::from(len)] += 1;
        }
        self.build_counted(lengths, count, bits)
    }

    /// [`Huffman::build`] with the per-length counts already known.
    fn build_counted(
        &mut self,
        lengths: &[u8],
        count: [u16; MAX_BITS + 1],
        bits: u32,
    ) -> Result<()> {
        self.bits = bits;
        self.mask = (1u64 << bits) - 1;
        self.count = count;
        let fast_bits = bits as usize;
        let fast_size = 1usize << bits;
        self.count[0] = 0;
        let mut left: i32 = 1;
        let mut code: u32 = 0;
        let mut next_code = [0u32; MAX_BITS + 1];
        let mut long_next = [0u16; MAX_BITS + 2];
        for len in 1..=MAX_BITS {
            left <<= 1;
            left -= i32::from(self.count[len]);
            if left < 0 {
                return Err(invalid("over-subscribed huffman code"));
            }
            self.first[len] = code;
            next_code[len] = code;
            code = (code + u32::from(self.count[len])) << 1;
            self.long_start[len + 1] = self.long_start[len]
                + match len > fast_bits {
                    true => self.count[len],
                    false => 0,
                };
            long_next[len] = self.long_start[len];
        }
        let table = &mut self.fast[..fast_size];
        table.fill(0);
        for (symbol, &len) in lengths.iter().enumerate() {
            let len = usize::from(len);
            if len == 0 {
                continue;
            }
            let code = next_code[len];
            next_code[len] += 1;
            if len > fast_bits {
                self.long_symbols[usize::from(long_next[len])] = symbol as u16;
                long_next[len] += 1;
                continue;
            }
            let reversed = usize::from((code as u16).reverse_bits() >> (16 - len));
            let entry = (symbol as u16) << 4 | len as u16;
            let step = 1 << len;
            let mut slot = reversed;
            while slot < table.len() {
                table[slot] = entry;
                slot += step;
            }
        }
        Ok(())
    }

    /// Decodes one symbol, or `INVALID`; `bits` should hold `MAX_BITS` bits unless the input has ended.
    #[inline(always)]
    fn decode(&self, bits: &mut Bits<'_>) -> u16 {
        let entry = self.fast[(bits.acc & self.mask) as usize];
        if entry != 0 {
            bits.consume(u32::from(entry & 15));
            return entry >> 4;
        }
        self.decode_long(bits)
    }

    /// The canonical lookup for codes longer than the table width.
    fn decode_long(&self, bits: &mut Bits<'_>) -> u16 {
        let peeked = bits.peek(MAX_BITS as u32) as u16;
        for len in self.bits as usize + 1..=MAX_BITS {
            let code = u32::from((peeked & ((1 << len) - 1)).reverse_bits() >> (16 - len));
            let offset = code.wrapping_sub(self.first[len]);
            if offset < u32::from(self.count[len]) {
                bits.consume(len as u32);
                return self.long_symbols[usize::from(self.long_start[len]) + offset as usize];
            }
        }
        bits.fail("invalid huffman code");
        INVALID
    }
}

/// The fixed literal/length and distance codes of block type 1 (3.2.6).
fn fixed_codes() -> &'static (Huffman, Huffman) {
    static FIXED: OnceLock<Box<(Huffman, Huffman)>> = OnceLock::new();
    FIXED.get_or_init(|| {
        let mut lengths = [8u8; MAX_SYMBOLS];
        lengths[144..256].fill(9);
        lengths[256..280].fill(7);
        let mut literals = Huffman::EMPTY;
        let mut distances = Huffman::EMPTY;
        literals
            .build(&lengths, LITERAL_BITS)
            .unwrap_or_else(|_| unreachable!("the fixed literal code is complete"));
        distances
            .build(&[5u8; 30], DISTANCE_BITS)
            .unwrap_or_else(|_| unreachable!("the fixed distance code is complete"));
        Box::new((literals, distances))
    })
}

/// The tables a dynamic block needs: code lengths, literals, distances.
struct Tables {
    code_lengths: Huffman,
    literals: Huffman,
    distances: Huffman,
}

thread_local! {
    /// Kept per thread so a call does not zero several kilobytes of tables on the stack.
    static TABLES: std::cell::RefCell<Box<Tables>> = std::cell::RefCell::new(Box::new(Tables {
        code_lengths: Huffman::EMPTY,
        literals: Huffman::EMPTY,
        distances: Huffman::EMPTY,
    }));
}

/// The output being produced: `buf` is kept longer than the bytes written
/// so literals land through an index instead of a push.
struct Out<'a> {
    buf: &'a mut Vec<u8>,
    /// Where this stream's output began in `buf`.
    base: usize,
    /// Next byte to write.
    at: usize,
    /// The most this stream may write.
    limit: usize,
}

impl Out<'_> {
    /// Room for `extra` more bytes; false when the buffer cannot hold them
    /// within the limit.
    #[inline(always)]
    fn ensure(&mut self, extra: usize) -> bool {
        self.at + extra <= self.buf.len() || self.grow(extra)
    }

    /// The end of the buffer at its fullest: the limit plus the 16 bytes a
    /// chunked copy overshoots into.
    fn ceiling(&self) -> usize {
        self.base.saturating_add(self.limit).saturating_add(16)
    }

    /// Grows the buffer for `extra` more bytes, never past `ceiling`. Every
    /// write comes here once the buffer is full, so the buffer itself holds
    /// the limit for literals, matches and stored blocks alike, and costs
    /// nothing while it has room.
    #[cold]
    #[inline(never)]
    fn grow(&mut self, extra: usize) -> bool {
        let ceiling = self.ceiling();
        if self.at + extra > ceiling {
            return false;
        }
        let grown = (self.buf.len() * 2).max(self.at + extra + 256).min(ceiling);
        self.buf.resize(grown, 0);
        true
    }

    /// Appends `byte`; false when that passes the limit.
    #[inline(always)]
    fn literal(&mut self, byte: u8) -> bool {
        if !self.ensure(1) {
            return false;
        }
        self.buf[self.at] = byte;
        self.at += 1;
        true
    }

    /// Appends `length` bytes copied from `distance` bytes back, a distance
    /// the caller has checked; false when that passes the limit. Copies run
    /// in 16- or 8-byte chunks that may overshoot `length`; the overshoot
    /// lands in slack that later output overwrites.
    #[inline(always)]
    fn copy_match(&mut self, distance: usize, length: usize) -> bool {
        if !self.ensure(length + 16) {
            return false;
        }
        let start = self.at - distance;
        if distance >= 16 {
            let mut offset = 0;
            while offset < length {
                let mut chunk = [0u8; 16];
                chunk.copy_from_slice(&self.buf[start + offset..start + offset + 16]);
                self.buf[self.at + offset..self.at + offset + 16].copy_from_slice(&chunk);
                offset += 16;
            }
            self.at += length;
            return true;
        }
        if distance >= 8 {
            let mut offset = 0;
            while offset < length {
                let mut chunk = [0u8; 8];
                chunk.copy_from_slice(&self.buf[start + offset..start + offset + 8]);
                self.buf[self.at + offset..self.at + offset + 8].copy_from_slice(&chunk);
                offset += 8;
            }
            self.at += length;
            return true;
        }
        if distance == 1 {
            let byte = self.buf[start];
            self.buf[self.at..self.at + length].fill(byte);
            self.at += length;
            return true;
        }
        let mut from = start;
        let mut remaining = length;
        while remaining > 0 {
            let chunk = remaining.min(distance);
            self.buf.copy_within(from..from + chunk, self.at);
            from += chunk;
            self.at += chunk;
            remaining -= chunk;
        }
        true
    }
}

/// Inflates a raw DEFLATE stream into `out` (appended); `expected` sizes the
/// reservation. Trailing bytes after the final block are ignored. The output
/// is held to `MAX_PART_BYTES` by what the stream actually produces, never by
/// a declared size.
pub fn inflate(input: &[u8], expected: usize, out: &mut Vec<u8>) -> Result<()> {
    inflate_within(input, expected, MAX_PART_BYTES, out)
}

/// `inflate` held to `limit` output bytes.
pub fn inflate_within(
    input: &[u8],
    expected: usize,
    limit: usize,
    out: &mut Vec<u8>,
) -> Result<()> {
    let base = out.len();
    let most = input.len().saturating_mul(MAX_RATIO).saturating_add(64);
    let mut out = Out {
        buf: out,
        base,
        at: base,
        limit,
    };
    let reserved = (expected.min(most).min(limit).max(64) + 64).min(out.ceiling() - base);
    out.buf.resize(base + reserved, 0);
    let mut bits = Bits::new(input);
    let result =
        inflate_blocks(&mut bits, &mut out).and_then(|()| match out.at - out.base > limit {
            true => Err(past_limit(limit)),
            false => Ok(()),
        });
    let written = out.at;
    out.buf.truncate(written);
    result
}

fn inflate_blocks(bits: &mut Bits<'_>, out: &mut Out<'_>) -> Result<()> {
    loop {
        bits.refill();
        let last = bits.take(1) == 1;
        let kind = bits.take(2);
        bits.check()?;
        match kind {
            0 => stored_block(bits, out)?,
            1 => {
                let (literals, distances) = fixed_codes();
                decode_block(bits, literals, distances, out)?;
            }
            2 => TABLES.with(|cell| {
                let mut tables = cell.borrow_mut();
                dynamic_codes(bits, &mut tables)?;
                decode_block(bits, &tables.literals, &tables.distances, out)
            })?,
            _ => return Err(invalid("reserved deflate block type")),
        }
        if last {
            return Ok(());
        }
    }
}

fn stored_block(bits: &mut Bits<'_>, out: &mut Out<'_>) -> Result<()> {
    bits.align_to_byte();
    let data = bits.data;
    let pos = bits.pos;
    if pos + 4 > data.len() {
        return Err(truncated());
    }
    let len = usize::from(u16::from_le_bytes([data[pos], data[pos + 1]]));
    let check = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
    if len as u16 != !check {
        return Err(invalid("stored block length check failed"));
    }
    let start = pos + 4;
    let end = start + len;
    if end > data.len() {
        return Err(truncated());
    }
    if !out.ensure(len) {
        return Err(past_limit(out.limit));
    }
    out.buf[out.at..out.at + len].copy_from_slice(&data[start..end]);
    out.at += len;
    bits.pos = end;
    Ok(())
}

/// Reads the code length tables of a dynamic block (3.2.7) into `tables`.
fn dynamic_codes(bits: &mut Bits<'_>, tables: &mut Tables) -> Result<()> {
    let hlit = bits.take(5) + 257;
    let hdist = bits.take(5) + 1;
    let hclen = bits.take(4) + 4;
    if hlit > 286 || hdist > 30 {
        return Err(invalid("too many literal or distance codes"));
    }
    let mut code_lengths = [0u8; 19];
    for &position in &CODE_LENGTH_ORDER[..hclen] {
        bits.refill();
        code_lengths[position] = bits.take(3) as u8;
    }
    bits.check()?;
    let code_length_code = &mut tables.code_lengths;
    code_length_code.build(&code_lengths, CODE_LENGTH_BITS)?;
    let total = hlit + hdist;
    let mut lengths = [0u8; MAX_SYMBOLS + 32];
    let mut literal_count = [0u16; MAX_BITS + 1];
    let mut distance_count = [0u16; MAX_BITS + 1];
    let mut i = 0;
    while i < total {
        bits.refill();
        let symbol = code_length_code.decode(bits);
        let (value, repeat) = match symbol {
            0..=15 => (symbol as u8, 1),
            16 => {
                if i == 0 {
                    return Err(invalid("code length repeat with no previous length"));
                }
                (lengths[i - 1], 3 + bits.take(2))
            }
            17 => (0, 3 + bits.take(3)),
            18 => (0, 11 + bits.take(7)),
            _ => return bits.check().and(Err(invalid("invalid code length symbol"))),
        };
        let end = i + repeat;
        if end > total {
            return Err(invalid("code length repeat runs past the table"));
        }
        lengths[i..end].fill(value);
        let literal_part = end.min(hlit).saturating_sub(i);
        literal_count[usize::from(value)] += literal_part as u16;
        distance_count[usize::from(value)] += (repeat - literal_part) as u16;
        i = end;
    }
    bits.check()?;
    if lengths[usize::from(END_OF_BLOCK)] == 0 {
        return Err(invalid("no end-of-block code"));
    }
    tables
        .literals
        .build_counted(&lengths[..hlit], literal_count, LITERAL_BITS)?;
    tables
        .distances
        .build_counted(&lengths[hlit..total], distance_count, DISTANCE_BITS)?;
    Ok(())
}

fn decode_block(
    bits: &mut Bits<'_>,
    literals: &Huffman,
    distances: &Huffman,
    out: &mut Out<'_>,
) -> Result<()> {
    loop {
        if bits.count < 48 {
            bits.refill();
            bits.check()?;
        }
        let symbol = literals.decode(bits);
        if symbol < END_OF_BLOCK {
            if !out.literal(symbol as u8) {
                return Err(past_limit(out.limit));
            }
            continue;
        }
        if symbol == END_OF_BLOCK {
            return bits.check();
        }
        let length_code = usize::from(symbol.wrapping_sub(END_OF_BLOCK + 1));
        if length_code >= LENGTH_BASE.len() {
            bits.check()?;
            return Err(invalid("invalid length code"));
        }
        let length =
            usize::from(LENGTH_BASE[length_code]) + bits.take(u32::from(LENGTH_EXTRA[length_code]));
        let distance_code = usize::from(distances.decode(bits));
        if distance_code >= DIST_BASE.len() {
            bits.check()?;
            return Err(invalid("invalid distance code"));
        }
        let distance =
            usize::from(DIST_BASE[distance_code]) + bits.take(u32::from(DIST_EXTRA[distance_code]));
        bits.check()?;
        if distance > out.at - out.base {
            return Err(invalid("distance reaches before the start of output"));
        }
        if !out.copy_match(distance, length) {
            return Err(past_limit(out.limit));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::write::DeflateEncoder;
    use flate2::Compression;

    use super::*;

    fn deflate(data: &[u8], level: Compression) -> Vec<u8> {
        let mut encoder = DeflateEncoder::new(Vec::new(), level);
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    fn inflated(stream: &[u8], expected: usize) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        inflate(stream, expected, &mut out)?;
        Ok(out)
    }

    /// A deterministic pseudo-random byte sequence.
    fn noise(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect()
    }

    fn samples() -> Vec<Vec<u8>> {
        let xml = br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Hello</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
        let mut skewed = Vec::new();
        for value in 0..=255u32 {
            let repeats = 1 + (1u32 << (value % 16)) / 8;
            skewed.extend(std::iter::repeat_n(value as u8, repeats as usize));
        }
        vec![
            Vec::new(),
            b"a".to_vec(),
            b"abcabcabcabcabcabcabcabcabc".to_vec(),
            xml.repeat(40),
            noise(100_000, 7),
            skewed.repeat(20),
            [xml.to_vec(), noise(5000, 3), xml.repeat(3), vec![0; 70_000]].concat(),
        ]
    }

    #[test]
    fn round_trips_every_compression_level() {
        for data in samples() {
            for level in [
                Compression::none(),
                Compression::fast(),
                Compression::default(),
                Compression::best(),
            ] {
                let stream = deflate(&data, level);
                assert_eq!(inflated(&stream, data.len()).unwrap(), data);
                assert_eq!(inflated(&stream, 0).unwrap(), data);
            }
        }
    }

    #[test]
    fn hand_made_blocks() {
        let stored = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflated(&stored, 3).unwrap(), b"abc");
        let fixed = [0x4b, 0x04, 0x00];
        assert_eq!(inflated(&fixed, 1).unwrap(), b"a");
        let empty_fixed = [0x03, 0x00];
        assert_eq!(inflated(&empty_fixed, 0).unwrap(), b"");
    }

    #[test]
    fn short_matches_that_overlap_their_source_copy_byte_by_byte() {
        let data: Vec<u8> = (0..20u8)
            .chain(std::iter::repeat_n(b'x', 3))
            .chain(10..22u8)
            .chain(0..20u8)
            .collect();
        let stream = deflate(&data, Compression::best());
        assert_eq!(inflated(&stream, data.len()).unwrap(), data);
        let mut out = Out {
            buf: &mut vec![0u8; 64],
            base: 0,
            at: 0,
            limit: usize::MAX,
        };
        for byte in b"abcdefghijklm" {
            assert!(out.literal(*byte));
        }
        assert!(out.copy_match(10, 12));
        assert_eq!(&out.buf[..25], b"abcdefghijklmdefghijklmde");
    }

    #[test]
    fn appends_after_existing_output() {
        let mut out = b"head:".to_vec();
        inflate(&deflate(b"tail", Compression::default()), 4, &mut out).unwrap();
        assert_eq!(out, b"head:tail");
    }

    #[test]
    fn rejects_truncated_and_corrupt_streams_without_panicking() {
        let data = samples()[3].clone();
        let stream = deflate(&data, Compression::default());
        for cut in [0, 1, 2, 5, stream.len() / 2, stream.len() - 1] {
            assert!(inflated(&stream[..cut], data.len()).is_err());
        }
        let mut corrupt = stream.clone();
        corrupt[0] = 0x07;
        assert!(inflated(&corrupt, data.len()).is_err());
        let mut state = 99u64;
        for _ in 0..2000 {
            let mut mutated = stream.clone();
            for _ in 0..3 {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let index = (state >> 33) as usize % mutated.len();
                mutated[index] ^= 1 << ((state >> 20) & 7);
            }
            let _ = inflated(&mutated, data.len());
        }
    }

    #[test]
    fn long_codes_take_the_slow_path() {
        let mut lengths = [0u8; MAX_SYMBOLS];
        for (symbol, len) in lengths.iter_mut().enumerate() {
            *len = match symbol {
                0..=15 => 15,
                16..=255 => 9,
                256 => 8,
                _ => 0,
            };
        }
        let mut code = Huffman::EMPTY;
        code.build(&lengths, LITERAL_BITS).unwrap();
        assert!(code
            .fast
            .iter()
            .all(|&entry| entry == 0 || u32::from(entry & 15) <= LITERAL_BITS));
        assert_eq!(code.count[15], 16);
        assert_eq!(&code.long_symbols[..16], &(0..16).collect::<Vec<u16>>()[..]);
        let stream = deflate(&samples()[5], Compression::best());
        assert_eq!(inflated(&stream, 0).unwrap(), samples()[5]);
    }

    #[test]
    fn over_subscribed_codes_are_rejected() {
        let mut code = Huffman::EMPTY;
        assert!(code.build(&[1, 1, 1], DISTANCE_BITS).is_err());
        assert!(code.build(&[1, 1], DISTANCE_BITS).is_ok());
        assert!(code.build(&[1], DISTANCE_BITS).is_ok());
    }

    #[test]
    fn a_stream_inflating_past_the_ratio_is_refused() {
        // 32 MiB of zeros deflates about 1000:1, far past any real part.
        let stream = deflate(&vec![0u8; 32 << 20], Compression::default());
        let mut out = Vec::new();
        let error = inflate_within(&stream, 0, ratio_limit(stream.len()), &mut out).unwrap_err();
        assert!(error.to_string().contains("inflate limit"), "{error}");
    }

    #[test]
    fn a_stream_inflating_past_the_part_limit_is_refused() {
        let stream = deflate(&noise(4096, 7).repeat(64), Compression::default());
        let mut out = Vec::new();
        let error = inflate_within(&stream, 0, 4096, &mut out).unwrap_err();
        assert!(error.to_string().contains("inflate limit"), "{error}");
    }

    #[test]
    fn a_small_part_is_exempt_from_the_ratio() {
        // 512 KiB of zeros inflates well past the ratio but under the exemption.
        let stream = deflate(&vec![0u8; 512 << 10], Compression::default());
        let mut out = Vec::new();
        inflate_within(&stream, 0, ratio_limit(stream.len()), &mut out).unwrap();
        assert_eq!(out, vec![0u8; 512 << 10]);
    }

    /// Bits written least significant first, as DEFLATE reads them.
    #[derive(Default)]
    struct BitWriter {
        bytes: Vec<u8>,
        pending: u64,
        used: u32,
    }

    impl BitWriter {
        fn put(&mut self, value: u64, width: u32) {
            self.pending |= value << self.used;
            self.used += width;
            while self.used >= 8 {
                self.bytes.push(self.pending as u8);
                self.pending >>= 8;
                self.used -= 8;
            }
        }

        /// A run of `count` zero code lengths, as code length symbols 18
        /// (runs of 11 to 138).
        fn zero_lengths(&mut self, mut count: u64) {
            while count > 0 {
                let run = count.min(138);
                self.put(1, 1);
                self.put(run - 11, 7);
                count -= run;
            }
        }

        fn finish(mut self) -> Vec<u8> {
            if self.used > 0 {
                self.bytes.push(self.pending as u8);
            }
            self.bytes
        }
    }

    /// One dynamic block whose only codes are `a` and end-of-block, one bit
    /// each: `count` literals in `count` bits, with no match at all.
    fn literal_stream(count: usize) -> Vec<u8> {
        let mut bits = BitWriter::default();
        const FINAL_BLOCK: u64 = 1;
        const DYNAMIC_CODES: u64 = 2;
        const MINIMUM_LITERAL_CODES: u64 = 0;
        const MINIMUM_DISTANCE_CODES: u64 = 0;
        const CODE_LENGTH_CODES_SENT: u64 = 14;
        bits.put(FINAL_BLOCK, 1);
        bits.put(DYNAMIC_CODES, 2);
        bits.put(MINIMUM_LITERAL_CODES, 5);
        bits.put(MINIMUM_DISTANCE_CODES, 5);
        bits.put(CODE_LENGTH_CODES_SENT, 4);
        for symbol in &CODE_LENGTH_ORDER[..18] {
            bits.put(u64::from(*symbol == 1 || *symbol == 18), 3);
        }
        bits.zero_lengths(u64::from(b'a'));
        bits.put(0, 1);
        bits.zero_lengths(u64::from(END_OF_BLOCK) - u64::from(b'a') - 1);
        bits.put(0, 1);
        bits.put(0, 1);
        for _ in 0..count {
            bits.put(0, 1);
        }
        bits.put(1, 1);
        bits.finish()
    }

    #[test]
    fn a_stream_of_literals_alone_is_held_to_the_limit() {
        // One-bit literal codes inflate 8x without a single match.
        let stream = literal_stream(1 << 16);
        let mut out = Vec::new();
        inflate_within(&stream, 0, usize::MAX, &mut out).unwrap();
        assert_eq!(out, vec![b'a'; 1 << 16]);
        out.clear();
        let error = inflate_within(&stream, 0, 4096, &mut out).unwrap_err();
        assert!(error.to_string().contains("inflate limit"), "{error}");
    }

    #[test]
    fn the_limit_is_exact_for_literals() {
        for (count, limit, accepted) in [(4096, 4096, true), (4097, 4096, false), (8, 4, false)] {
            let mut out = Vec::new();
            let result = inflate_within(&literal_stream(count), 0, limit, &mut out);
            assert_eq!(result.is_ok(), accepted, "{count} literals, limit {limit}");
        }
    }
}
