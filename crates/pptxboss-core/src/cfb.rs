//! Compound File Binary containers (MS-CFB): the OLE storage behind legacy
//! `.ppt` files and encrypted packages. The reader materializes the FAT,
//! directory and mini stream once and serves streams by path, tolerating
//! the header and chain defects the specification tells readers to expect.
//! The writer produces a minimal version-3 file for fixtures and encrypted
//! output.

use std::sync::Arc;

use crate::error::{Error, Result};
use crate::zip::Source;

pub const SIGNATURE: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
const MAXREGSECT: u32 = 0xffff_fffa;
const FATSECT: u32 = 0xffff_fffd;
const ENDOFCHAIN: u32 = 0xffff_fffe;
const FREESECT: u32 = 0xffff_ffff;
const NOSTREAM: u32 = 0xffff_ffff;
const HEADER_LEN: usize = 512;
const HEADER_DIFAT_ENTRIES: usize = 109;
const DIRECTORY_ENTRY_LEN: usize = 128;
const MINI_SECTOR_LEN: usize = 64;
const DEFAULT_MINI_CUTOFF: u64 = 4096;
/// Iteration cap for every chain walk, well above any real file's sector count.
const MAX_CHAIN: usize = 1 << 24;

/// Whether `bytes` start with the compound file signature.
pub fn is_compound(bytes: &[u8]) -> bool {
    bytes.starts_with(&SIGNATURE)
}

/// One directory entry: a storage, a stream, or the root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Storage,
    Stream,
    Root,
    Unallocated,
}

/// A parsed compound file.
pub struct Compound {
    data: Vec<u8>,
    sector_len: usize,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    mini_stream: Vec<u8>,
    mini_cutoff: u64,
    entries: Vec<Entry>,
}

impl Compound {
    /// Reads the whole source and parses its container structures.
    pub fn open(source: &dyn Source) -> Result<Self> {
        let len =
            usize::try_from(source.len()).map_err(|_| Error::Other("file too large".into()))?;
        let mut data = vec![0u8; len];
        source.read_at(0, &mut data)?;
        Self::from_bytes(data)
    }

