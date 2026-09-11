//! ZIP container reader for OPC packages (ECMA-376 Part 2, Annex B; PKWARE
//! APPNOTE 6.3).
//!
//! The central directory is parsed once at open. Entry data is then read
//! on demand through a [`Source`] with positioned reads, so opening a
//! 40 MB deck to extract its text costs the central directory plus the XML
//! parts actually touched, never the embedded media.
//!
//! Reading is lenient where the archive is still unambiguous: junk before
//! the first local header is detected and compensated, data descriptors
//! with or without their signature are accepted, Zip64 records are
//! followed, and a truncated central directory yields the entries that
//! were readable. What was tolerated is recorded in [`Layout`] so a
//! verifier can report it.

use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::Arc;

use memchr::memmem;

use crate::crc32::crc32;
use crate::error::{Error, Result};
use crate::hash::FastMap;

const LOCAL_HEADER_SIG: [u8; 4] = 0x0403_4b50u32.to_le_bytes();
const CENTRAL_HEADER_SIG: [u8; 4] = 0x0201_4b50u32.to_le_bytes();
const END_RECORD_SIG: [u8; 4] = 0x0605_4b50u32.to_le_bytes();
const ZIP64_END_RECORD_SIG: [u8; 4] = 0x0606_4b50u32.to_le_bytes();
const ZIP64_LOCATOR_SIG: [u8; 4] = 0x0706_4b50u32.to_le_bytes();

const END_RECORD_LEN: usize = 22;
const ZIP64_LOCATOR_LEN: usize = 20;
const ZIP64_END_RECORD_LEN: usize = 56;
const CENTRAL_HEADER_LEN: usize = 46;
const LOCAL_HEADER_LEN: usize = 30;
/// Files up to this size are read whole in one call and kept.
const WHOLE_FILE_LEN: u64 = 256 * 1024;
/// For larger files, the bytes read from the end in one call and kept: the
/// end records (a comment can be 65535 bytes long) and the central directory.
const TAIL_LEN: u64 = 66 * 1024;
const ZIP64_EXTRA_ID: u16 = 0x0001;

/// Compression method 0: the data is stored as-is.
pub const METHOD_STORED: u16 = 0;
/// Compression method 8: raw DEFLATE.
pub const METHOD_DEFLATE: u16 = 8;

/// General purpose bit 0: the entry is encrypted (Annex B forbids it).
pub const FLAG_ENCRYPTED: u16 = 1;
/// General purpose bit 3: CRC and sizes follow the data in a descriptor.
pub const FLAG_DATA_DESCRIPTOR: u16 = 1 << 3;
/// General purpose bit 11: the file name is UTF-8.
pub const FLAG_UTF8_NAMES: u16 = 1 << 11;

/// Random access to the bytes of an archive.
pub trait Source: Send + Sync {
    /// Total length in bytes.
    fn len(&self) -> u64;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Fills `buf` from `offset`; fails if the range is out of bounds.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
    /// The whole content when it already sits in memory, so reads can borrow it.
    fn as_bytes(&self) -> Option<&[u8]> {
        None
    }
}

impl Source for Vec<u8> {
    fn len(&self) -> u64 {
        Vec::len(self) as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let start = usize::try_from(offset).map_err(|_| out_of_bounds())?;
        let end = start.checked_add(buf.len()).ok_or_else(out_of_bounds)?;
        let slice = self.get(start..end).ok_or_else(out_of_bounds)?;
        buf.copy_from_slice(slice);
        Ok(())
    }

    fn as_bytes(&self) -> Option<&[u8]> {
        Some(self)
    }
}

/// A file read with positioned reads; no seeking, so shared across threads.
pub struct FileSource {
    file: File,
    len: u64,
}

impl FileSource {
    pub fn new(file: File) -> io::Result<Self> {
        let len = file.metadata()?.len();
        Ok(Self { file, len })
    }

    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::new(File::open(path)?)
    }
}

impl Source for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    #[cfg(unix)]
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        use std::os::unix::fs::FileExt;
        self.file.read_exact_at(buf, offset)
    }

    #[cfg(windows)]
    fn read_at(&self, offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        let mut offset = offset;
        while !buf.is_empty() {
            let read = self.file.seek_read(buf, offset)?;
            if read == 0 {
                return Err(out_of_bounds());
            }
            buf = &mut buf[read..];
            offset += read as u64;
        }
        Ok(())
    }
}

fn out_of_bounds() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "read past end of archive")
}

/// One item of the central directory, exactly as recorded.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The item name decoded as UTF-8, with invalid sequences replaced.
    pub name: String,
    /// The item name bytes as written.
    pub raw_name: Vec<u8>,
    pub method: u16,
    pub flags: u16,
    pub version_needed: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    /// Local header offset as recorded (before any shift the archive needed).
    pub header_offset: u64,
    /// Offset of this entry's central directory header.
    pub central_offset: u64,
    /// Header ids of the extra fields present in the central header.
    pub extra_ids: Vec<u16>,
    /// True when the central header's comment length was nonzero.
    pub has_comment: bool,
}

impl Entry {
    pub fn is_directory(&self) -> bool {
        self.raw_name.last() == Some(&b'/')
    }

