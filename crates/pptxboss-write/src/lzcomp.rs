//! LZCOMP, the entropy coder of MicroType Express font data: an LZ77
//! variant whose literals, copy lengths and copy distances each go through
//! an adaptive Huffman tree, with an optional run-length pass in front.
//! The tree updates and the match search follow Monotype's reference
//! implementation, so the output is what PowerPoint itself writes.

/// Bytes preloaded in front of every block so early copies have a source.
pub(crate) const PRELOAD: usize = 2 * 32 * 96 + 4 * 256;
/// Copies from at least this far back must be three bytes or longer.
const FAR_DISTANCE: usize = 512;
/// A copy of two bytes is the shortest one.
const LENGTH_MIN: usize = 2;
/// Bits per length group and per distance group.
const LENGTH_WIDTH: u32 = 3;
const LENGTH_BITS: u32 = LENGTH_WIDTH - 1;
const DISTANCE_WIDTH: u32 = 3;
/// Hash chain entries examined per position.
const CHAIN_LIMIT: usize = 256;
/// Literal costs are summed exactly up to this many bytes.
const COST_CACHE: usize = 32;

/// The number of bits in `x`; one for zero, as the reference counts it.
fn bits_used(x: u64) -> u32 {
    if x == 0 {
        return 1;
    }
    64 - x.leading_zeros()
}

/// Writes bits most significant first.
struct BitWriter {
    bytes: Vec<u8>,
    buffer: u32,
    count: u32,
}

impl BitWriter {
    fn new() -> BitWriter {
        BitWriter {
            bytes: Vec::new(),
            buffer: 0,
            count: 0,
        }
    }

    fn bit(&mut self, bit: bool) {
        self.buffer = (self.buffer << 1) | u32::from(bit);
        self.count += 1;
        if self.count < 8 {
            return;
        }
        self.bytes.push(self.buffer as u8);
        self.buffer = 0;
        self.count = 0;
    }

    fn value(&mut self, value: u32, bits: u32) {
        for i in (0..bits).rev() {
            self.bit(value & (1 << i) != 0);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            self.bytes.push((self.buffer << (8 - self.count)) as u8);
        }
        self.bytes
    }
}

#[derive(Clone, Copy)]
struct Node {
    up: usize,
    left: usize,
    right: usize,
    /// The symbol at a leaf; None on an internal node.
    code: Option<usize>,
    weight: i64,
}

const ROOT: usize = 1;

/// An adaptive Huffman tree over `range` symbols, laid out as the
/// reference lays it out: node 1 is the root and the leaves start at
/// `range`, so encoder and decoder grow identical trees.
struct Huffman {
    tree: Vec<Node>,
    index: Vec<usize>,
}

impl Huffman {
    fn new(range: usize) -> Huffman {
        let mut tree = vec![
            Node {
                up: 0,
                left: 0,
                right: 0,
                code: None,
                weight: 1,
            };
            2 * range
        ];
        for (i, node) in tree.iter_mut().enumerate().skip(2) {
            node.up = i / 2;
        }
        for (i, node) in tree.iter_mut().enumerate().take(range).skip(1) {
            node.left = 2 * i;
            node.right = 2 * i + 1;
        }
        let mut index = vec![0usize; range];
        for (i, slot) in index.iter_mut().enumerate() {
            tree[range + i].code = Some(i);
            *slot = range + i;
        }
        let mut coder = Huffman { tree, index };
        coder.sum_weights(ROOT);
        if range > 256 && range < 512 {
            coder.update(coder.index[256]);
            coder.update(coder.index[257]);
            for _ in 0..12 {
                coder.update(coder.index[range - 3]);
            }
            for _ in 0..6 {
                coder.update(coder.index[range - 2]);
            }
            return coder;
        }
        for _ in 0..2 {
            for i in 0..range {
                coder.update(coder.index[i]);
            }
        }
        coder
    }

    fn sum_weights(&mut self, at: usize) -> i64 {
        if self.tree[at].code.is_some() {
            return self.tree[at].weight;
        }
        let (left, right) = (self.tree[at].left, self.tree[at].right);
        let weight = self.sum_weights(left) + self.sum_weights(right);
        self.tree[at].weight = weight;
        weight
    }