    pub fn open_source(source: Arc<dyn Source>) -> Result<Self> {
        Self::open(source.as_ref())
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        if data.len() < HEADER_LEN || !is_compound(&data) {
            return Err(cfb("missing compound file signature"));
        }
        let major = u16_at(&data, 0x1a);
        let sector_shift = u16_at(&data, 0x1e);
        if !matches!(major, 3 | 4) {
            return Err(cfb(&format!("unsupported major version {major}")));
        }
        if !matches!(sector_shift, 9 | 12) {
            return Err(cfb(&format!("unsupported sector shift {sector_shift}")));
        }
        let sector_len = 1usize << sector_shift;
        if data.len() < sector_len * 3 {
            return Err(cfb("file shorter than three sectors"));
        }
        let fat_sector_count = u32_at(&data, 0x2c) as usize;
        let first_directory = u32_at(&data, 0x30);
        let mut mini_cutoff = u64::from(u32_at(&data, 0x38));
        if mini_cutoff == 0 {
            mini_cutoff = DEFAULT_MINI_CUTOFF;
        }
        let first_mini_fat = u32_at(&data, 0x3c);
        let mini_fat_count = u32_at(&data, 0x40) as usize;
        let first_difat = u32_at(&data, 0x44);
        let difat_count = u32_at(&data, 0x48) as usize;

        let mut difat: Vec<u32> = (0..HEADER_DIFAT_ENTRIES)
            .map(|i| u32_at(&data, 0x4c + i * 4))
            .take_while(|&sector| sector <= MAXREGSECT)
            .collect();
        let mut next = first_difat;
        let mut visited = 0usize;
        while next <= MAXREGSECT && visited < difat_count.max(1) && visited < MAX_CHAIN {
            let Some(sector) = sector_bytes(&data, sector_len, next) else {
                break;
            };
            let entries = sector_len / 4 - 1;
            difat.extend(
                (0..entries)
                    .map(|i| u32_at(sector, i * 4))
                    .take_while(|&s| s <= MAXREGSECT),
            );
            next = u32_at(sector, entries * 4);
            visited += 1;
        }
        if fat_sector_count > 0 {
            difat.truncate(fat_sector_count);
        }

        let mut fat = Vec::with_capacity(difat.len() * (sector_len / 4));
        for &sector in &difat {
            let Some(bytes) = sector_bytes(&data, sector_len, sector) else {
                break;
            };
            fat.extend((0..sector_len / 4).map(|i| u32_at(bytes, i * 4)));
        }

        let directory = read_chain(&data, sector_len, &fat, first_directory, u64::MAX);
        let mut entries = Vec::with_capacity(directory.len() / DIRECTORY_ENTRY_LEN);
        for raw in directory.as_chunks::<DIRECTORY_ENTRY_LEN>().0 {
            entries.push(parse_entry(raw, major));
        }
        if entries.is_empty() {
            return Err(cfb("empty directory"));
        }

        let root = &entries[0];
        let mini_stream = match root.size {
            0 => Vec::new(),
            size => read_chain(&data, sector_len, &fat, root.start, size),
        };
        let mini_fat_bytes = match mini_fat_count {
            0 => Vec::new(),
            _ => read_chain(&data, sector_len, &fat, first_mini_fat, u64::MAX),
        };
        let mini_fat = mini_fat_bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect();

        Ok(Self {
            data,
            sector_len,
            fat,
            mini_fat,
            mini_stream,
            mini_cutoff,
            entries,
        })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Full paths (`storage/stream`) of every stream, in directory order.
    pub fn stream_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        self.collect_paths(0, "", &mut paths, &mut vec![false; self.entries.len()]);
        paths
    }

    fn collect_paths(&self, id: u32, prefix: &str, out: &mut Vec<String>, seen: &mut Vec<bool>) {
        let Some(entry) = self.entries.get(id as usize) else {
            return;
        };
        let child = entry.child;
        let mut stack = vec![child];
        while let Some(current) = stack.pop() {
            if current == NOSTREAM
                || current as usize >= self.entries.len()
                || seen[current as usize]
            {
                continue;
            }
            seen[current as usize] = true;
            let node = &self.entries[current as usize];
            let path = match prefix.is_empty() {
                true => node.name.clone(),
                false => format!("{prefix}/{}", node.name),
            };
            match node.kind {
                EntryKind::Stream => out.push(path),
                EntryKind::Storage => self.collect_paths(current, &path, out, seen),
                _ => {}
            }
            stack.push(node.left);
            stack.push(node.right);
        }
    }

    /// The entry at `path` (components separated by `/`), matched ASCII
    /// case-insensitively by a bounded search of each storage's tree.
    pub fn entry(&self, path: &str) -> Option<&Entry> {
        let mut current = self.entries.first()?;
        for component in path.split('/').filter(|part| !part.is_empty()) {
            current = self.find_child(current.child, component)?;
        }
        Some(current)
    }

    fn find_child(&self, root: u32, name: &str) -> Option<&Entry> {
        let mut stack = vec![root];
        let mut seen = vec![false; self.entries.len()];
        while let Some(id) = stack.pop() {
            if id == NOSTREAM || id as usize >= self.entries.len() || seen[id as usize] {
                continue;
            }
            seen[id as usize] = true;
            let entry = &self.entries[id as usize];
            if entry.kind != EntryKind::Unallocated && entry.name.eq_ignore_ascii_case(name) {
                return Some(entry);
            }
            stack.push(entry.left);
            stack.push(entry.right);
        }
        None
    }

    pub fn has_stream(&self, path: &str) -> bool {
        self.entry(path)
            .is_some_and(|entry| entry.kind == EntryKind::Stream)
    }