    pub fn is_encrypted(&self) -> bool {
        self.flags & FLAG_ENCRYPTED != 0
    }

    pub fn has_data_descriptor(&self) -> bool {
        self.flags & FLAG_DATA_DESCRIPTOR != 0
    }

    pub fn names_are_utf8(&self) -> bool {
        self.flags & FLAG_UTF8_NAMES != 0
    }

    pub fn has_zip64_extra(&self) -> bool {
        self.extra_ids.contains(&ZIP64_EXTRA_ID)
    }
}

/// The fixed fields of an entry's local file header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalHeader {
    pub version_needed: u16,
    pub flags: u16,
    pub method: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub raw_name: Vec<u8>,
    pub extra_len: u16,
    /// Absolute offset of the first data byte.
    pub data_offset: u64,
}

/// What the reader found and tolerated about the archive's structure.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// Total length of the source in bytes.
    pub len: u64,
    /// Absolute offset of the end of central directory record.
    pub end_record_offset: u64,
    /// Whether Zip64 end records were present and used.
    pub zip64: bool,
    /// Bytes to add to every recorded offset; nonzero when junk precedes
    /// the archive.
    pub offset_shift: u64,
    /// Central directory offset as recorded.
    pub central_offset: u64,
    /// Central directory size as recorded.
    pub central_size: u64,
    /// Entry count as recorded.
    pub declared_entries: u64,
    /// True when fewer central headers were readable than declared.
    pub truncated_central: bool,
    /// The archive comment.
    pub comment: Vec<u8>,
    /// Bytes between the end of the last central header and the end
    /// records, or between the end record and the end of the source after
    /// the comment; zero for a well-formed archive.
    pub trailing_garbage: u64,
    /// True when no central directory was found and the entry list was
    /// rebuilt by scanning for local file headers.
    pub reconstructed: bool,
}

/// A parsed archive: central directory in memory, entry data on demand.
pub struct Archive {
    source: Arc<dyn Source>,
    /// The tail of the file as read at open; empty for in-memory sources.
    cache: Box<[u8]>,
    cache_start: u64,
    entries: Vec<Entry>,
    index: FastMap<String, usize>,
    duplicates: Vec<usize>,
    layout: Layout,
}

/// Where a requested byte range was found.
#[derive(Clone, Copy)]
enum Located {
    /// At this index of the source's in-memory bytes.
    Bytes(usize),
    /// At this index of the tail cache.
    Cache(usize),
    /// Read into the caller's scratch buffer.
    Scratch,
}

/// Serves `len` bytes at `offset` from in-memory bytes or `cache` when
/// possible, reading into `scratch` otherwise.
fn locate_range(
    source: &dyn Source,
    cache: &[u8],
    cache_start: u64,
    offset: u64,
    len: usize,
    scratch: &mut Vec<u8>,
) -> io::Result<Located> {
    let end = offset.checked_add(len as u64).ok_or_else(out_of_bounds)?;
    if let Some(bytes) = source.as_bytes() {
        if end > bytes.len() as u64 {
            return Err(out_of_bounds());
        }
        return Ok(Located::Bytes(offset as usize));
    }
    if offset >= cache_start && end <= cache_start + cache.len() as u64 {
        return Ok(Located::Cache((offset - cache_start) as usize));
    }
    scratch.clear();
    scratch.resize(len, 0);
    source.read_at(offset, scratch)?;
    Ok(Located::Scratch)
}

fn view_range<'a>(
    source: &'a dyn Source,
    cache: &'a [u8],
    located: Located,
    len: usize,
    scratch: &'a [u8],
) -> &'a [u8] {
    match located {
        Located::Bytes(start) => &source.as_bytes().unwrap_or_default()[start..start + len],
        Located::Cache(start) => &cache[start..start + len],
        Located::Scratch => &scratch[..len],
    }
}