    fn swap(&mut self, a: usize, b: usize) {
        let (up_a, up_b) = (self.tree[a].up, self.tree[b].up);
        self.tree.swap(a, b);
        self.tree[a].up = up_a;
        self.tree[b].up = up_b;
        self.relink(a);
        self.relink(b);
    }

    fn relink(&mut self, at: usize) {
        let node = self.tree[at];
        match node.code {
            Some(code) => self.index[code] = at,
            None => {
                self.tree[node.left].up = at;
                self.tree[node.right].up = at;
            }
        }
    }

    /// Adds one to the weight of the node at `at` and of its ancestors,
    /// swapping nodes as needed to keep the tree ordered by weight.
    fn update(&mut self, mut at: usize) {
        while at != ROOT {
            let weight = self.tree[at].weight;
            let mut b = at - 1;
            if self.tree[b].weight == weight {
                while self.tree[b].weight == weight {
                    b -= 1;
                }
                b += 1;
                if b > ROOT {
                    self.swap(at, b);
                    at = b;
                }
            }
            self.tree[at].weight = weight + 1;
            at = self.tree[at].up;
        }
        self.tree[ROOT].weight += 1;
    }

    /// The cost of writing `symbol` now, in bits shifted left by sixteen.
    fn cost(&self, symbol: usize) -> i64 {
        let mut at = self.index[symbol];
        let mut depth = 0i64;
        while at != ROOT {
            depth += 1;
            at = self.tree[at].up;
        }
        depth << 16
    }

    fn write(&mut self, out: &mut BitWriter, symbol: usize) {
        let leaf = self.index[symbol];
        let mut at = leaf;
        let mut path = Vec::with_capacity(32);
        while at != ROOT {
            let up = self.tree[at].up;
            path.push(self.tree[up].right == at);
            at = up;
        }
        for bit in path.into_iter().rev() {
            out.bit(bit);
        }
        self.update(leaf);
    }

    #[cfg(test)]
    fn read(&mut self, input: &mut BitReader) -> usize {
        let mut at = ROOT;
        let code = loop {
            at = match input.bit() {
                true => self.tree[at].right,
                false => self.tree[at].left,
            };
            if let Some(code) = self.tree[at].code {
                break code;
            }
        };
        self.update(at);
        code
    }
}

/// How the symbol alphabet and distance coding scale with the block: the
/// number of three-bit distance groups needed to reach any byte of it.
struct Ranges {
    count: u32,
    distance_max: usize,
}

impl Ranges {
    fn for_length(length: usize) -> Ranges {
        let mut count = 1u32;
        let mut distance_max = 1usize << (DISTANCE_WIDTH * count);
        while distance_max < length {
            count += 1;
            distance_max = 1usize << (DISTANCE_WIDTH * count);
        }
        Ranges {
            count,
            distance_max,
        }
    }

    fn symbols(&self) -> usize {
        256 + (1usize << LENGTH_WIDTH) * self.count as usize + 3
    }

    fn dup2(&self) -> usize {
        self.symbols() - 3
    }

    fn dup4(&self) -> usize {
        self.symbols() - 2
    }

    fn dup6(&self) -> usize {
        self.symbols() - 1
    }

    /// The groups needed for one distance.
    fn groups(distance: usize) -> u32 {
        bits_used(distance as u64 - 1).div_ceil(DISTANCE_WIDTH)
    }
}

/// The groups of a copy length, most significant first, each with the
/// continuation bit set when more follow.
fn length_groups(length: usize, distance: usize) -> Vec<usize> {
    let value = match distance >= FAR_DISTANCE {
        true => length - LENGTH_MIN - 1,
        false => length - LENGTH_MIN,
    };
    let count = bits_used(value as u64).div_ceil(LENGTH_BITS);
    (0..count)
        .map(|group| {
            let shift = LENGTH_BITS * (count - 1 - group);
            let bits = (value >> shift) & ((1 << LENGTH_BITS) - 1);
            match group + 1 < count {
                true => bits | (1 << LENGTH_BITS),
                false => bits,
            }
        })
        .collect()
}

