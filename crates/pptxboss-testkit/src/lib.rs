//! In-memory fixture builders for pptxboss tests.
//!
//! [`ZipBuilder`] writes ZIP archives with a controllable record layout
//! (stored or deflated entries, data descriptors, Zip64 records, a leading
//! stub, an archive comment) so container tests never hand-compute byte
//! offsets. The builder depends on nothing under test: it has its own
//! CRC-32 and serializes every header itself.

pub mod deck;
pub mod legacy;

use std::io::Write;

pub use deck::{Deck, DeckSlide};
pub use legacy::{compound_file, PptDeck, PptSlide};

use flate2::write::DeflateEncoder;
use flate2::Compression;

const LOCAL_HEADER_SIG: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIG: u32 = 0x0201_4b50;
const END_RECORD_SIG: u32 = 0x0605_4b50;
const DATA_DESCRIPTOR_SIG: u32 = 0x0807_4b50;
const ZIP64_END_RECORD_SIG: u32 = 0x0606_4b50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const FLAG_DATA_DESCRIPTOR: u16 = 1 << 3;
const FLAG_UTF8: u16 = 1 << 11;
const ZIP64_EXTRA_ID: u16 = 0x0001;

/// CRC-32 (IEEE 802.3 polynomial, reflected), as the ZIP format requires.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Compression method of one archive entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Stored,
    Deflated,
}

impl Method {
    fn code(self) -> u16 {
        match self {
            Method::Stored => 0,
            Method::Deflated => 8,
        }
    }
}

struct Entry {
    name: Vec<u8>,
    data: Vec<u8>,
    method: Method,
}

/// Builds a ZIP archive in memory.
#[derive(Default)]
pub struct ZipBuilder {
    entries: Vec<Entry>,
    data_descriptors: bool,
    descriptor_signature: bool,
    zip64: bool,
    utf8_flag: bool,
    prefix: Vec<u8>,
    comment: Vec<u8>,
    method_override: Option<u16>,
}