impl Archive {
    /// Parses the central directory of `source`.
    pub fn open(source: Arc<dyn Source>) -> Result<Self> {
        let len = source.len();
        let tail_len = match len <= WHOLE_FILE_LEN {
            true => len,
            false => TAIL_LEN,
        };
        let tail_len = usize::try_from(tail_len).map_err(|_| Error::NotZip)?;
        let tail_start = len - tail_len as u64;
        let cache: Box<[u8]> = match source.as_bytes() {
            Some(_) => Box::default(),
            None => {
                let mut tail = vec![0u8; tail_len];
                source.read_at(tail_start, &mut tail)?;
                tail.into_boxed_slice()
            }
        };
        let cache_start = tail_start;
        let tail: &[u8] = match source.as_bytes() {
            Some(bytes) => &bytes[tail_start as usize..],
            None => &cache,
        };
        let Some(end_pos) = find_end_record(tail) else {
            return Self::reconstruct(source, len);
        };
        let end = &tail[end_pos..];
        let end_record_offset = tail_start + end_pos as u64;
        let comment_len = usize::from(u16_at(end, 20));
        let comment = end[END_RECORD_LEN..]
            .get(..comment_len)
            .unwrap_or(&end[END_RECORD_LEN..])
            .to_vec();
        let after_comment = end.len().saturating_sub(END_RECORD_LEN + comment_len) as u64;

        let mut layout = Layout {
            len,
            end_record_offset,
            comment,
            trailing_garbage: after_comment,
            ..Layout::default()
        };
        let mut declared_entries = u64::from(u16_at(end, 10));
        let mut central_size = u64::from(u32_at(end, 12));
        let mut central_offset = u64::from(u32_at(end, 16));
        let mut records_start = end_record_offset;

        let locator_pos = end_pos.checked_sub(ZIP64_LOCATOR_LEN);
        let has_locator = locator_pos.is_some_and(|pos| tail[pos..pos + 4] == ZIP64_LOCATOR_SIG);
        let needs_zip64 = declared_entries == 0xffff
            || central_size == 0xffff_ffff
            || central_offset == 0xffff_ffff;
        if has_locator && (needs_zip64 || true) {
            let locator = &tail[locator_pos.unwrap_or(0)..];
            let declared_z64 = u64_at(locator, 8);
            let locator_offset = end_record_offset - ZIP64_LOCATOR_LEN as u64;
            let (z64_pos, shift) = locate_zip64_end(&source, declared_z64, locator_offset)?;
            let mut record = [0u8; ZIP64_END_RECORD_LEN];
            source.read_at(z64_pos, &mut record)?;
            layout.zip64 = true;
            layout.offset_shift = shift;
            declared_entries = u64_at(&record, 32);
            central_size = u64_at(&record, 40);
            central_offset = u64_at(&record, 48);
            records_start = z64_pos;
        } else if needs_zip64 {
            return Err(Error::zip(
                end_record_offset,
                "end record needs a Zip64 record that is missing",
            ));
        }

        layout.declared_entries = declared_entries;
        layout.central_size = central_size;
        layout.central_offset = central_offset;

        let central_start = match locate_central(
            &source,
            central_offset,
            central_size,
            records_start,
            layout.offset_shift,
        ) {
            Ok(start) => start,
            Err(_) => return Self::reconstruct(source, len),
        };
        layout.offset_shift = central_start.wrapping_sub(central_offset);
        let readable = records_start.saturating_sub(central_start);
        let central_len = usize::try_from(central_size.min(readable))
            .map_err(|_| Error::zip(central_start, "central directory too large"))?;
        let mut scratch = Vec::new();
        let located = locate_range(
            source.as_ref(),
            &cache,
            cache_start,
            central_start,
            central_len,
            &mut scratch,
        )?;
        let central = view_range(source.as_ref(), &cache, located, central_len, &scratch);
        layout.trailing_garbage += readable.saturating_sub(central_size);

        let mut entries =
            Vec::with_capacity(usize::try_from(declared_entries).unwrap_or(0).min(1 << 16));
        let mut cursor = 0usize;
        while cursor + CENTRAL_HEADER_LEN <= central.len()
            && (entries.len() as u64) < declared_entries
        {
            let header = &central[cursor..];
            if header[..4] != CENTRAL_HEADER_SIG {
                break;
            }
            let name_len = usize::from(u16_at(header, 28));
            let extra_len = usize::from(u16_at(header, 30));
            let comment_len = usize::from(u16_at(header, 32));
            let total = CENTRAL_HEADER_LEN + name_len + extra_len + comment_len;
            if cursor + total > central.len() {
                break;
            }
            let raw_name = header[CENTRAL_HEADER_LEN..CENTRAL_HEADER_LEN + name_len].to_vec();
            let extra =
                &header[CENTRAL_HEADER_LEN + name_len..CENTRAL_HEADER_LEN + name_len + extra_len];
            let mut entry = Entry {
                name: String::from_utf8_lossy(&raw_name).into_owned(),
                raw_name,
                method: u16_at(header, 10),
                flags: u16_at(header, 8),
                version_needed: u16_at(header, 6),
                crc32: u32_at(header, 16),
                compressed_size: u64::from(u32_at(header, 20)),
                uncompressed_size: u64::from(u32_at(header, 24)),
                header_offset: u64::from(u32_at(header, 42)),
                central_offset: central_start + cursor as u64,
                extra_ids: Vec::new(),
                has_comment: comment_len > 0,
            };
            apply_extras(&mut entry, extra);
            entries.push(entry);
            cursor += total;
        }
        layout.truncated_central = (entries.len() as u64) < declared_entries;

        let mut index = FastMap::default();
        index.reserve(entries.len());
        let mut duplicates = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            if index.contains_key(&entry.name) {
                duplicates.push(i);
                continue;
            }
            index.insert(entry.name.clone(), i);
        }