    /// The bytes of the stream at `path`, or None when it does not exist.
    pub fn stream(&self, path: &str) -> Option<Vec<u8>> {
        let entry = self.entry(path)?;
        if entry.kind != EntryKind::Stream {
            return None;
        }
        Some(self.read_entry(entry))
    }

    fn read_entry(&self, entry: &Entry) -> Vec<u8> {
        if entry.size == 0 {
            return Vec::new();
        }
        if entry.size < self.mini_cutoff {
            let mut out = Vec::with_capacity(entry.size as usize);
            let mut sector = entry.start;
            let mut steps = 0usize;
            while sector <= MAXREGSECT && (out.len() as u64) < entry.size && steps < MAX_CHAIN {
                let start = sector as usize * MINI_SECTOR_LEN;
                let Some(bytes) = self.mini_stream.get(start..) else {
                    break;
                };
                let take = bytes.len().min(MINI_SECTOR_LEN);
                out.extend_from_slice(&bytes[..take]);
                sector = self
                    .mini_fat
                    .get(sector as usize)
                    .copied()
                    .unwrap_or(ENDOFCHAIN);
                steps += 1;
            }
            out.truncate(entry.size as usize);
            return out;
        }
        read_chain(
            &self.data,
            self.sector_len,
            &self.fat,
            entry.start,
            entry.size,
        )
    }
}

fn cfb(msg: &str) -> Error {
    Error::Other(format!("compound file: {msg}"))
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// The bytes of regular sector `sector`, or None past the end of the file.
fn sector_bytes(data: &[u8], sector_len: usize, sector: u32) -> Option<&[u8]> {
    let start = (sector as usize + 1).checked_mul(sector_len)?;
    let end = start.checked_add(sector_len)?;
    match end <= data.len() {
        true => Some(&data[start..end]),
        false => data.get(start..).filter(|rest| !rest.is_empty()),
    }
}

/// Concatenates the FAT chain from `start`, up to `size` bytes (or the whole chain).
fn read_chain(data: &[u8], sector_len: usize, fat: &[u32], start: u32, size: u64) -> Vec<u8> {
    let mut out = Vec::new();
    let mut sector = start;
    let mut steps = 0usize;
    while sector <= MAXREGSECT && (out.len() as u64) < size && steps < MAX_CHAIN {
        let Some(bytes) = sector_bytes(data, sector_len, sector) else {
            break;
        };
        out.extend_from_slice(bytes);
        sector = fat.get(sector as usize).copied().unwrap_or(ENDOFCHAIN);
        steps += 1;
    }
    if size != u64::MAX {
        out.truncate(size as usize);
    }
    out
}

fn parse_entry(raw: &[u8], major: u16) -> Entry {
    let declared = usize::from(u16_at(raw, 0x40)).min(64);
    let name_bytes = &raw[..declared];
    let units: Vec<u16> = name_bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&unit| unit != 0)
        .collect();
    let name = String::from_utf16_lossy(&units);
    let kind = match raw[0x42] {
        1 => EntryKind::Storage,
        2 => EntryKind::Stream,
        5 => EntryKind::Root,
        _ => EntryKind::Unallocated,
    };
    let mut size = u64::from(u32_at(raw, 0x78)) | (u64::from(u32_at(raw, 0x7c)) << 32);
    if major == 3 {
        size &= 0xffff_ffff;
    }
    Entry {
        name,
        kind,
        left: u32_at(raw, 0x44),
        right: u32_at(raw, 0x48),
        child: u32_at(raw, 0x4c),
        start: u32_at(raw, 0x74),
        size,
    }
}

/// Builds a minimal version-3 compound file: root storage with streams,
/// small streams in the mini stream, every directory node black.
#[derive(Default)]
pub struct Writer {
    streams: Vec<(String, Vec<u8>)>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a stream at the root storage (nested storages are not written).
    pub fn stream(mut self, name: &str, data: &[u8]) -> Self {
        self.streams.push((name.to_string(), data.to_vec()));
        self
    }