/// Run-length coding with the rarest byte as the escape: a run of four or
/// more bytes becomes escape, count, byte; a lone escape byte becomes
/// escape, zero.
fn run_length(data: &[u8]) -> Vec<u8> {
    let mut counts = [0usize; 256];
    for &byte in data {
        counts[byte as usize] += 1;
    }
    let escape = (0..256)
        .min_by_key(|&i| counts[i])
        .map(|i| i as u8)
        .unwrap_or(0);
    let mut out = vec![escape];
    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        let run = data[i..]
            .iter()
            .take(255)
            .take_while(|&&b| b == byte)
            .count();
        if run > 3 {
            out.extend_from_slice(&[escape, run as u8, byte]);
            i += run;
            continue;
        }
        match byte == escape {
            true => out.extend_from_slice(&[escape, 0]),
            false => out.push(byte),
        }
        i += 1;
    }
    out
}

struct Match {
    length: usize,
    distance: usize,
    gain: i64,
    cost_per_byte: i64,
}

struct Encoder {
    /// The preload followed by the block.
    data: Vec<u8>,
    /// Positions of earlier byte pairs by pair value, oldest first.
    chains: Vec<Vec<usize>>,
    ranges: Ranges,
    symbols: Huffman,
    lengths: Huffman,
    distances: Huffman,
    out: BitWriter,
}

impl Encoder {
    fn new(block: &[u8], run_length_coded: bool) -> Encoder {
        let mut data = Vec::with_capacity(PRELOAD + block.len());
        for k in 0..32u8 {
            for j in 0..96u8 {
                data.push(k);
                data.push(j);
            }
        }
        for j in 0..=255u8 {
            data.extend_from_slice(&[j; 4]);
        }
        data.extend_from_slice(block);
        let ranges = Ranges::for_length(block.len());
        let mut out = BitWriter::new();
        out.bit(run_length_coded);
        let mut encoder = Encoder {
            data,
            chains: vec![Vec::new(); 1 << 16],
            symbols: Huffman::new(ranges.symbols()),
            lengths: Huffman::new(1 << LENGTH_WIDTH),
            distances: Huffman::new(1 << DISTANCE_WIDTH),
            ranges,
            out,
        };
        for i in 1..PRELOAD {
            encoder.remember(i);
        }
        encoder
    }

    fn pair(&self, at: usize) -> usize {
        (self.data[at] as usize) << 8 | self.data[at + 1] as usize
    }

    /// Records the byte pair ending at `at`.
    fn remember(&mut self, at: usize) {
        let pair = self.pair(at - 1);
        self.chains[pair].push(at - 1);
    }

    fn length_cost(&self, length: usize, distance: usize, groups: u32) -> i64 {
        let parts = length_groups(length, distance);
        let first = 256 + parts[0] + (groups as usize - 1) * (1 << LENGTH_WIDTH);
        self.symbols.cost(first)
            + parts[1..]
                .iter()
                .map(|&group| self.lengths.cost(group))
                .sum::<i64>()
    }

    fn distance_cost(&self, distance: usize, groups: u32) -> i64 {
        let value = distance - 1;
        (0..groups)
            .map(|group| {
                let shift = DISTANCE_WIDTH * (groups - 1 - group);
                self.distances
                    .cost((value >> shift) & ((1 << DISTANCE_WIDTH) - 1))
            })
            .sum()
    }

    fn write_copy(&mut self, length: usize, distance: usize) {
        let groups = Ranges::groups(distance);
        let parts = length_groups(length, distance);
        let first = 256 + parts[0] + (groups as usize - 1) * (1 << LENGTH_WIDTH);
        self.symbols.write(&mut self.out, first);
        for &group in &parts[1..] {
            self.lengths.write(&mut self.out, group);
        }
        let value = distance - 1;
        for group in 0..groups {
            let shift = DISTANCE_WIDTH * (groups - 1 - group);
            self.distances.write(
                &mut self.out,
                (value >> shift) & ((1 << DISTANCE_WIDTH) - 1),
            );
        }
    }