        Ok(Self {
            source,
            cache,
            cache_start,
            entries,
            index,
            duplicates,
            layout,
        })
    }

    /// Rebuilds the entry list from local file headers when the central
    /// directory is missing or unreadable (a truncated download, a file
    /// cut before its directory). Entries with a data descriptor take their
    /// sizes from the descriptor found before the next header.
    fn reconstruct(source: Arc<dyn Source>, len: u64) -> Result<Self> {
        let cache: Box<[u8]> = Box::default();
        let cache_start = 0;
        let total = usize::try_from(len).map_err(|_| Error::NotZip)?;
        let mut data = vec![0u8; total];
        source.read_at(0, &mut data)?;
        let positions: Vec<usize> = memmem::find_iter(&data, &LOCAL_HEADER_SIG).collect();
        if positions.is_empty() {
            return Err(Error::NotZip);
        }
        let mut entries = Vec::with_capacity(positions.len());
        for (i, &pos) in positions.iter().enumerate() {
            let Some(header) = data.get(pos..pos + LOCAL_HEADER_LEN) else {
                break;
            };
            let name_len = usize::from(u16_at(header, 26));
            let extra_len = usize::from(u16_at(header, 28));
            let data_offset = pos + LOCAL_HEADER_LEN + name_len + extra_len;
            let Some(raw_name) =
                data.get(pos + LOCAL_HEADER_LEN..pos + LOCAL_HEADER_LEN + name_len)
            else {
                break;
            };
            if raw_name.is_empty() || raw_name.contains(&0) || data_offset > data.len() {
                continue;
            }
            let flags = u16_at(header, 6);
            let next = positions.get(i + 1).copied().unwrap_or_else(|| {
                memmem::find(&data[data_offset..], &CENTRAL_HEADER_SIG)
                    .map_or(data.len(), |rel| data_offset + rel)
            });
            let (crc, compressed_size, uncompressed_size) = match flags & FLAG_DATA_DESCRIPTOR != 0
            {
                false => (
                    u32_at(header, 14),
                    u64::from(u32_at(header, 18)),
                    u64::from(u32_at(header, 22)),
                ),
                true => match descriptor_before(&data, data_offset, next) {
                    Some(descriptor) => descriptor,
                    None => continue,
                },
            };
            entries.push(Entry {
                name: String::from_utf8_lossy(raw_name).into_owned(),
                raw_name: raw_name.to_vec(),
                method: u16_at(header, 8),
                flags,
                version_needed: u16_at(header, 4),
                crc32: crc,
                compressed_size,
                uncompressed_size,
                header_offset: pos as u64,
                central_offset: 0,
                extra_ids: Vec::new(),
                has_comment: false,
            });
        }
        if entries.is_empty() {
            return Err(Error::NotZip);
        }
        let mut index = FastMap::default();
        let mut duplicates = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            if index.contains_key(&entry.name) {
                duplicates.push(i);
                continue;
            }
            index.insert(entry.name.clone(), i);
        }
        let layout = Layout {
            len,
            declared_entries: entries.len() as u64,
            reconstructed: true,
            ..Layout::default()
        };
        Ok(Self {
            source,
            cache,
            cache_start,
            entries,
            index,
            duplicates,
            layout,
        })
    }

    /// Opens an archive held entirely in memory.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        Self::open(Arc::new(bytes))
    }

    /// Opens a file with positioned reads.
    pub fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        Self::open(Arc::new(FileSource::open(path)?))
    }

    pub fn source(&self) -> &Arc<dyn Source> {
        &self.source
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The first entry with this exact name.
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.index.get(name).map(|&i| &self.entries[i])
    }

    pub fn get(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    /// Indexes of entries whose name repeats an earlier entry's name.
    pub fn duplicates(&self) -> &[usize] {
        &self.duplicates
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Serves `len` bytes at `offset`, borrowing them from memory when the
    /// source or the tail cache holds them and reading into `scratch` otherwise.
    fn locate(&self, offset: u64, len: usize, scratch: &mut Vec<u8>) -> Result<Located> {
        locate_range(
            self.source.as_ref(),
            &self.cache,
            self.cache_start,
            offset,
            len,
            scratch,
        )
        .map_err(|_| Error::zip(offset, "read past the end of the archive"))
    }

    fn view<'a>(&'a self, located: Located, len: usize, scratch: &'a [u8]) -> &'a [u8] {
        view_range(self.source.as_ref(), &self.cache, located, len, scratch)
    }

    /// The compressed bytes of `entry`: one read covering the local header
    /// and the data when the sizes are known, a second read otherwise.
    fn raw_slice<'a>(&'a self, entry: &Entry, scratch: &'a mut Vec<u8>) -> Result<&'a [u8]> {
        let offset = entry.header_offset.wrapping_add(self.layout.offset_shift);
        let available = self
            .layout
            .len
            .checked_sub(offset)
            .filter(|available| *available >= LOCAL_HEADER_LEN as u64)
            .ok_or_else(|| Error::zip(offset, "local header out of bounds"))?;
        let size_hint = match entry.compressed_size > 0 || entry.uncompressed_size == 0 {
            true => entry.compressed_size,
            false => 0,
        };
        let want = (LOCAL_HEADER_LEN + entry.raw_name.len() + 64) as u64 + size_hint;
        let want = usize::try_from(want.min(available))
            .map_err(|_| Error::zip(offset, "entry too large"))?;
        let first = self.locate(offset, want, scratch)?;
        let (start, size, fits) = {
            let block = self.view(first, want, scratch);
            if block[..4] != LOCAL_HEADER_SIG {
                return Err(Error::zip(offset, "bad local header signature"));
            }
            let start =
                LOCAL_HEADER_LEN + usize::from(u16_at(block, 26)) + usize::from(u16_at(block, 28));
            let size = self.compressed_len(entry, offset + start as u64)?;
            (start, size, start + size <= block.len())
        };
        if fits {
            return Ok(&self.view(first, want, scratch)[start..start + size]);
        }
        let second = self.locate(offset + start as u64, size, scratch)?;
        Ok(self.view(second, size, scratch))
    }

    /// Reads and checks the local header of `entry`.
    pub fn local_header(&self, entry: &Entry) -> Result<LocalHeader> {
        let offset = entry.header_offset.wrapping_add(self.layout.offset_shift);
        let mut scratch = Vec::new();
        let located = self
            .locate(offset, LOCAL_HEADER_LEN, &mut scratch)
            .map_err(|_| Error::zip(offset, "local header out of bounds"))?;
        let mut fixed = [0u8; LOCAL_HEADER_LEN];
        fixed.copy_from_slice(self.view(located, LOCAL_HEADER_LEN, &scratch));
        if fixed[..4] != LOCAL_HEADER_SIG {
            return Err(Error::zip(offset, "bad local header signature"));
        }
        let name_len = usize::from(u16_at(&fixed, 26));
        let extra_len = u16_at(&fixed, 28);
        let located = self.locate(offset + LOCAL_HEADER_LEN as u64, name_len, &mut scratch)?;
        let raw_name = self.view(located, name_len, &scratch).to_vec();
        Ok(LocalHeader {
            version_needed: u16_at(&fixed, 4),
            flags: u16_at(&fixed, 6),
            method: u16_at(&fixed, 8),
            crc32: u32_at(&fixed, 14),
            compressed_size: u32_at(&fixed, 18),
            uncompressed_size: u32_at(&fixed, 22),
            raw_name,
            extra_len,
            data_offset: offset + (LOCAL_HEADER_LEN + name_len + usize::from(extra_len)) as u64,
        })
    }

    /// Reads the compressed bytes of `entry` into `out` (cleared first).
    pub fn read_raw(&self, entry: &Entry, out: &mut Vec<u8>) -> Result<()> {
        COMPRESSED.with(|cell| {
            let mut scratch = cell.borrow_mut();
            let raw = self.raw_slice(entry, &mut scratch)?;
            out.clear();
            out.extend_from_slice(raw);
            Ok(())
        })
    }

    /// Reads and decompresses `entry` into `out` (cleared first).
    pub fn read(&self, entry: &Entry, out: &mut Vec<u8>) -> Result<()> {
        if entry.is_encrypted() {
            return Err(Error::Unsupported(format!(
                "encrypted zip entry {}",
                entry.name
            )));
        }
        match entry.method {
            METHOD_STORED => self.read_raw(entry, out),
            METHOD_DEFLATE => COMPRESSED.with(|cell| {
                let mut scratch = cell.borrow_mut();
                let raw = self.raw_slice(entry, &mut scratch)?;
                out.clear();
                let expected = usize::try_from(entry.uncompressed_size).unwrap_or(0);
                inflate(raw, expected, out)
            }),
            other => Err(Error::Unsupported(format!(
                "zip compression method {other} for {}",
                entry.name
            ))),
        }
    }

    pub fn read_to_vec(&self, entry: &Entry) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.read(entry, &mut out)?;
        Ok(out)
    }

    /// Whether `data` (the decompressed bytes) matches the recorded CRC-32.
    pub fn crc_matches(entry: &Entry, data: &[u8]) -> bool {
        crc32(data) == entry.crc32
    }

    fn compressed_len(&self, entry: &Entry, data_offset: u64) -> Result<usize> {
        let recorded = entry.compressed_size;
        let known = recorded > 0 || entry.uncompressed_size == 0;
        let size = match known {
            true => recorded,
            false => self.next_boundary(entry).saturating_sub(data_offset),
        };
        let end = data_offset
            .checked_add(size)
            .filter(|end| *end <= self.layout.len);
        if end.is_none() {
            return Err(Error::zip(
                data_offset,
                format!("data of {} runs past the end of the archive", entry.name),
            ));
        }
        usize::try_from(size).map_err(|_| Error::zip(data_offset, "entry too large"))
    }

    fn next_boundary(&self, entry: &Entry) -> u64 {
        let shift = self.layout.offset_shift;
        let start = entry.header_offset.wrapping_add(shift);
        self.entries
            .iter()
            .map(|other| other.header_offset.wrapping_add(shift))
            .filter(|offset| *offset > start)
            .min()
            .unwrap_or(self.layout.central_offset.wrapping_add(shift))
    }
}

