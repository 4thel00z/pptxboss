//! A deterministic ZIP writer for packages (ECMA-376 Part 2, Annex B):
//! stored or deflated entries, fixed timestamps, no data descriptors,
//! no Zip64, version needed 2.0.

use std::io::{self, Write};

use flate2::write::DeflateEncoder;
use flate2::Compression;
use pptxboss_core::crc32::crc32;

const LOCAL_HEADER_SIG: u32 = 0x0403_4b50;
const CENTRAL_HEADER_SIG: u32 = 0x0201_4b50;
const END_RECORD_SIG: u32 = 0x0605_4b50;
/// 1980-01-01 00:00:00 in MS-DOS time and date fields.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = 0x0021;

struct Central {
    name: Vec<u8>,
    method: u16,
    crc: u32,
    compressed: u32,
    uncompressed: u32,
    offset: u32,
}

/// Accumulates entries and produces the archive bytes.
pub struct ZipWriter {
    out: Vec<u8>,
    central: Vec<Central>,
}

impl ZipWriter {
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            central: Vec::new(),
        }
    }

    /// Adds an entry; `compress` selects deflate over stored.
    pub fn add(&mut self, name: &str, data: &[u8], compress: bool) -> io::Result<()> {
        let payload;
        let (method, body): (u16, &[u8]) = match compress {
            true => {
                let mut encoder =
                    DeflateEncoder::new(Vec::with_capacity(data.len() / 2), Compression::default());
                encoder.write_all(data)?;
                payload = encoder.finish()?;
                (8, &payload)
            }
            false => (0, data),
        };
        let crc = crc32(data);
        let offset = self.out.len() as u32;
        put_u32(&mut self.out, LOCAL_HEADER_SIG);
        put_u16(&mut self.out, 20);
        put_u16(&mut self.out, 0);
        put_u16(&mut self.out, method);
        put_u16(&mut self.out, DOS_TIME);
        put_u16(&mut self.out, DOS_DATE);
        put_u32(&mut self.out, crc);
        put_u32(&mut self.out, body.len() as u32);
        put_u32(&mut self.out, data.len() as u32);
        put_u16(&mut self.out, name.len() as u16);
        put_u16(&mut self.out, 0);
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(body);
        self.central.push(Central {
            name: name.as_bytes().to_vec(),
            method,
            crc,
            compressed: body.len() as u32,
            uncompressed: data.len() as u32,
            offset,
        });
        Ok(())
    }

    pub fn finish(mut self) -> Vec<u8> {
        let central_offset = self.out.len() as u32;
        for entry in &self.central {
            put_u32(&mut self.out, CENTRAL_HEADER_SIG);
            put_u16(&mut self.out, 20);
            put_u16(&mut self.out, 20);
            put_u16(&mut self.out, 0);
            put_u16(&mut self.out, entry.method);
            put_u16(&mut self.out, DOS_TIME);
            put_u16(&mut self.out, DOS_DATE);
            put_u32(&mut self.out, entry.crc);
            put_u32(&mut self.out, entry.compressed);
            put_u32(&mut self.out, entry.uncompressed);
            put_u16(&mut self.out, entry.name.len() as u16);
            put_u16(&mut self.out, 0);
            put_u16(&mut self.out, 0);
            put_u16(&mut self.out, 0);
            put_u16(&mut self.out, 0);
            put_u32(&mut self.out, 0);
            put_u32(&mut self.out, entry.offset);
            self.out.extend_from_slice(&entry.name);
        }
        let central_size = self.out.len() as u32 - central_offset;
        put_u32(&mut self.out, END_RECORD_SIG);
        put_u16(&mut self.out, 0);
        put_u16(&mut self.out, 0);
        put_u16(&mut self.out, self.central.len() as u16);
        put_u16(&mut self.out, self.central.len() as u16);
        put_u32(&mut self.out, central_size);
        put_u32(&mut self.out, central_offset);
        put_u16(&mut self.out, 0);
        self.out
    }
}

impl Default for ZipWriter {
    fn default() -> Self {
        Self::new()
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use pptxboss_core::zip::Archive;

    #[test]
    fn written_archives_read_back_and_are_deterministic() {
        let build = || {
            let mut writer = ZipWriter::new();
            writer.add("a.xml", b"<a>hello</a>", true).unwrap();
            writer.add("b.bin", &[1, 2, 3], false).unwrap();
            writer.finish()
        };
        let first = build();
        assert_eq!(first, build());
        let archive = Archive::from_bytes(first).unwrap();
        assert_eq!(archive.entries().len(), 2);
        assert_eq!(
            archive
                .read_to_vec(archive.entry("a.xml").unwrap())
                .unwrap(),
            b"<a>hello</a>"
        );
        assert_eq!(
            archive
                .read_to_vec(archive.entry("b.bin").unwrap())
                .unwrap(),
            [1, 2, 3]
        );
        assert!(!archive.layout().zip64);
        assert_eq!(archive.layout().offset_shift, 0);
        for entry in archive.entries() {
            let header = archive.local_header(entry).unwrap();
            assert_eq!(header.crc32, entry.crc32);
            assert!(!entry.has_data_descriptor());
        }
    }
}