    /// The best copy for the bytes at `index`, judged by the bits it saves
    /// over literals at the current tree state.
    fn find_match(&mut self, index: usize) -> Match {
        let end = self.data.len();
        let mut best = Match {
            length: 0,
            distance: 0,
            gain: 0,
            cost_per_byte: 0,
        };
        let mut best_cost = 0i64;
        if index + 1 >= end {
            return best;
        }
        let pair = self.pair(index);
        let mut literal_costs = vec![0i64; COST_CACHE + 1];
        let mut computed = 0usize;
        let mut keep = None;
        for (examined, &start) in self.chains[pair].iter().rev().enumerate() {
            let head_distance = index - start;
            if examined == CHAIN_LIMIT {
                keep = Some(examined);
                break;
            }
            let max_length = head_distance.min(end - index);
            if max_length < LENGTH_MIN {
                continue;
            }
            let mut length = 2;
            while length < max_length && self.data[start + length] == self.data[index + length] {
                length += 1;
            }
            let distance = head_distance - length + 1;
            if distance > self.ranges.distance_max {
                continue;
            }
            if length == 2 && distance >= FAR_DISTANCE {
                continue;
            }
            if length <= best.length && distance > best.distance {
                if length + 2 <= best.length {
                    continue;
                }
                if distance > (best.distance << DISTANCE_WIDTH) {
                    if length < best.length {
                        continue;
                    }
                    if distance > (best.distance << (DISTANCE_WIDTH + 1)) {
                        continue;
                    }
                }
            }
            let literal_cost = match length > computed {
                true => {
                    let limit = length.min(COST_CACHE);
                    for i in computed..limit {
                        literal_costs[i + 1] =
                            literal_costs[i] + self.symbols.cost(self.data[index + i] as usize);
                    }
                    computed = limit;
                    match length > COST_CACHE {
                        true => {
                            let exact = literal_costs[COST_CACHE];
                            exact + exact / COST_CACHE as i64 * (length - COST_CACHE) as i64
                        }
                        false => literal_costs[length],
                    }
                }
                false => literal_costs[length],
            };
            if literal_cost <= best.gain {
                continue;
            }
            let groups = Ranges::groups(distance);
            let mut copy_cost = self.length_cost(length, distance, groups);
            if literal_cost - copy_cost - ((groups as i64) << 16) <= best.gain {
                continue;
            }
            copy_cost += self.distance_cost(distance, groups);
            let gain = literal_cost - copy_cost;
            if gain > best.gain {
                best = Match {
                    length,
                    distance,
                    gain,
                    cost_per_byte: 0,
                };
                best_cost = copy_cost;
            }
        }
        if let Some(keep) = keep {
            let chain = &mut self.chains[pair];
            let drop = chain.len() - keep;
            chain.drain(..drop);
        }
        if best.length > 0 {
            best.cost_per_byte = best_cost / best.length as i64;
        }
        best
    }

    /// Whether to copy at `index`, and how much: the reference's look-ahead
    /// of one byte, its shortening of a copy when the following copy gains
    /// more, and its preference for a DUP symbol over a two-byte copy.
    fn copy_decision(&mut self, index: usize) -> (usize, usize) {
        let here = index;
        let mut first = self.find_match(here);
        self.remember(here);
        if first.gain <= 0 {
            return (0, 0);
        }
        let next = self.find_match(here + 1);
        let literal = self.symbols.cost(self.data[here] as usize);
        if next.gain >= first.gain
            && first.cost_per_byte
                > (next.cost_per_byte * next.length as i64 + literal) / (next.length as i64 + 1)
        {
            return (0, 0);
        }
        if first.length > 3 {
            let after = self.find_match(here + first.length);
            if after.length >= 2 {
                let shorter = self.find_match(here + first.length - 1);
                if shorter.length > after.length && shorter.cost_per_byte < after.cost_per_byte {
                    let distance = first.distance + 1;
                    let groups = Ranges::groups(distance);
                    let cut_cost = self.length_cost(first.length - 1, distance, groups)
                        + self.distance_cost(distance, groups)
                        + shorter.cost_per_byte * shorter.length as i64;
                    let full_cost = first.cost_per_byte * first.length as i64
                        + after.cost_per_byte * after.length as i64;
                    if full_cost / (first.length + after.length) as i64
                        > cut_cost / (first.length - 1 + shorter.length) as i64
                    {
                        first.length -= 1;
                        first.distance += 1;
                    }
                }
            }
        }
        if first.length == 2 {
            let dup2 = self.symbols.cost(self.ranges.dup2());
            if self.data[here] == self.data[here - 2] {
                let second = self.symbols.cost(self.data[here + 1] as usize);
                if first.cost_per_byte * 2 > dup2 + second {
                    return (0, 0);
                }
            } else if here + 1 < self.data.len()
                && self.data[here + 1] == self.data[here - 1]
                && first.cost_per_byte * 2 > literal + dup2
            {
                return (0, 0);
            }
        }
        (first.length, first.distance)
    }