/// Reads the data descriptor that ends just before `next`, with or without
/// its signature, returning `(crc, compressed size, uncompressed size)`
/// when the compressed size is consistent with the span it describes.
fn descriptor_before(data: &[u8], data_offset: usize, next: usize) -> Option<(u32, u64, u64)> {
    for (len, signed) in [(16usize, true), (12, false)] {
        let Some(start) = next.checked_sub(len) else {
            continue;
        };
        if start < data_offset {
            continue;
        }
        let descriptor = &data[start..next];
        let body = match signed {
            true if descriptor[..4] == 0x0807_4b50u32.to_le_bytes() => &descriptor[4..],
            true => continue,
            false => descriptor,
        };
        let compressed = u64::from(u32_at(body, 4));
        if compressed == (start - data_offset) as u64 {
            return Some((u32_at(body, 0), compressed, u64::from(u32_at(body, 8))));
        }
    }
    None
}

fn find_end_record(tail: &[u8]) -> Option<usize> {
    let mut candidates =
        memmem::rfind_iter(tail, &END_RECORD_SIG).filter(|pos| pos + END_RECORD_LEN <= tail.len());
    let mut fallback = None;
    for pos in candidates.by_ref() {
        let comment_len = usize::from(u16_at(&tail[pos..], 20));
        if pos + END_RECORD_LEN + comment_len == tail.len() {
            return Some(pos);
        }
        fallback.get_or_insert(pos);
    }
    fallback
}