    pub fn build(&self) -> Vec<u8> {
        const SECTOR: usize = 512;
        let mut mini_stream: Vec<u8> = Vec::new();
        let mut mini_fat: Vec<u32> = Vec::new();
        // Regular streams are laid out after: FAT sectors, directory sectors, mini FAT sectors, mini stream.
        let mut entries: Vec<(String, u32, u64, bool)> = Vec::new(); // name, start, size, in mini stream
        let mut regular: Vec<Vec<u8>> = Vec::new();
        for (name, data) in &self.streams {
            if (data.len() as u64) < DEFAULT_MINI_CUTOFF && !data.is_empty() {
                let first = (mini_stream.len() / MINI_SECTOR_LEN) as u32;
                let sectors = data.len().div_ceil(MINI_SECTOR_LEN);
                mini_stream.extend_from_slice(data);
                mini_stream.resize(
                    mini_stream.len().div_ceil(MINI_SECTOR_LEN) * MINI_SECTOR_LEN,
                    0,
                );
                for i in 0..sectors {
                    mini_fat.push(match i + 1 == sectors {
                        true => ENDOFCHAIN,
                        false => first + i as u32 + 1,
                    });
                }
                entries.push((name.clone(), first, data.len() as u64, true));
                continue;
            }
            entries.push((name.clone(), 0, data.len() as u64, false));
            regular.push(data.clone());
        }
        let directory_entries = 1 + entries.len();
        let directory_sectors = directory_entries
            .div_ceil(SECTOR / DIRECTORY_ENTRY_LEN)
            .max(1);
        let mini_fat_sectors = match mini_fat.is_empty() {
            true => 0,
            false => (mini_fat.len() * 4).div_ceil(SECTOR),
        };
        let mini_stream_sectors = mini_stream.len().div_ceil(SECTOR);
        let regular_sectors: usize = regular.iter().map(|d| d.len().div_ceil(SECTOR)).sum();
        let mut fat_sectors = 1usize;
        loop {
            let total = fat_sectors
                + directory_sectors
                + mini_fat_sectors
                + mini_stream_sectors
                + regular_sectors;
            if total <= fat_sectors * (SECTOR / 4) {
                break;
            }
            fat_sectors += 1;
        }
        let total_sectors = fat_sectors
            + directory_sectors
            + mini_fat_sectors
            + mini_stream_sectors
            + regular_sectors;
        let mut fat: Vec<u32> = vec![FREESECT; fat_sectors * (SECTOR / 4)];
        let mut next_sector = 0u32;
        let mut allocate = |count: usize, fat: &mut Vec<u32>| -> u32 {
            let start = next_sector;
            for i in 0..count {
                let s = start as usize + i;
                fat[s] = match i + 1 == count {
                    true => ENDOFCHAIN,
                    false => start + i as u32 + 1,
                };
            }
            next_sector += count as u32;
            start
        };
        let fat_start = allocate(fat_sectors, &mut fat);
        for i in 0..fat_sectors {
            fat[fat_start as usize + i] = FATSECT;
        }
        let directory_start = allocate(directory_sectors, &mut fat);
        let mini_fat_start = match mini_fat_sectors {
            0 => ENDOFCHAIN,
            n => allocate(n, &mut fat),
        };
        let mini_stream_start = match mini_stream_sectors {
            0 => ENDOFCHAIN,
            n => allocate(n, &mut fat),
        };
        let mut regular_iter = regular.iter();
        for entry in entries.iter_mut() {
            if entry.3 {
                continue;
            }
            let data = regular_iter.next().expect("one regular stream per entry");
            entry.1 = match data.is_empty() {
                true => ENDOFCHAIN,
                false => allocate(data.len().div_ceil(SECTOR), &mut fat),
            };
        }

        let mut out = Vec::with_capacity((total_sectors + 1) * SECTOR);
        out.extend_from_slice(&SIGNATURE);
        out.extend_from_slice(&[0u8; 16]);
        out.extend_from_slice(&0x003eu16.to_le_bytes());
        out.extend_from_slice(&3u16.to_le_bytes());
        out.extend_from_slice(&0xfffeu16.to_le_bytes());
        out.extend_from_slice(&9u16.to_le_bytes());
        out.extend_from_slice(&6u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 6]);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(fat_sectors as u32).to_le_bytes());
        out.extend_from_slice(&directory_start.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(DEFAULT_MINI_CUTOFF as u32).to_le_bytes());
        out.extend_from_slice(&mini_fat_start.to_le_bytes());
        out.extend_from_slice(&(mini_fat_sectors as u32).to_le_bytes());
        out.extend_from_slice(&ENDOFCHAIN.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        for i in 0..HEADER_DIFAT_ENTRIES {
            let value = match i < fat_sectors {
                true => fat_start + i as u32,
                false => FREESECT,
            };
            out.extend_from_slice(&value.to_le_bytes());
        }
        debug_assert_eq!(out.len(), HEADER_LEN);
        for value in &fat {
            out.extend_from_slice(&value.to_le_bytes());
        }

        let order = sorted_order(&entries);
        let tree = balanced_tree(&order);
        let mut directory = Vec::with_capacity(directory_sectors * SECTOR);
        let root_child = tree.root.map_or(NOSTREAM, |id| id as u32 + 1);
        directory.extend(directory_entry(
            "Root Entry",
            5,
            NOSTREAM,
            NOSTREAM,
            root_child,
            match mini_stream.is_empty() {
                true => ENDOFCHAIN,
                false => mini_stream_start,
            },
            mini_stream.len() as u64,
        ));
        for (index, (name, start, size, _)) in entries.iter().enumerate() {
            let (left, right) = tree.links[index];
            directory.extend(directory_entry(
                name,
                2,
                left.map_or(NOSTREAM, |id| id as u32 + 1),
                right.map_or(NOSTREAM, |id| id as u32 + 1),
                NOSTREAM,
                *start,
                *size,
            ));
        }
        while directory.len() < directory_sectors * SECTOR {
            directory.extend(directory_entry("", 0, NOSTREAM, NOSTREAM, NOSTREAM, 0, 0));
        }
        out.extend_from_slice(&directory);
        if mini_fat_sectors > 0 {
            let mut bytes = Vec::with_capacity(mini_fat_sectors * SECTOR);
            for value in &mini_fat {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
            bytes.resize(mini_fat_sectors * SECTOR, 0xff);
            out.extend_from_slice(&bytes);
        }
        if mini_stream_sectors > 0 {
            let mut bytes = mini_stream.clone();
            bytes.resize(mini_stream_sectors * SECTOR, 0);
            out.extend_from_slice(&bytes);
        }
        for data in &regular {
            let mut bytes = data.clone();
            bytes.resize(data.len().div_ceil(SECTOR) * SECTOR, 0);
            out.extend_from_slice(&bytes);
        }
        out
    }
}