    fn encode(mut self, block_length: usize) -> Vec<u8> {
        self.out.value(block_length as u32, 24);
        let end = self.data.len();
        let mut i = PRELOAD;
        while i < end {
            let here = i;
            let (length, distance) = self.copy_decision(i);
            i += 1;
            if length > 0 {
                self.write_copy(length, distance);
                for _ in 1..length {
                    self.remember(i);
                    i += 1;
                }
                continue;
            }
            let byte = self.data[here];
            let symbol = match () {
                _ if byte == self.data[here - 2] => self.ranges.dup2(),
                _ if byte == self.data[here - 4] => self.ranges.dup4(),
                _ if byte == self.data[here - 6] => self.ranges.dup6(),
                _ => byte as usize,
            };
            self.symbols.write(&mut self.out, symbol);
        }
        self.out.finish()
    }
}

/// One block compressed, headed by the run-length flag and the 24-bit
/// length the decoder reads. Blocks of 16 MiB or more cannot be coded.
pub(crate) fn pack(block: &[u8]) -> Option<Vec<u8>> {
    if block.len() >= 1 << 24 {
        return None;
    }
    let packed = run_length(block);
    match packed.len() < block.len() * 3 / 4 {
        true => Some(Encoder::new(&packed, true).encode(packed.len())),
        false => Some(Encoder::new(block, false).encode(block.len())),
    }
}

/// The length a decoder will produce for `packed`, read from its header.
pub(crate) fn block_length(packed: &[u8]) -> usize {
    let bytes: [u8; 4] = packed
        .get(0..4)
        .and_then(|b| b.try_into().ok())
        .unwrap_or([0; 4]);
    ((u32::from_be_bytes(bytes) >> 7) & 0xFF_FFFF) as usize
}

#[cfg(test)]
struct BitReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

#[cfg(test)]
impl BitReader<'_> {
    fn bit(&mut self) -> bool {
        let byte = self.bytes[self.at / 8];
        let bit = byte & (0x80 >> (self.at % 8)) != 0;
        self.at += 1;
        bit
    }

    fn value(&mut self, bits: u32) -> u32 {
        (0..bits).fold(0, |acc, _| (acc << 1) | u32::from(self.bit()))
    }
}