fn locate_zip64_end(
    source: &Arc<dyn Source>,
    declared: u64,
    locator_offset: u64,
) -> Result<(u64, u64)> {
    let mut sig = [0u8; 4];
    if source.read_at(declared, &mut sig).is_ok() && sig == ZIP64_END_RECORD_SIG {
        return Ok((declared, 0));
    }
    let candidate = locator_offset.saturating_sub(ZIP64_END_RECORD_LEN as u64);
    source.read_at(candidate, &mut sig)?;
    if sig != ZIP64_END_RECORD_SIG {
        return Err(Error::zip(
            declared,
            "zip64 end of central directory record not found",
        ));
    }
    Ok((candidate, candidate.wrapping_sub(declared)))
}

fn locate_central(
    source: &Arc<dyn Source>,
    declared: u64,
    size: u64,
    records_start: u64,
    shift: u64,
) -> Result<u64> {
    let mut sig = [0u8; 4];
    let shifted = declared.wrapping_add(shift);
    if size == 0 {
        return Ok(shifted);
    }
    if source.read_at(shifted, &mut sig).is_ok() && sig == CENTRAL_HEADER_SIG {
        return Ok(shifted);
    }
    let candidate = records_start.saturating_sub(size);
    source
        .read_at(candidate, &mut sig)
        .map_err(|_| Error::zip(declared, "central directory out of bounds"))?;
    if sig != CENTRAL_HEADER_SIG {
        return Err(Error::zip(declared, "central directory not found"));
    }
    Ok(candidate)
}

fn apply_extras(entry: &mut Entry, mut extra: &[u8]) {
    while extra.len() >= 4 {
        let id = u16_at(extra, 0);
        let len = usize::from(u16_at(extra, 2));
        let body = extra.get(4..4 + len).unwrap_or(&extra[4..]);
        entry.extra_ids.push(id);
        if id == ZIP64_EXTRA_ID {
            let mut fields = body
                .as_chunks::<8>()
                .0
                .iter()
                .map(|chunk| u64::from_le_bytes(*chunk));
            if entry.uncompressed_size == 0xffff_ffff {
                entry.uncompressed_size = fields.next().unwrap_or(entry.uncompressed_size);
            }
            if entry.compressed_size == 0xffff_ffff {
                entry.compressed_size = fields.next().unwrap_or(entry.compressed_size);
            }
            if entry.header_offset == 0xffff_ffff {
                entry.header_offset = fields.next().unwrap_or(entry.header_offset);
            }
        }
        extra = extra.get(4 + len..).unwrap_or(&[]);
    }
}

/// Inflates a raw DEFLATE stream, pre-sizing `out` to `expected` bytes.
pub fn inflate(input: &[u8], expected: usize, out: &mut Vec<u8>) -> Result<()> {
    out.clear();
    crate::inflate::inflate(input, expected, out)
}