/// Entry indexes ordered as the directory tree requires: shorter names
/// first, then by upper-cased UTF-16 code units.
fn sorted_order(entries: &[(String, u32, u64, bool)]) -> Vec<usize> {
    let key = |name: &str| -> (usize, Vec<u16>) {
        let units: Vec<u16> = name
            .encode_utf16()
            .map(|unit| match char::from_u32(u32::from(unit)) {
                Some(ch) if ch.is_ascii_lowercase() => unit - 32,
                _ => unit,
            })
            .collect();
        ((units.len() + 1) * 2, units)
    };
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by_key(|&i| key(&entries[i].0));
    order
}

struct Tree {
    root: Option<usize>,
    /// `(left, right)` per entry index.
    links: Vec<(Option<usize>, Option<usize>)>,
}

fn balanced_tree(order: &[usize]) -> Tree {
    let mut links = vec![(None, None); order.len()];
    let root = build_subtree(order, &mut links);
    Tree { root, links }
}

fn build_subtree(order: &[usize], links: &mut [(Option<usize>, Option<usize>)]) -> Option<usize> {
    if order.is_empty() {
        return None;
    }
    let middle = order.len() / 2;
    let node = order[middle];
    let left = build_subtree(&order[..middle], links);
    let right = build_subtree(&order[middle + 1..], links);
    links[node] = (left, right);
    Some(node)
}

