//! MicroType Express (MTX), the compressed form of embedded font data. The
//! font's tables go into Compact Table Format: `loca` is dropped, `cvt `
//! is delta coded, and each `glyf` outline is re-coded as point triplets
//! with its instructions split into a push stream and a code stream. The
//! three streams are then packed with LZCOMP. A decoder rebuilds `glyf`
//! and `loca` from the streams, as it does for PowerPoint's own font parts.

use crate::lzcomp;

const VERSION: u8 = 3;
/// Stands in for the contour count when the bounding box must be stored.
const BBOX_MARKER: i16 = 0x7FFF;
const OFF_CURVE: u8 = 0x80;
const ARGS_ARE_WORDS: u16 = 0x01;
const HAVE_SCALE: u16 = 0x08;
const MORE_COMPONENTS: u16 = 0x20;
const HAVE_XY_SCALE: u16 = 0x40;
const HAVE_2X2: u16 = 0x80;
const HAVE_INSTRUCTIONS: u16 = 0x100;
const NPUSHB: u8 = 0x40;
const NPUSHW: u8 = 0x41;
const PUSHB: u8 = 0xB0;
const PUSHW: u8 = 0xB8;
/// The largest `glyf` a short `loca` can address.
const SHORT_LOCA_LIMIT: usize = 0x1FFFE;

fn be16(data: &[u8], at: usize) -> Option<u16> {
    let bytes: [u8; 2] = data.get(at..at + 2)?.try_into().ok()?;
    Some(u16::from_be_bytes(bytes))
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

struct Table<'a> {
    tag: [u8; 4],
    checksum: u32,
    data: &'a [u8],
}

fn tables(font: &[u8]) -> Option<Vec<Table<'_>>> {
    let count = be16(font, 4)? as usize;
    (0..count)
        .map(|i| {
            let record = 12 + i * 16;
            let tag: [u8; 4] = font.get(record..record + 4)?.try_into().ok()?;
            let offset = be32(font, record + 8)? as usize;
            let length = be32(font, record + 12)? as usize;
            Some(Table {
                tag,
                checksum: be32(font, record + 4)?,
                data: font.get(offset..offset + length)?,
            })
        })
        .collect()
}