thread_local! {
    /// Scratch buffer for compressed bytes read from the source.
    static COMPRESSED: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[inline]
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[inline]
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[inline]
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    let mut chunk = [0u8; 8];
    chunk.copy_from_slice(&bytes[offset..offset + 8]);
    u64::from_le_bytes(chunk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pptxboss_testkit::ZipBuilder;

    fn names(archive: &Archive) -> Vec<&str> {
        archive
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    #[test]
    fn stored_and_deflated_entries_round_trip() {
        let bytes = ZipBuilder::new()
            .stored("a.txt", b"hello")
            .deflated("dir/b.xml", &[b'x'; 5000])
            .build();
        let archive = Archive::from_bytes(bytes).unwrap();
        assert_eq!(names(&archive), ["a.txt", "dir/b.xml"]);
        let a = archive.entry("a.txt").unwrap();
        assert_eq!(archive.read_to_vec(a).unwrap(), b"hello");
        let b = archive.entry("dir/b.xml").unwrap();
        assert_eq!(b.method, METHOD_DEFLATE);
        assert_eq!(archive.read_to_vec(b).unwrap(), vec![b'x'; 5000]);
        assert!(Archive::crc_matches(b, &[b'x'; 5000]));
        assert_eq!(archive.layout().offset_shift, 0);
        assert!(!archive.layout().zip64);
    }

    #[test]
    fn data_descriptors_with_and_without_signature_are_read() {
        for builder in [
            ZipBuilder::new().with_data_descriptors(),
            ZipBuilder::new().with_descriptor_signature(),
        ] {
            let bytes = builder
                .deflated("a", b"alpha")
                .deflated("b", b"beta")
                .build();
            let archive = Archive::from_bytes(bytes).unwrap();
            assert!(archive.entry("a").unwrap().has_data_descriptor());
            assert_eq!(
                archive.read_to_vec(archive.entry("a").unwrap()).unwrap(),
                b"alpha"
            );
            assert_eq!(
                archive.read_to_vec(archive.entry("b").unwrap()).unwrap(),
                b"beta"
            );
        }
    }

    #[test]
    fn zip64_records_are_followed() {
        let bytes = ZipBuilder::new()
            .with_zip64()
            .deflated("one", b"1")
            .stored("two", b"22")
            .build();
        let archive = Archive::from_bytes(bytes).unwrap();
        assert!(archive.layout().zip64);
        assert_eq!(archive.layout().declared_entries, 2);
        assert!(archive.entry("two").unwrap().has_zip64_extra());
        assert_eq!(
            archive.read_to_vec(archive.entry("one").unwrap()).unwrap(),
            b"1"
        );
        assert_eq!(
            archive.read_to_vec(archive.entry("two").unwrap()).unwrap(),
            b"22"
        );
    }

    #[test]
    fn junk_before_the_archive_is_compensated() {
        let bytes = ZipBuilder::new()
            .with_prefix(b"JUNKJUNKJUNK")
            .stored("a", b"A")
            .deflated("b", b"BB")
            .build();
        let archive = Archive::from_bytes(bytes).unwrap();
        assert_eq!(archive.layout().offset_shift, 12);
        assert_eq!(
            archive.read_to_vec(archive.entry("a").unwrap()).unwrap(),
            b"A"
        );
        assert_eq!(
            archive.read_to_vec(archive.entry("b").unwrap()).unwrap(),
            b"BB"
        );
    }

    #[test]
    fn junk_before_a_zip64_archive_is_compensated() {
        let bytes = ZipBuilder::new()
            .with_zip64()
            .with_prefix(b"xx")
            .stored("a", b"A")
            .build();
        let archive = Archive::from_bytes(bytes).unwrap();
        assert_eq!(archive.layout().offset_shift, 2);
        assert_eq!(
            archive.read_to_vec(archive.entry("a").unwrap()).unwrap(),
            b"A"
        );
    }

    #[test]
    fn the_archive_comment_is_kept_and_does_not_hide_the_end_record() {
        let comment = b"a comment containing PK\x05\x06 the signature bytes";
        let bytes = ZipBuilder::new()
            .with_comment(comment)
            .stored("a", b"A")
            .build();
        let archive = Archive::from_bytes(bytes).unwrap();
        assert_eq!(archive.layout().comment, comment);
        assert_eq!(archive.layout().trailing_garbage, 0);
        assert_eq!(names(&archive), ["a"]);
    }

    #[test]
    fn duplicate_names_keep_the_first_and_record_the_rest() {
        let bytes = ZipBuilder::new()
            .stored("a", b"first")
            .stored("a", b"second")
            .stored("b", b"")
            .build();
        let archive = Archive::from_bytes(bytes).unwrap();
        assert_eq!(archive.duplicates(), &[1]);
        assert_eq!(
            archive.read_to_vec(archive.entry("a").unwrap()).unwrap(),
            b"first"
        );
    }

    #[test]
    fn not_a_zip_is_reported() {
        assert!(matches!(
            Archive::from_bytes(b"<?xml version=\"1.0\"?><x/>".to_vec()),
            Err(Error::NotZip)
        ));
        assert!(matches!(
            Archive::from_bytes(Vec::new()),
            Err(Error::NotZip)
        ));
        assert!(matches!(
            Archive::from_bytes(b"PK\x03\x04 and then nothing useful at all".to_vec()),
            Err(Error::NotZip)
        ));
    }

    #[test]
    fn a_missing_central_directory_is_rebuilt_from_local_headers() {
        for builder in [
            ZipBuilder::new(),
            ZipBuilder::new().with_data_descriptors(),
            ZipBuilder::new().with_descriptor_signature(),
        ] {
            let bytes = builder
                .deflated("a.xml", b"<a>alpha</a>")
                .stored("b.bin", b"BB")
                .deflated("c.xml", &[b'c'; 4000])
                .build();
            let central = memmem::find(&bytes, &CENTRAL_HEADER_SIG).unwrap();
            let truncated = bytes[..central + 7].to_vec();
            let archive = Archive::from_bytes(truncated).unwrap();
            assert!(archive.layout().reconstructed);
            assert_eq!(names(&archive), ["a.xml", "b.bin", "c.xml"]);
            assert_eq!(
                archive
                    .read_to_vec(archive.entry("a.xml").unwrap())
                    .unwrap(),
                b"<a>alpha</a>"
            );
            assert_eq!(
                archive
                    .read_to_vec(archive.entry("b.bin").unwrap())
                    .unwrap(),
                b"BB"
            );
            assert_eq!(
                archive
                    .read_to_vec(archive.entry("c.xml").unwrap())
                    .unwrap(),
                vec![b'c'; 4000]
            );
            assert!(Archive::crc_matches(
                archive.entry("c.xml").unwrap(),
                &[b'c'; 4000]
            ));
        }
    }

    #[test]
    fn a_file_cut_inside_its_last_entry_still_yields_the_earlier_ones() {
        let bytes = ZipBuilder::new()
            .deflated("a.xml", b"<a/>")
            .deflated("b.xml", &[b'b'; 3000])
            .build();
        let second = memmem::find_iter(&bytes, &LOCAL_HEADER_SIG).nth(1).unwrap();
        let archive = Archive::from_bytes(bytes[..second + 40].to_vec()).unwrap();
        assert!(archive.layout().reconstructed);
        assert_eq!(
            archive
                .read_to_vec(archive.entry("a.xml").unwrap())
                .unwrap(),
            b"<a/>"
        );
        assert!(archive
            .read_to_vec(archive.entry("b.xml").unwrap())
            .is_err());
    }

    #[test]
    fn unsupported_method_and_encryption_are_refused_per_entry() {
        let bytes = ZipBuilder::new()
            .with_method_code(12)
            .stored("a", b"A")
            .build();
        let archive = Archive::from_bytes(bytes).unwrap();
        assert!(matches!(
            archive.read_to_vec(archive.entry("a").unwrap()),
            Err(Error::Unsupported(_))
        ));
        let mut bytes = ZipBuilder::new().stored("e", b"E").build();
        let central = memmem::find(&bytes, &CENTRAL_HEADER_SIG).unwrap();
        bytes[central + 8] |= FLAG_ENCRYPTED as u8;
        let archive = Archive::from_bytes(bytes).unwrap();
        assert!(archive.entry("e").unwrap().is_encrypted());
        assert!(matches!(
            archive.read_to_vec(archive.entry("e").unwrap()),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn a_truncated_central_directory_yields_the_readable_entries() {
        let bytes = ZipBuilder::new()
            .stored("a", b"A")
            .stored("b", b"B")
            .stored("c", b"C")
            .build();
        let central = memmem::find(&bytes, &CENTRAL_HEADER_SIG).unwrap();
        let end = memmem::rfind(&bytes, &END_RECORD_SIG).unwrap();
        let third = memmem::find_iter(&bytes[central..end], &CENTRAL_HEADER_SIG)
            .nth(2)
            .unwrap()
            + central;
        let mut damaged = bytes[..third].to_vec();
        damaged.extend_from_slice(&bytes[end..]);
        let archive = Archive::from_bytes(damaged).unwrap();
        assert_eq!(names(&archive), ["a", "b"]);
        assert!(archive.layout().truncated_central);
        assert_eq!(
            archive.read_to_vec(archive.entry("b").unwrap()).unwrap(),
            b"B"
        );
    }

    #[test]
    fn local_header_reports_the_recorded_fields() {
        let bytes = ZipBuilder::new()
            .with_utf8_flag()
            .deflated("ppt/slides/slide1.xml", b"<p:sld/>")
            .build();
        let archive = Archive::from_bytes(bytes).unwrap();
        let entry = archive.entry("ppt/slides/slide1.xml").unwrap();
        assert!(entry.names_are_utf8());
        let header = archive.local_header(entry).unwrap();
        assert_eq!(header.raw_name, b"ppt/slides/slide1.xml");
        assert_eq!(header.method, METHOD_DEFLATE);
        assert_eq!(header.crc32, entry.crc32);
        assert_eq!(header.data_offset, 30 + 21);
    }

    #[test]
    fn a_file_source_reads_the_same_bytes() {
        let bytes = ZipBuilder::new().deflated("a", b"from a file").build();
        let path = std::env::temp_dir().join(format!("pptxboss-zip-{}.zip", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let archive = Archive::open_path(&path).unwrap();
        assert_eq!(
            archive.read_to_vec(archive.entry("a").unwrap()).unwrap(),
            b"from a file"
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn corrupt_deflate_data_is_an_inflate_error() {
        let mut bytes = ZipBuilder::new().deflated("a", &[b'z'; 3000]).build();
        let data_start = 30 + 1;
        for byte in &mut bytes[data_start + 2..data_start + 12] {
            *byte = 0xff;
        }
        let archive = Archive::from_bytes(bytes).unwrap();
        assert!(matches!(
            archive.read_to_vec(archive.entry("a").unwrap()),
            Err(Error::Inflate(_))
        ));
    }

    #[test]
    fn missing_compressed_size_falls_back_to_the_next_header() {
        let mut bytes = ZipBuilder::new()
            .with_data_descriptors()
            .deflated("a", b"alpha alpha alpha")
            .stored("b", b"beta")
            .build();
        let central = memmem::find(&bytes, &CENTRAL_HEADER_SIG).unwrap();
        for byte in &mut bytes[central + 20..central + 24] {
            *byte = 0;
        }
        let archive = Archive::from_bytes(bytes).unwrap();
        let a = archive.entry("a").unwrap();
        assert_eq!(a.compressed_size, 0);
        assert_eq!(archive.read_to_vec(a).unwrap(), b"alpha alpha alpha");
    }
}