fn directory_entry(
    name: &str,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
) -> Vec<u8> {
    let mut entry = vec![0u8; DIRECTORY_ENTRY_LEN];
    let units: Vec<u16> = name.encode_utf16().take(31).collect();
    for (i, unit) in units.iter().enumerate() {
        entry[i * 2..i * 2 + 2].copy_from_slice(&unit.to_le_bytes());
    }
    let name_len = match name.is_empty() {
        true => 0u16,
        false => (units.len() as u16 + 1) * 2,
    };
    entry[0x40..0x42].copy_from_slice(&name_len.to_le_bytes());
    entry[0x42] = kind;
    entry[0x43] = 1;
    entry[0x44..0x48].copy_from_slice(&left.to_le_bytes());
    entry[0x48..0x4c].copy_from_slice(&right.to_le_bytes());
    entry[0x4c..0x50].copy_from_slice(&child.to_le_bytes());
    entry[0x74..0x78].copy_from_slice(&start.to_le_bytes());
    entry[0x78..0x80].copy_from_slice(&size.to_le_bytes());
    entry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_files_read_back_with_mini_and_regular_streams() {
        let big: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let bytes = Writer::new()
            .stream("Current User", b"tiny")
            .stream("PowerPoint Document", &big)
            .stream("Pictures", &[7u8; 100])
            .stream("\u{5}SummaryInformation", b"")
            .build();
        assert!(is_compound(&bytes));
        assert_eq!(bytes.len() % 512, 0);
        let compound = Compound::from_bytes(bytes).unwrap();
        assert_eq!(compound.stream("Current User").unwrap(), b"tiny");
        assert_eq!(compound.stream("powerpoint document").unwrap(), big);
        assert_eq!(compound.stream("Pictures").unwrap(), vec![7u8; 100]);
        assert_eq!(compound.stream("\u{5}SummaryInformation").unwrap(), b"");
        assert!(compound.stream("Missing").is_none());
        let mut paths = compound.stream_paths();
        paths.sort();
        assert_eq!(
            paths,
            [
                "\u{5}SummaryInformation",
                "Current User",
                "Pictures",
                "PowerPoint Document"
            ]
        );
    }

    #[test]
    fn many_small_streams_span_several_directory_sectors() {
        let mut writer = Writer::new();
        for i in 0..40 {
            writer = writer.stream(&format!("s{i}"), format!("payload {i}").as_bytes());
        }
        let compound = Compound::from_bytes(writer.build()).unwrap();
        for i in 0..40 {
            assert_eq!(
                compound.stream(&format!("s{i}")).unwrap(),
                format!("payload {i}").as_bytes()
            );
        }
        assert_eq!(compound.stream_paths().len(), 40);
    }

    #[test]
    fn rejects_non_compound_and_truncated_input() {
        assert!(Compound::from_bytes(b"PK\x03\x04".to_vec()).is_err());
        let mut header = vec![0u8; 600];
        header[..8].copy_from_slice(&SIGNATURE);
        assert!(Compound::from_bytes(header).is_err());
        let bytes = Writer::new().stream("a", b"x").build();
        let cut = bytes[..bytes.len() - 700].to_vec();
        let compound = Compound::from_bytes(cut);
        if let Ok(compound) = compound {
            let _ = compound.stream("a");
        }
    }
}