/// A count up to 65535 in one to three bytes.
fn push_u255(out: &mut Vec<u8>, value: u16) {
    match value {
        0..=252 => out.push(value as u8),
        253..=505 => out.extend_from_slice(&[255, (value - 253) as u8]),
        506..=761 => out.extend_from_slice(&[254, (value - 506) as u8]),
        _ => {
            out.push(253);
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
}

/// A signed value in one to three bytes; never starts with the hop codes
/// 0xFB and 0xFC, so a decoder reads it as a plain value.
fn push_s255(out: &mut Vec<u8>, value: i16) {
    let magnitude = value.unsigned_abs();
    if magnitude > 755 {
        out.push(253);
        out.extend_from_slice(&value.to_be_bytes());
        return;
    }
    if value < 0 {
        out.push(250);
    }
    match magnitude {
        0..=249 => out.push(magnitude as u8),
        250..=499 => out.extend_from_slice(&[255, (magnitude - 250) as u8]),
        _ => out.extend_from_slice(&[254, (magnitude - 500) as u8]),
    }
}

/// The triplet index and trailing bytes for one point's deltas: the
/// smallest encoding whose ranges hold both values.
fn triplet(dx: i16, dy: i16) -> (u8, Vec<u8>) {
    let (ax, ay) = (dx.unsigned_abs() as u32, dy.unsigned_abs() as u32);
    let signs = u8::from(dx > 0) | (u8::from(dy > 0) << 1);
    if dx == 0 && dy != 0 && ay < 1280 {
        let index = (2 * (ay / 256)) as u8 | u8::from(dy > 0);
        return (index, vec![(ay % 256) as u8]);
    }
    if dy == 0 && dx != 0 && ax < 1280 {
        let index = (10 + 2 * (ax / 256)) as u8 | u8::from(dx > 0);
        return (index, vec![(ax % 256) as u8]);
    }
    if dx != 0 && dy != 0 && ax <= 64 && ay <= 64 {
        let (xg, yg) = ((ax - 1) / 16, (ay - 1) / 16);
        let index = 20 + ((xg * 4 + yg) * 4) as u8 + signs;
        return (
            index,
            vec![((((ax - 1) % 16) << 4) | ((ay - 1) % 16)) as u8],
        );
    }
    if dx != 0 && dy != 0 && ax <= 768 && ay <= 768 {
        let (xg, yg) = ((ax - 1) / 256, (ay - 1) / 256);
        let index = 84 + ((xg * 3 + yg) * 4) as u8 + signs;
        return (index, vec![((ax - 1) % 256) as u8, ((ay - 1) % 256) as u8]);
    }
    if ax < 4096 && ay < 4096 {
        let packed = (ax << 12) | ay;
        return (120 + signs, packed.to_be_bytes()[1..].to_vec());
    }
    let mut bytes = (ax as u16).to_be_bytes().to_vec();
    bytes.extend_from_slice(&(ay as u16).to_be_bytes());
    (124 + signs, bytes)
}

/// The values of the push instructions that open `code`, and how many
/// bytes those instructions take.
fn leading_pushes(code: &[u8]) -> (Vec<i16>, usize) {
    let mut values = Vec::new();
    let mut at = 0usize;
    while at < code.len() {
        let op = code[at];
        let (count, word, header) = match op {
            NPUSHB => (code.get(at + 1).copied().unwrap_or(0) as usize, false, 2),
            NPUSHW => (code.get(at + 1).copied().unwrap_or(0) as usize, true, 2),
            PUSHB..=0xB7 => ((op - PUSHB) as usize + 1, false, 1),
            PUSHW..=0xBF => ((op - PUSHW) as usize + 1, true, 1),
            _ => break,
        };
        let width = match word {
            true => 2,
            false => 1,
        };
        let end = at + header + count * width;
        if end > code.len() || count == 0 {
            break;
        }
        for i in 0..count {
            let start = at + header + i * width;
            values.push(match word {
                true => i16::from_be_bytes([code[start], code[start + 1]]),
                false => code[start] as i16,
            });
        }
        at = end;
    }
    (values, at)
}

/// The bytes a decoder writes for `values`: runs of bytes and words, each
/// run at most 255 long, as PUSHB, PUSHW, NPUSHB or NPUSHW.
fn reissued_length(values: &[i16]) -> usize {
    let mut total = 0usize;
    let mut run: Option<(bool, usize)> = None;
    let flush = |run: Option<(bool, usize)>| -> usize {
        let Some((word, count)) = run else {
            return 0;
        };
        let header = match count < 8 {
            true => 1,
            false => 2,
        };
        header + count * if word { 2 } else { 1 }
    };
    for &value in values {
        let word = !(0..256).contains(&value);
        match run {
            Some((kind, count)) if kind == word && count < 255 => run = Some((kind, count + 1)),
            _ => {
                total += flush(run);
                run = Some((word, 1));
            }
        }
    }
    total + flush(run)
}

/// The three streams of the Compact Table Format `glyf` data and the size
/// the decoder's rebuilt `glyf` will have.
#[derive(Default)]
struct Streams {
    glyphs: Vec<u8>,
    pushes: Vec<u8>,
    code: Vec<u8>,
    rebuilt: usize,
}

impl Streams {
    /// Splits `code` into pushed values and the remaining instructions,
    /// keeping everything in the code stream when re-issuing the pushes
    /// would lengthen the glyph's instructions. Returns the rebuilt length.
    fn instructions(&mut self, code: &[u8]) -> usize {
        let (values, taken) = leading_pushes(code);
        let reissued = reissued_length(&values);
        let (values, taken, reissued) = match reissued <= taken {
            true => (values, taken, reissued),
            false => (Vec::new(), 0, 0),
        };
        push_u255(&mut self.glyphs, values.len() as u16);
        for value in &values {
            push_s255(&mut self.pushes, *value);
        }
        let rest = &code[taken..];
        push_u255(&mut self.glyphs, rest.len() as u16);
        self.code.extend_from_slice(rest);
        reissued + rest.len()
    }

    fn glyph(&mut self, data: &[u8]) -> Option<()> {
        let contours = be16(data, 0).map(|v| v as i16).unwrap_or(0);
        if contours == 0 {
            self.glyphs.extend_from_slice(&0i16.to_be_bytes());
            return Some(());
        }
        let rebuilt = match contours < 0 {
            true => self.composite(data)?,
            false => self.simple(data, contours as usize)?,
        };
        self.rebuilt += rebuilt + rebuilt % 2;
        Some(())
    }

    fn simple(&mut self, data: &[u8], contours: usize) -> Option<usize> {
        let bbox = [
            be16(data, 2)?,
            be16(data, 4)?,
            be16(data, 6)?,
            be16(data, 8)?,
        ];
        let end_points: Vec<u16> = (0..contours)
            .map(|i| be16(data, 10 + 2 * i))
            .collect::<Option<_>>()?;
        if end_points.windows(2).any(|pair| pair[1] <= pair[0]) {
            return None;
        }
        let points = *end_points.last()? as usize + 1;
        let mut at = 10 + 2 * contours;
        let code_len = be16(data, at)? as usize;
        let code = data.get(at + 2..at + 2 + code_len)?;
        at += 2 + code_len;
        let mut flags = Vec::with_capacity(points);
        while flags.len() < points {
            let flag = *data.get(at)?;
            at += 1;
            let repeats = match flag & 0x08 != 0 {
                true => {
                    at += 1;
                    *data.get(at - 1)? as usize
                }
                false => 0,
            };
            flags.extend(std::iter::repeat_n(flag, repeats + 1));
        }
        flags.truncate(points);
        let mut deltas = |short: u8, same: u8| -> Option<Vec<i16>> {
            flags
                .iter()
                .map(|&flag| {
                    if flag & short != 0 {
                        let value = *data.get(at)? as i16;
                        at += 1;
                        return Some(match flag & same != 0 {
                            true => value,
                            false => -value,
                        });
                    }
                    if flag & same != 0 {
                        return Some(0);
                    }
                    let value = be16(data, at)? as i16;
                    at += 2;
                    Some(value)
                })
                .collect()
        };
        let xs = deltas(0x02, 0x10)?;
        let ys = deltas(0x04, 0x20)?;
        let (mut x, mut y) = (0i16, 0i16);
        let mut extent = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
        for (dx, dy) in xs.iter().zip(&ys) {
            x = x.wrapping_add(*dx);
            y = y.wrapping_add(*dy);
            extent = [
                extent[0].min(x),
                extent[1].min(y),
                extent[2].max(x),
                extent[3].max(y),
            ];
        }
        let stored: [i16; 4] = bbox.map(|v| v as i16);
        if stored != extent {
            self.glyphs.extend_from_slice(&BBOX_MARKER.to_be_bytes());
        }
        self.glyphs
            .extend_from_slice(&(contours as i16).to_be_bytes());
        if stored != extent {
            for value in bbox {
                self.glyphs.extend_from_slice(&value.to_be_bytes());
            }
        }
        for (i, end) in end_points.iter().enumerate() {
            let count = match i {
                0 => *end,
                _ => end - end_points[i - 1],
            };
            push_u255(&mut self.glyphs, count);
        }
        let mut coordinates = Vec::with_capacity(points * 2);
        for ((flag, dx), dy) in flags.iter().zip(&xs).zip(&ys) {
            let (index, bytes) = triplet(*dx, *dy);
            let on_curve = flag & 0x01 != 0;
            self.glyphs
                .push(index | if on_curve { 0 } else { OFF_CURVE });
            coordinates.extend_from_slice(&bytes);
        }
        self.glyphs.extend_from_slice(&coordinates);
        let code_rebuilt = self.instructions(code);
        let coordinate_bytes = |values: &[i16]| -> usize {
            values
                .iter()
                .enumerate()
                .map(|(i, &v)| match (i, v) {
                    (0, v) | (_, v) if i == 0 || v != 0 => match v.unsigned_abs() < 256 {
                        true => 1,
                        false => 2,
                    },
                    _ => 0,
                })
                .sum()
        };
        Some(
            12 + 2 * contours
                + code_rebuilt
                + points
                + coordinate_bytes(&xs)
                + coordinate_bytes(&ys),
        )
    }

    fn composite(&mut self, data: &[u8]) -> Option<usize> {
        let mut at = 10usize;
        let mut flags;
        loop {
            flags = be16(data, at)?;
            at += 4;
            at += match flags & ARGS_ARE_WORDS != 0 {
                true => 4,
                false => 2,
            };
            at += match () {
                _ if flags & HAVE_2X2 != 0 => 8,
                _ if flags & HAVE_XY_SCALE != 0 => 4,
                _ if flags & HAVE_SCALE != 0 => 2,
                _ => 0,
            };
            if flags & MORE_COMPONENTS == 0 {
                break;
            }
        }
        let components = data.get(10..at)?;
        self.glyphs.extend_from_slice(data.get(0..10)?);
        self.glyphs.extend_from_slice(components);
        let mut rebuilt = 10 + components.len();
        if flags & HAVE_INSTRUCTIONS != 0 {
            let code_len = be16(data, at)? as usize;
            let code = data.get(at + 2..at + 2 + code_len)?;
            rebuilt += 2 + self.instructions(code);
        }
        Some(rebuilt)
    }
}

/// The `cvt ` table as a count and one delta per value.
fn control_values(data: &[u8]) -> Vec<u8> {
    let values: Vec<i16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i16::from_be_bytes(*pair))
        .collect();
    let mut out = Vec::with_capacity(values.len() + 2);
    out.extend_from_slice(&(values.len() as u16).to_be_bytes());
    let mut last = 0i16;
    for value in values {
        let delta = value.wrapping_sub(last);
        last = value;
        match delta {
            0..=237 => out.push(delta as u8),
            238..=2141 => {
                let group = delta / 238;
                out.extend_from_slice(&[247 + group as u8, (delta - 238 * group) as u8]);
            }
            -2141..=-1 => {
                let group = -delta / 238;
                out.extend_from_slice(&[239 + group as u8, (-delta - 238 * group) as u8]);
            }
            _ => {
                out.push(238);
                out.extend_from_slice(&delta.to_be_bytes());
            }
        }
    }
    out
}

fn u24(value: usize) -> Option<[u8; 3]> {
    if value > 0xFF_FFFF {
        return None;
    }
    let bytes = (value as u32).to_be_bytes();
    Some([bytes[1], bytes[2], bytes[3]])
}

/// The font as MTX data, or None when it cannot be coded: a table that
/// does not parse, or a stream too large for the format.
pub(crate) fn compress(font: &[u8]) -> Option<Vec<u8>> {
    let tables = tables(font)?;
    let find = |tag: &[u8; 4]| tables.iter().find(|t| &t.tag == tag).map(|t| t.data);
    let mut head = find(b"head")?.to_vec();
    let long_loca = be16(&head, 50)? != 0;
    let glyph_count = be16(find(b"maxp")?, 4)? as usize;
    let mut streams = Streams::default();
    if let (Some(glyf), Some(loca)) = (find(b"glyf"), find(b"loca")) {
        for i in 0..glyph_count {
            let (start, end) = match long_loca {
                true => (be32(loca, 4 * i)? as usize, be32(loca, 4 * i + 4)? as usize),
                false => (
                    be16(loca, 2 * i)? as usize * 2,
                    be16(loca, 2 * i + 2)? as usize * 2,
                ),
            };
            streams.glyph(glyf.get(start..end.max(start))?)?;
        }
        if !long_loca && streams.rebuilt > SHORT_LOCA_LIMIT {
            head[50..52].copy_from_slice(&1u16.to_be_bytes());
        }
    }
    let kept: Vec<&Table<'_>> = tables
        .iter()
        .filter(|t| &t.tag != b"hdmx" && &t.tag != b"VDMX")
        .collect();
    let directory_len = 12 + 16 * kept.len();
    let mut directory = Vec::with_capacity(directory_len);
    let mut body = Vec::with_capacity(font.len());
    directory.extend_from_slice(font.get(0..4)?);
    let count = kept.len() as u16;
    let power = 1u16 << (15 - count.max(1).leading_zeros());
    directory.extend_from_slice(&count.to_be_bytes());
    directory.extend_from_slice(&(power * 16).to_be_bytes());
    directory.extend_from_slice(&power.trailing_zeros().to_be_bytes()[2..]);
    directory.extend_from_slice(&(count * 16 - power * 16).to_be_bytes());
    for table in &kept {
        let content: Option<&[u8]> = match &table.tag {
            b"loca" => None,
            b"glyf" => Some(&streams.glyphs),
            b"head" => Some(&head),
            _ => Some(table.data),
        };
        let transformed;
        let content = match &table.tag {
            b"cvt " => {
                transformed = control_values(table.data);
                Some(transformed.as_slice())
            }
            _ => content,
        };
        directory.extend_from_slice(&table.tag);
        directory.extend_from_slice(&table.checksum.to_be_bytes());
        let Some(content) = content else {
            directory.extend_from_slice(&[0u8; 8]);
            continue;
        };
        directory.extend_from_slice(&((directory_len + body.len()) as u32).to_be_bytes());
        directory.extend_from_slice(&(content.len() as u32).to_be_bytes());
        body.extend_from_slice(content);
        body.resize(body.len().div_ceil(4) * 4, 0);
    }
    directory.extend_from_slice(&body);
    let blocks = [
        lzcomp::pack(&directory)?,
        lzcomp::pack(&streams.pushes)?,
        lzcomp::pack(&streams.code)?,
    ];
    let longest = blocks
        .iter()
        .map(|block| lzcomp::block_length(block))
        .max()
        .unwrap_or(0);
    let mut out = vec![VERSION];
    out.extend_from_slice(&u24((lzcomp::PRELOAD + longest).min(0xFF_FFFF))?);
    out.extend_from_slice(&u24(10 + blocks[0].len())?);
    out.extend_from_slice(&u24(10 + blocks[0].len() + blocks[1].len())?);
    for block in &blocks {
        out.extend_from_slice(block);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGULAR: &[u8] = include_bytes!("../tests/data/boxy-regular.ttf");
    const HINTED: &[u8] = include_bytes!("../tests/data/boxy-hinted.ttf");

    fn read_u255(data: &[u8], at: &mut usize) -> u16 {
        let code = data[*at];
        *at += 1;
        match code {
            253 => {
                *at += 2;
                u16::from_be_bytes([data[*at - 2], data[*at - 1]])
            }
            255 => {
                *at += 1;
                253 + data[*at - 1] as u16
            }
            254 => {
                *at += 1;
                506 + data[*at - 1] as u16
            }
            _ => code as u16,
        }
    }

    fn read_s255(data: &[u8], at: &mut usize) -> i16 {
        let mut code = data[*at];
        *at += 1;
        if code == 253 {
            *at += 2;
            return i16::from_be_bytes([data[*at - 2], data[*at - 1]]);
        }
        let mut sign = 1i16;
        if code == 250 {
            sign = -1;
            code = data[*at];
            *at += 1;
        }
        let value = match code {
            255 => {
                *at += 1;
                250 + data[*at - 1] as i16
            }
            254 => {
                *at += 1;
                500 + data[*at - 1] as i16
            }
            _ => code as i16,
        };
        value * sign
    }

    /// The reference triplet table row: byte count, x bits, y bits, x
    /// delta, y delta, x sign, y sign.
    fn triplet_row(index: u8) -> (usize, u32, u32, i32, i32, i32, i32) {
        let sign = |bit: u8| if bit != 0 { 1 } else { -1 };
        match index {
            0..=9 => (2, 0, 8, 0, 256 * (index / 2) as i32, 0, sign(index & 1)),
            10..=19 => (
                2,
                8,
                0,
                256 * ((index - 10) / 2) as i32,
                0,
                sign(index & 1),
                0,
            ),
            20..=83 => {
                let group = (index - 20) / 4;
                let (xg, yg) = ((group / 4) as i32, (group % 4) as i32);
                (
                    2,
                    4,
                    4,
                    1 + 16 * xg,
                    1 + 16 * yg,
                    sign(index & 1),
                    sign(index & 2),
                )
            }
            84..=119 => {
                let group = (index - 84) / 4;
                let (xg, yg) = ((group / 3) as i32, (group % 3) as i32);
                (
                    3,
                    8,
                    8,
                    1 + 256 * xg,
                    1 + 256 * yg,
                    sign(index & 1),
                    sign(index & 2),
                )
            }
            120..=123 => (4, 12, 12, 0, 0, sign(index & 1), sign(index & 2)),
            _ => (5, 16, 16, 0, 0, sign(index & 1), sign(index & 2)),
        }
    }

    fn read_triplet(index: u8, bytes: &[u8]) -> (i16, i16) {
        let (count, x_bits, y_bits, dx, dy, sx, sy) = triplet_row(index & 0x7F);
        assert_eq!(bytes.len(), count - 1);
        let mut bits = 0u64;
        for &b in bytes {
            bits = (bits << 8) | b as u64;
        }
        let total = (x_bits + y_bits) as u64;
        assert_eq!(total, 8 * (count as u64 - 1));
        let x = (bits >> y_bits) & ((1u64 << x_bits) - 1);
        let y = bits & ((1u64 << y_bits) - 1);
        ((sx * (x as i32 + dx)) as i16, (sy * (y as i32 + dy)) as i16)
    }

    /// One glyph decoded as the reference decoder rebuilds it: contour
    /// count, bounding box, end points, on-curve flags, deltas and
    /// instructions.
    #[derive(Debug, PartialEq, Eq)]
    struct Outline {
        contours: i16,
        bbox: [i16; 4],
        end_points: Vec<u16>,
        on_curve: Vec<bool>,
        xs: Vec<i16>,
        ys: Vec<i16>,
        instructions: Vec<u8>,
        components: Vec<u8>,
    }

    fn read_instructions(
        streams: &Streams,
        at: &mut usize,
        push_at: &mut usize,
        code_at: &mut usize,
    ) -> Vec<u8> {
        let push_count = read_u255(&streams.glyphs, at) as usize;
        let values: Vec<i16> = (0..push_count)
            .map(|_| read_s255(&streams.pushes, push_at))
            .collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < values.len() {
            let word = !(0..256).contains(&values[i]);
            let run = values[i..]
                .iter()
                .take(255)
                .take_while(|v| !(0..256).contains(*v) == word)
                .count();
            match (run < 8, word) {
                (true, false) => out.push(PUSHB | (run as u8 - 1)),
                (true, true) => out.push(PUSHW | (run as u8 - 1)),
                (false, false) => out.extend_from_slice(&[NPUSHB, run as u8]),
                (false, true) => out.extend_from_slice(&[NPUSHW, run as u8]),
            }
            for value in &values[i..i + run] {
                match word {
                    true => out.extend_from_slice(&value.to_be_bytes()),
                    false => out.push(*value as u8),
                }
            }
            i += run;
        }
        let code_len = read_u255(&streams.glyphs, at) as usize;
        out.extend_from_slice(&streams.code[*code_at..*code_at + code_len]);
        *code_at += code_len;
        out
    }

    fn decode_glyph(
        streams: &Streams,
        at: &mut usize,
        push_at: &mut usize,
        code_at: &mut usize,
    ) -> Outline {
        let s16 = |at: &mut usize| {
            *at += 2;
            i16::from_be_bytes([streams.glyphs[*at - 2], streams.glyphs[*at - 1]])
        };
        let mut contours = s16(at);
        let mut outline = Outline {
            contours,
            bbox: [0; 4],
            end_points: Vec::new(),
            on_curve: Vec::new(),
            xs: Vec::new(),
            ys: Vec::new(),
            instructions: Vec::new(),
            components: Vec::new(),
        };
        if contours == 0 {
            return outline;
        }
        if contours < 0 {
            outline.bbox = [s16(at), s16(at), s16(at), s16(at)];
            let start = *at;
            let mut flags;
            loop {
                flags = u16::from_be_bytes([streams.glyphs[*at], streams.glyphs[*at + 1]]);
                *at += 4;
                *at += if flags & ARGS_ARE_WORDS != 0 { 4 } else { 2 };
                *at += match () {
                    _ if flags & HAVE_2X2 != 0 => 8,
                    _ if flags & HAVE_XY_SCALE != 0 => 4,
                    _ if flags & HAVE_SCALE != 0 => 2,
                    _ => 0,
                };
                if flags & MORE_COMPONENTS == 0 {
                    break;
                }
            }
            outline.components = streams.glyphs[start..*at].to_vec();
            if flags & HAVE_INSTRUCTIONS != 0 {
                outline.instructions = read_instructions(streams, at, push_at, code_at);
            }
            return outline;
        }
        let mut stored_bbox = None;
        if contours == BBOX_MARKER {
            contours = s16(at);
            outline.contours = contours;
            stored_bbox = Some([s16(at), s16(at), s16(at), s16(at)]);
        }
        let mut total = 0u16;
        for i in 0..contours as usize {
            let count = read_u255(&streams.glyphs, at);
            total += count + u16::from(i == 0);
            outline.end_points.push(total - 1);
        }
        let points = total as usize;
        let flags = streams.glyphs[*at..*at + points].to_vec();
        *at += points;
        let (mut x, mut y) = (0i16, 0i16);
        let mut extent = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
        for flag in flags {
            let count = triplet_row(flag & 0x7F).0 - 1;
            let (dx, dy) = read_triplet(flag, &streams.glyphs[*at..*at + count]);
            *at += count;
            outline.on_curve.push(flag & OFF_CURVE == 0);
            outline.xs.push(dx);
            outline.ys.push(dy);
            x = x.wrapping_add(dx);
            y = y.wrapping_add(dy);
            extent = [
                extent[0].min(x),
                extent[1].min(y),
                extent[2].max(x),
                extent[3].max(y),
            ];
        }
        outline.bbox = stored_bbox.unwrap_or(extent);
        outline.instructions = read_instructions(streams, at, push_at, code_at);
        outline
    }

    /// The same outline read straight from a TrueType glyph record.
    fn parse_glyph(data: &[u8]) -> Outline {
        let s16 = |at: usize| i16::from_be_bytes([data[at], data[at + 1]]);
        let mut outline = Outline {
            contours: 0,
            bbox: [0; 4],
            end_points: Vec::new(),
            on_curve: Vec::new(),
            xs: Vec::new(),
            ys: Vec::new(),
            instructions: Vec::new(),
            components: Vec::new(),
        };
        if data.is_empty() {
            return outline;
        }
        outline.contours = s16(0);
        outline.bbox = [s16(2), s16(4), s16(6), s16(8)];
        if outline.contours < 0 {
            let mut at = 10;
            let mut flags;
            loop {
                flags = u16::from_be_bytes([data[at], data[at + 1]]);
                at += 4;
                at += if flags & ARGS_ARE_WORDS != 0 { 4 } else { 2 };
                at += match () {
                    _ if flags & HAVE_2X2 != 0 => 8,
                    _ if flags & HAVE_XY_SCALE != 0 => 4,
                    _ if flags & HAVE_SCALE != 0 => 2,
                    _ => 0,
                };
                if flags & MORE_COMPONENTS == 0 {
                    break;
                }
            }
            outline.components = data[10..at].to_vec();
            if flags & HAVE_INSTRUCTIONS != 0 {
                let len = u16::from_be_bytes([data[at], data[at + 1]]) as usize;
                outline.instructions = data[at + 2..at + 2 + len].to_vec();
            }
            return outline;
        }
        let contours = outline.contours as usize;
        outline.end_points = (0..contours)
            .map(|i| u16::from_be_bytes([data[10 + 2 * i], data[11 + 2 * i]]))
            .collect();
        let points = *outline.end_points.last().unwrap() as usize + 1;
        let mut at = 10 + 2 * contours;
        let len = u16::from_be_bytes([data[at], data[at + 1]]) as usize;
        outline.instructions = data[at + 2..at + 2 + len].to_vec();
        at += 2 + len;
        let mut flags = Vec::new();
        while flags.len() < points {
            let flag = data[at];
            at += 1;
            let repeats = match flag & 0x08 != 0 {
                true => {
                    at += 1;
                    data[at - 1] as usize
                }
                false => 0,
            };
            flags.extend(std::iter::repeat_n(flag, repeats + 1));
        }
        outline.on_curve = flags.iter().map(|f| f & 1 != 0).collect();
        let mut read = |short: u8, same: u8| -> Vec<i16> {
            flags
                .iter()
                .map(|&flag| {
                    if flag & short != 0 {
                        at += 1;
                        let v = data[at - 1] as i16;
                        return if flag & same != 0 { v } else { -v };
                    }
                    if flag & same != 0 {
                        return 0;
                    }
                    at += 2;
                    i16::from_be_bytes([data[at - 2], data[at - 1]])
                })
                .collect()
        };
        outline.xs = read(0x02, 0x10);
        outline.ys = read(0x04, 0x20);
        outline
    }

    fn glyph_records(font: &[u8]) -> Vec<&[u8]> {
        let tables = tables(font).unwrap();
        let find = |tag: &[u8; 4]| tables.iter().find(|t| &t.tag == tag).unwrap().data;
        let long = be16(find(b"head"), 50).unwrap() != 0;
        let loca = find(b"loca");
        let glyf = find(b"glyf");
        let count = be16(find(b"maxp"), 4).unwrap() as usize;
        (0..count)
            .map(|i| match long {
                true => {
                    &glyf[be32(loca, 4 * i).unwrap() as usize
                        ..be32(loca, 4 * i + 4).unwrap() as usize]
                }
                false => {
                    &glyf[be16(loca, 2 * i).unwrap() as usize * 2
                        ..be16(loca, 2 * i + 2).unwrap() as usize * 2]
                }
            })
            .collect()
    }

    /// Every glyph of `font` through the encoder and the test decoder.
    /// Re-issued push instructions may be shorter than the original ones,
    /// so instructions compare by their pushed values and remaining code.
    fn glyphs_round_trip(font: &[u8]) -> Streams {
        let records = glyph_records(font);
        let mut streams = Streams::default();
        for record in &records {
            streams.glyph(record).unwrap();
        }
        let (mut at, mut push_at, mut code_at) = (0, 0, 0);
        for (i, record) in records.iter().enumerate() {
            let mut decoded = decode_glyph(&streams, &mut at, &mut push_at, &mut code_at);
            let mut expected = parse_glyph(record);
            if expected.contours == 0 {
                expected.bbox = [0; 4];
            }
            let (values, taken) = leading_pushes(&decoded.instructions);
            let (original_values, original_taken) = leading_pushes(&expected.instructions);
            assert_eq!(values, original_values, "glyph {i} pushes");
            assert_eq!(
                decoded.instructions[taken..],
                expected.instructions[original_taken..],
                "glyph {i} code"
            );
            assert!(
                decoded.instructions.len() <= expected.instructions.len(),
                "glyph {i} grew"
            );
            decoded.instructions = expected.instructions.clone();
            assert_eq!(decoded, expected, "glyph {i}");
        }
        assert_eq!(at, streams.glyphs.len());
        assert_eq!(push_at, streams.pushes.len());
        assert_eq!(code_at, streams.code.len());
        streams
    }

    #[test]
    fn counts_and_values_round_trip() {
        for value in [0u16, 1, 252, 253, 254, 505, 506, 761, 762, 4000, 65535] {
            let mut out = Vec::new();
            push_u255(&mut out, value);
            let mut at = 0;
            assert_eq!(read_u255(&out, &mut at), value);
            assert_eq!(at, out.len());
        }
        for value in [
            0i16,
            1,
            -1,
            249,
            -249,
            250,
            -250,
            499,
            -499,
            500,
            -500,
            755,
            -755,
            756,
            -756,
            i16::MAX,
            i16::MIN,
        ] {
            let mut out = Vec::new();
            push_s255(&mut out, value);
            assert!(out[0] != 0xFB && out[0] != 0xFC);
            let mut at = 0;
            assert_eq!(read_s255(&out, &mut at), value, "{value}");
            assert_eq!(at, out.len());
        }
    }

    #[test]
    fn triplets_pick_the_shortest_encoding() {
        let samples = [
            0i16,
            1,
            -1,
            16,
            17,
            -17,
            64,
            -64,
            65,
            255,
            -255,
            256,
            -256,
            768,
            -768,
            769,
            1279,
            -1279,
            1280,
            4095,
            -4095,
            4096,
            i16::MAX,
            i16::MIN,
        ];
        for &dx in &samples {
            for &dy in &samples {
                let (index, bytes) = triplet(dx, dy);
                assert!(index < 128);
                assert_eq!(read_triplet(index, &bytes), (dx, dy), "{dx},{dy}");
            }
        }
        assert_eq!(triplet(0, 5).1.len(), 1);
        assert_eq!(triplet(3, -3).1.len(), 1);
        assert_eq!(triplet(300, 1).1.len(), 2);
        assert_eq!(triplet(3000, 1).1.len(), 3);
        assert_eq!(triplet(0, 0).1.len(), 3);
        assert_eq!(triplet(5000, 1).1.len(), 4);
    }

    #[test]
    fn control_values_are_delta_coded() {
        let mut data = Vec::new();
        for value in [
            0i16,
            100,
            100,
            337,
            338,
            2479,
            2480,
            -1000,
            -1001,
            5000,
            i16::MIN,
            i16::MAX,
        ] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let coded = control_values(&data);
        let count = u16::from_be_bytes([coded[0], coded[1]]) as usize;
        assert_eq!(count, 12);
        let mut at = 2;
        let mut last = 0i16;
        let mut decoded = Vec::new();
        for _ in 0..count {
            let code = coded[at];
            at += 1;
            let delta = match code {
                248..=255 => {
                    at += 1;
                    238 * (code as i16 - 247) + coded[at - 1] as i16
                }
                239..=247 => {
                    at += 1;
                    -(238 * (code as i16 - 239) + coded[at - 1] as i16)
                }
                238 => {
                    at += 2;
                    i16::from_be_bytes([coded[at - 2], coded[at - 1]])
                }
                _ => code as i16,
            };
            last = last.wrapping_add(delta);
            decoded.extend_from_slice(&last.to_be_bytes());
        }
        assert_eq!(at, coded.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn leading_pushes_split_off_when_they_do_not_grow() {
        let code = [
            PUSHB | 1,
            3,
            5,
            NPUSHW,
            2,
            1,
            44,
            255,
            156,
            0x21,
            0x21,
            0x21,
            0x21,
        ];
        let (values, taken) = leading_pushes(&code);
        assert_eq!(values, vec![3, 5, 300, -100]);
        assert_eq!(taken, 9);
        assert_eq!(reissued_length(&values), 8);
        let mixed = [PUSHW | 3, 0, 5, 2, 88, 0, 7, 3, 32, 0x21];
        let (values, taken) = leading_pushes(&mixed);
        assert_eq!(taken, 9);
        assert_eq!(reissued_length(&values), 10);
        let mut streams = Streams::default();
        assert_eq!(streams.instructions(&mixed), mixed.len());
        assert!(streams.pushes.is_empty());
        assert_eq!(streams.code, mixed);
        assert_eq!(leading_pushes(&[0x21, PUSHB, 1]), (vec![], 0));
        assert_eq!(leading_pushes(&[NPUSHB, 3, 1, 2]), (vec![], 0));
    }

    #[test]
    fn glyphs_round_trip_through_the_streams() {
        let plain = glyphs_round_trip(REGULAR);
        assert!(plain.pushes.is_empty() && plain.code.is_empty());
        let hinted = glyphs_round_trip(HINTED);
        assert!(!hinted.pushes.is_empty() && !hinted.code.is_empty());
        let records = glyph_records(HINTED);
        assert!(records.iter().any(|r| r.len() >= 2 && (r[0] & 0x80) != 0));
    }

    #[test]
    fn fonts_compress_into_three_blocks() {
        for font in [REGULAR, HINTED] {
            let mtx = compress(font).unwrap();
            assert_eq!(mtx[0], VERSION);
            let o2 = u32::from_be_bytes([0, mtx[4], mtx[5], mtx[6]]) as usize;
            let o3 = u32::from_be_bytes([0, mtx[7], mtx[8], mtx[9]]) as usize;
            let directory = lzcomp::unpack(&mtx[10..o2]);
            let pushes = lzcomp::unpack(&mtx[o2..o3]);
            let code = lzcomp::unpack(&mtx[o3..]);
            let copy_limit = u32::from_be_bytes([0, mtx[1], mtx[2], mtx[3]]) as usize;
            assert!(
                copy_limit >= lzcomp::PRELOAD + directory.len().min(0xFF_FFFF - lzcomp::PRELOAD)
            );
            let count = be16(&directory, 4).unwrap() as usize;
            let original = tables(font).unwrap();
            assert_eq!(count, original.len());
            let ctf = tables(&directory).unwrap();
            let loca = ctf.iter().find(|t| &t.tag == b"loca").unwrap();
            assert!(loca.data.is_empty());
            let glyf = ctf.iter().find(|t| &t.tag == b"glyf").unwrap();
            let streams = glyphs_round_trip(font);
            assert_eq!(glyf.data, streams.glyphs);
            assert_eq!(pushes, streams.pushes);
            assert_eq!(code, streams.code);
            for table in &ctf {
                let source = original.iter().find(|t| t.tag == table.tag).unwrap();
                assert_eq!(table.checksum, source.checksum);
                if !matches!(&table.tag, b"glyf" | b"loca" | b"cvt ") {
                    assert_eq!(
                        table.data,
                        source.data,
                        "{:?}",
                        std::str::from_utf8(&table.tag)
                    );
                }
            }
            assert!(mtx.len() < font.len());
        }
        assert!(compress(b"<svg/>").is_none());
    }
}