/// The block back from its compressed form, as the reference decoder
/// reads it.
#[cfg(test)]
pub(crate) fn unpack(packed: &[u8]) -> Vec<u8> {
    let mut input = BitReader {
        bytes: packed,
        at: 0,
    };
    let run_length_coded = input.bit();
    let mut distances = Huffman::new(1 << DISTANCE_WIDTH);
    let mut lengths = Huffman::new(1 << LENGTH_WIDTH);
    let length = input.value(24) as usize;
    let ranges = Ranges::for_length(length);
    let mut symbols = Huffman::new(ranges.symbols());
    let mut data = Encoder::new(&[], false).data;
    while data.len() < PRELOAD + length {
        let symbol = symbols.read(&mut input);
        let here = data.len();
        if symbol < 256 {
            data.push(symbol as u8);
            continue;
        }
        if symbol == ranges.dup2() {
            data.push(data[here - 2]);
            continue;
        }
        if symbol == ranges.dup4() {
            data.push(data[here - 4]);
            continue;
        }
        if symbol == ranges.dup6() {
            data.push(data[here - 6]);
            continue;
        }
        let bits = symbol - 256;
        let groups = (bits / (1 << LENGTH_WIDTH)) as u32 + 1;
        let mut group = bits % (1 << LENGTH_WIDTH);
        let mut value = 0usize;
        loop {
            let more = group & (1 << LENGTH_BITS) != 0;
            value = (value << LENGTH_BITS) | (group & ((1 << LENGTH_BITS) - 1));
            if !more {
                break;
            }
            group = lengths.read(&mut input);
        }
        let mut copy = value + LENGTH_MIN;
        let distance = (0..groups).fold(0usize, |acc, _| {
            (acc << DISTANCE_WIDTH) | distances.read(&mut input)
        }) + 1;
        if distance >= FAR_DISTANCE {
            copy += 1;
        }
        let start = here - distance - copy + 1;
        for j in 0..copy {
            data.push(data[start + j]);
        }
    }
    let block = &data[PRELOAD..];
    if !run_length_coded {
        return block.to_vec();
    }
    let escape = block[0];
    let mut out = Vec::new();
    let mut i = 1;
    while i < block.len() {
        if block[i] != escape {
            out.push(block[i]);
            i += 1;
            continue;
        }
        let count = block[i + 1];
        if count == 0 {
            out.push(escape);
            i += 2;
            continue;
        }
        out.extend(std::iter::repeat_n(block[i + 2], count as usize));
        i += 3;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_counts_and_length_groups_follow_the_reference() {
        assert_eq!(bits_used(0), 1);
        assert_eq!(bits_used(1), 1);
        assert_eq!(bits_used(2), 2);
        assert_eq!(bits_used(255), 8);
        assert_eq!(Ranges::groups(1), 1);
        assert_eq!(Ranges::groups(8), 1);
        assert_eq!(Ranges::groups(9), 2);
        assert_eq!(Ranges::groups(64), 2);
        assert_eq!(Ranges::groups(65), 3);
        assert_eq!(Ranges::for_length(8).count, 1);
        assert_eq!(Ranges::for_length(9).count, 2);
        assert_eq!(Ranges::for_length(9).symbols(), 256 + 16 + 3);
        assert_eq!(length_groups(2, 1), vec![0]);
        assert_eq!(length_groups(3, 512), vec![0]);
        assert_eq!(length_groups(5, 1), vec![3]);
        assert_eq!(length_groups(6, 1), vec![4 | 1, 0]);
        assert_eq!(length_groups(2 + 0b10110, 1), vec![4 | 1, 4 | 1, 2]);
    }

    #[test]
    fn run_length_uses_the_rarest_byte_as_escape() {
        let coded = run_length(&[7, 7, 7, 7, 7, 1, 2, 0, 3]);
        assert_eq!(coded, vec![4, 4, 5, 7, 1, 2, 0, 3]);
        assert_eq!(run_length(&[4, 4]), vec![0, 4, 4]);
        assert_eq!(run_length(&[]), vec![0]);
    }

    fn round_trip(block: &[u8]) -> Vec<u8> {
        let packed = pack(block).unwrap();
        assert_eq!(unpack(&packed), block);
        packed
    }

    #[test]
    fn blocks_survive_a_round_trip() {
        round_trip(&[]);
        round_trip(b"a");
        round_trip(b"abcabcabcabcabcabc");
        round_trip(&[0u8; 5000]);
        let mut noise = Vec::new();
        let mut state = 0x2545_F491u32;
        for _ in 0..20_000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            noise.push((state >> 3) as u8);
        }
        round_trip(&noise);
        let mut text = Vec::new();
        for i in 0..3000u32 {
            text.extend_from_slice(
                format!("glyph {} advance {}\n", i % 97, i * 7 % 1013).as_bytes(),
            );
        }
        let packed = round_trip(&text);
        assert!(packed.len() < text.len() / 4);
        let mut sparse = vec![0u8; 40_000];
        for i in (0..sparse.len()).step_by(700) {
            sparse[i] = (i / 700) as u8;
        }
        round_trip(&sparse);
    }

    #[test]
    fn block_lengths_read_back() {
        assert_eq!(block_length(&pack(b"abc").unwrap()), 3);
        assert_eq!(block_length(&pack(&[]).unwrap()), 0);
        let coded = pack(&[9u8; 1000]).unwrap();
        assert_eq!(block_length(&coded), run_length(&[9u8; 1000]).len());
    }

    #[test]
    fn oversized_blocks_are_refused() {
        assert!(pack(&vec![0u8; 1 << 24]).is_none());
    }
}