impl ZipBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an entry stored without compression.
    pub fn stored(self, name: &str, data: &[u8]) -> Self {
        self.entry(name, data, Method::Stored)
    }

    /// Adds a deflated entry.
    pub fn deflated(self, name: &str, data: &[u8]) -> Self {
        self.entry(name, data, Method::Deflated)
    }

    pub fn entry(mut self, name: &str, data: &[u8], method: Method) -> Self {
        self.entries.push(Entry {
            name: name.as_bytes().to_vec(),
            data: data.to_vec(),
            method,
        });
        self
    }

    /// Adds an entry whose name is arbitrary bytes (for non-UTF-8 name tests).
    pub fn raw_name_entry(mut self, name: &[u8], data: &[u8]) -> Self {
        self.entries.push(Entry {
            name: name.to_vec(),
            data: data.to_vec(),
            method: Method::Stored,
        });
        self
    }

    /// Writes CRC and sizes in a data descriptor after each entry's data
    /// (general purpose bit 3) instead of in the local header.
    pub fn with_data_descriptors(mut self) -> Self {
        self.data_descriptors = true;
        self
    }

    /// Prefixes each data descriptor with the optional 0x08074b50 signature.
    pub fn with_descriptor_signature(mut self) -> Self {
        self.data_descriptors = true;
        self.descriptor_signature = true;
        self
    }

    /// Emits Zip64 central directory extras and end records even though the
    /// archive is small.
    pub fn with_zip64(mut self) -> Self {
        self.zip64 = true;
        self
    }

    /// Sets general purpose bit 11 (names are UTF-8).
    pub fn with_utf8_flag(mut self) -> Self {
        self.utf8_flag = true;
        self
    }

    /// Prepends bytes before the first local header without adjusting any
    /// recorded offset, as happens when junk is prepended to a finished
    /// archive; readers have to detect the shift.
    pub fn with_prefix(mut self, prefix: &[u8]) -> Self {
        self.prefix = prefix.to_vec();
        self
    }

    /// Sets the archive comment stored after the end of central directory.
    pub fn with_comment(mut self, comment: &[u8]) -> Self {
        self.comment = comment.to_vec();
        self
    }

    /// Writes this compression method code into every header regardless of
    /// how the data was actually encoded (for unsupported-method tests).
    pub fn with_method_code(mut self, code: u16) -> Self {
        self.method_override = Some(code);
        self
    }

    pub fn build(&self) -> Vec<u8> {
        let mut out = self.prefix.clone();
        let base = self.prefix.len() as u64;
        let mut central = Vec::new();
        let flags = self.flags();
        for entry in &self.entries {
            let payload = encode(entry);
            let crc = crc32(&entry.data);
            let offset = out.len() as u64 - base;
            let method = self.method_override.unwrap_or(entry.method.code());
            let (header_crc, header_csize, header_usize) = match self.data_descriptors {
                true => (0, 0, 0),
                false => (crc, payload.len() as u32, entry.data.len() as u32),
            };
            put_u32(&mut out, LOCAL_HEADER_SIG);
            put_u16(&mut out, self.version_needed());
            put_u16(&mut out, flags);
            put_u16(&mut out, method);
            put_u16(&mut out, 0);
            put_u16(&mut out, 0x21);
            put_u32(&mut out, header_crc);
            put_u32(&mut out, header_csize);
            put_u32(&mut out, header_usize);
            put_u16(&mut out, entry.name.len() as u16);
            put_u16(&mut out, 0);
            out.extend_from_slice(&entry.name);
            out.extend_from_slice(&payload);
            if self.data_descriptors {
                if self.descriptor_signature {
                    put_u32(&mut out, DATA_DESCRIPTOR_SIG);
                }
                put_u32(&mut out, crc);
                put_u32(&mut out, payload.len() as u32);
                put_u32(&mut out, entry.data.len() as u32);
            }
            self.central_header(
                &mut central,
                entry,
                method,
                flags,
                crc,
                payload.len() as u64,
                offset,
            );
        }
        let central_offset = out.len() as u64 - base;
        out.extend_from_slice(&central);
        if self.zip64 {
            let zip64_end = out.len() as u64 - base;
            put_u32(&mut out, ZIP64_END_RECORD_SIG);
            put_u64(&mut out, 44);
            put_u16(&mut out, 45);
            put_u16(&mut out, 45);
            put_u32(&mut out, 0);
            put_u32(&mut out, 0);
            put_u64(&mut out, self.entries.len() as u64);
            put_u64(&mut out, self.entries.len() as u64);
            put_u64(&mut out, central.len() as u64);
            put_u64(&mut out, central_offset);
            put_u32(&mut out, ZIP64_LOCATOR_SIG);
            put_u32(&mut out, 0);
            put_u64(&mut out, zip64_end);
            put_u32(&mut out, 1);
        }
        put_u32(&mut out, END_RECORD_SIG);
        put_u16(&mut out, 0);
        put_u16(&mut out, 0);
        let count = match self.zip64 {
            true => 0xffff,
            false => self.entries.len() as u16,
        };
        put_u16(&mut out, count);
        put_u16(&mut out, count);
        match self.zip64 {
            true => {
                put_u32(&mut out, 0xffff_ffff);
                put_u32(&mut out, 0xffff_ffff);
            }
            false => {
                put_u32(&mut out, central.len() as u32);
                put_u32(&mut out, central_offset as u32);
            }
        }
        put_u16(&mut out, self.comment.len() as u16);
        out.extend_from_slice(&self.comment);
        out
    }

    fn flags(&self) -> u16 {
        let mut flags = 0;
        if self.data_descriptors {
            flags |= FLAG_DATA_DESCRIPTOR;
        }
        if self.utf8_flag {
            flags |= FLAG_UTF8;
        }
        flags
    }

    fn version_needed(&self) -> u16 {
        match self.zip64 {
            true => 45,
            false => 20,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn central_header(
        &self,
        central: &mut Vec<u8>,
        entry: &Entry,
        method: u16,
        flags: u16,
        crc: u32,
        compressed_size: u64,
        offset: u64,
    ) {
        let mut extra = Vec::new();
        if self.zip64 {
            put_u16(&mut extra, ZIP64_EXTRA_ID);
            put_u16(&mut extra, 24);
            put_u64(&mut extra, entry.data.len() as u64);
            put_u64(&mut extra, compressed_size);
            put_u64(&mut extra, offset);
        }
        put_u32(central, CENTRAL_HEADER_SIG);
        put_u16(central, self.version_needed());
        put_u16(central, self.version_needed());
        put_u16(central, flags);
        put_u16(central, method);
        put_u16(central, 0);
        put_u16(central, 0x21);
        put_u32(central, crc);
        match self.zip64 {
            true => {
                put_u32(central, 0xffff_ffff);
                put_u32(central, 0xffff_ffff);
            }
            false => {
                put_u32(central, compressed_size as u32);
                put_u32(central, entry.data.len() as u32);
            }
        }
        put_u16(central, entry.name.len() as u16);
        put_u16(central, extra.len() as u16);
        put_u16(central, 0);
        put_u16(central, 0);
        put_u16(central, 0);
        put_u32(central, 0);
        match self.zip64 {
            true => put_u32(central, 0xffff_ffff),
            false => put_u32(central, offset as u32),
        }
        central.extend_from_slice(&entry.name);
        central.extend_from_slice(&extra);
    }
}

fn encode(entry: &Entry) -> Vec<u8> {
    match entry.method {
        Method::Stored => entry.data.clone(),
        Method::Deflated => {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(&entry.data)
                .expect("writing to an in-memory deflate encoder cannot fail");
            encoder
                .finish()
                .expect("finishing an in-memory deflate encoder cannot fail")
        }
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_reference_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn a_stored_archive_ends_with_the_end_record_signature() {
        let bytes = ZipBuilder::new().stored("a.txt", b"hello").build();
        let tail = &bytes[bytes.len() - 22..];
        assert_eq!(&tail[..4], &END_RECORD_SIG.to_le_bytes());
        assert_eq!(&bytes[..4], &LOCAL_HEADER_SIG.to_le_bytes());
    }

    #[test]
    fn a_prefix_shifts_the_first_local_header() {
        let bytes = ZipBuilder::new()
            .with_prefix(b"STUB")
            .stored("a", b"x")
            .build();
        assert_eq!(&bytes[..4], b"STUB");
        assert_eq!(&bytes[4..8], &LOCAL_HEADER_SIG.to_le_bytes());
    }
}
