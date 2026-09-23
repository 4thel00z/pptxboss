//! Font embedding. Reads the tables of a TrueType or OpenType font that
//! name and describe it, and wraps the file as Embedded OpenType (EOT)
//! version 2.2, the container PowerPoint stores under `ppt/fonts`. The
//! font data is MicroType Express compressed, as PowerPoint writes it; a
//! font the coder cannot handle is carried as it is. The header fields
//! come from the `OS/2`, `head` and `name` tables.

use std::borrow::Cow;

use crate::mtx;
use crate::{Error, Result};

/// The EOT flag for MicroType Express compressed font data.
const COMPRESSED: u32 = 0x0000_0004;

/// What the writer needs to know about one font file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FontInfo {
    pub(crate) family: String,
    pub(crate) subfamily: String,
    pub(crate) full_name: String,
    pub(crate) version: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) panose: [u8; 10],
    pub(crate) weight: u16,
    pub(crate) fs_type: u16,
    pub(crate) unicode_range: [u32; 4],
    pub(crate) code_page_range: [u32; 2],
    pub(crate) checksum_adjustment: u32,
    pub(crate) fixed_pitch: bool,
}

impl FontInfo {
    /// The `pitchFamily` attribute of `p:font`: pitch in the low nibble
    /// (1 fixed, 2 variable), family in the high nibble (roman, swiss,
    /// modern, script, decorative) from the PANOSE family and serif style.
    pub(crate) fn pitch_family(&self) -> u8 {
        let pitch = match self.fixed_pitch {
            true => 1,
            false => 2,
        };
        let family = match (self.fixed_pitch, self.panose[0], self.panose[1]) {
            (true, _, _) => 0x30,
            (_, 3, _) => 0x40,
            (_, 4, _) => 0x50,
            (_, 2, 11..=15) => 0x20,
            (_, 2, 2..=10) => 0x10,
            _ => 0x00,
        };
        pitch | family
    }

    /// The PANOSE classification as the twenty hex digits `p:font` takes.
    pub(crate) fn panose_hex(&self) -> String {
        self.panose.iter().map(|b| format!("{b:02X}")).collect()
    }
}

fn be16(data: &[u8], at: usize) -> Option<u16> {
    let bytes: [u8; 2] = data.get(at..at + 2)?.try_into().ok()?;
    Some(u16::from_be_bytes(bytes))
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

fn malformed(what: &str) -> Error {
    Error::UnsupportedFont(what.to_string())
}

/// The byte range of a table by tag.
fn table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let count = be16(data, 4)? as usize;
    (0..count).find_map(|i| {
        let record = 12 + i * 16;
        if data.get(record..record + 4)? != tag {
            return None;
        }
        let offset = be32(data, record + 8)? as usize;
        let length = be32(data, record + 12)? as usize;
        data.get(offset..offset + length)
    })
}

/// A string from the `name` table by name id: the Windows Unicode entry
/// in US English when present, else any Windows or Unicode entry, else a
/// Macintosh Roman entry.
fn name(name_table: &[u8], id: u16) -> Option<String> {
    let count = be16(name_table, 2)? as usize;
    let strings = be16(name_table, 4)? as usize;
    let mut best: Option<(u8, String)> = None;
    for i in 0..count {
        let record = 6 + i * 12;
        let platform = be16(name_table, record)?;
        let encoding = be16(name_table, record + 2)?;
        let language = be16(name_table, record + 4)?;
        if be16(name_table, record + 6)? != id {
            continue;
        }
        let length = be16(name_table, record + 8)? as usize;
        let offset = strings + be16(name_table, record + 10)? as usize;
        let bytes = name_table.get(offset..offset + length)?;
        let (rank, text) = match (platform, encoding) {
            (3, 1) | (3, 10) | (0, _) => {
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes(*pair))
                    .collect();
                let rank = match (platform, language) {
                    (3, 0x409) => 3,
                    (3, _) => 2,
                    _ => 1,
                };
                (rank, String::from_utf16_lossy(&units))
            }
            (1, 0) => (0, bytes.iter().map(|b| *b as char).collect()),
            _ => continue,
        };
        if best.as_ref().is_none_or(|(kept, _)| rank > *kept) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, text)| text)
}

/// Reads the font's description; fails on anything but a single TrueType
/// or OpenType font, and on a font whose license forbids embedding.
pub(crate) fn info(data: &[u8]) -> Result<FontInfo> {
    let tag = data.get(0..4).ok_or_else(|| malformed("file too short"))?;
    if tag == b"ttcf" {
        return Err(malformed("font collections are not supported"));
    }
    if tag != [0, 1, 0, 0] && tag != b"OTTO" && tag != b"true" {
        return Err(malformed("not a TrueType or OpenType font"));
    }
    let name_table = table(data, b"name").ok_or_else(|| malformed("no name table"))?;
    let os2 = table(data, b"OS/2").ok_or_else(|| malformed("no OS/2 table"))?;
    let head = table(data, b"head").ok_or_else(|| malformed("no head table"))?;
    let field16 = |at: usize| be16(os2, at).ok_or_else(|| malformed("OS/2 table too short"));
    let field32 = |at: usize| be32(os2, at).ok_or_else(|| malformed("OS/2 table too short"));
    let os2_version = field16(0)?;
    let weight = field16(4)?;
    let fs_type = field16(8)?;
    if fs_type & 0x000F == 0x0002 {
        return Err(Error::FontEmbeddingRestricted(
            name(name_table, 4).unwrap_or_default(),
        ));
    }
    let mut panose = [0u8; 10];
    panose.copy_from_slice(
        os2.get(32..42)
            .ok_or_else(|| malformed("OS/2 table too short"))?,
    );
    let unicode_range = [field32(42)?, field32(46)?, field32(50)?, field32(54)?];
    let fs_selection = field16(62)?;
    let code_page_range = match os2_version {
        0 => [0, 0],
        _ => [field32(78)?, field32(82)?],
    };
    let mac_style = be16(head, 44).ok_or_else(|| malformed("head table too short"))?;
    let checksum_adjustment = be32(head, 8).ok_or_else(|| malformed("head table too short"))?;
    let fixed_pitch = table(data, b"post")
        .and_then(|post| be32(post, 12))
        .is_some_and(|flag| flag != 0);
    let family = name(name_table, 16)
        .or_else(|| name(name_table, 1))
        .ok_or_else(|| malformed("no family name"))?;
    let subfamily = name(name_table, 17)
        .or_else(|| name(name_table, 2))
        .unwrap_or_else(|| "Regular".to_string());
    Ok(FontInfo {
        full_name: name(name_table, 4).unwrap_or_else(|| family.clone()),
        version: name(name_table, 5).unwrap_or_default(),
        family,
        subfamily,
        bold: fs_selection & 0x20 != 0 || mac_style & 0x01 != 0,
        italic: fs_selection & 0x01 != 0 || mac_style & 0x02 != 0,
        panose,
        weight,
        fs_type,
        unicode_range,
        code_page_range,
        checksum_adjustment,
        fixed_pitch,
    })
}

fn push_name(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&[0, 0]);
    let units: Vec<u16> = text.encode_utf16().collect();
    let size = (units.len() as u16 + 1) * 2;
    out.extend_from_slice(&size.to_le_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
}

/// The font wrapped as EOT 2.2, compressed when it can be.
pub(crate) fn eot(data: &[u8], info: &FontInfo) -> Vec<u8> {
    let (body, flags): (Cow<'_, [u8]>, u32) = match mtx::compress(data) {
        Some(compressed) => (Cow::Owned(compressed), COMPRESSED),
        None => (Cow::Borrowed(data), 0),
    };
    let mut out = Vec::with_capacity(body.len() + 256);
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x0002_0002u32.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&info.panose);
    out.push(0);
    out.push(u8::from(info.italic));
    out.extend_from_slice(&(info.weight as u32).to_le_bytes());
    out.extend_from_slice(&info.fs_type.to_le_bytes());
    out.extend_from_slice(&0x504Cu16.to_le_bytes());
    for range in info.unicode_range {
        out.extend_from_slice(&range.to_le_bytes());
    }
    for range in info.code_page_range {
        out.extend_from_slice(&range.to_le_bytes());
    }
    out.extend_from_slice(&info.checksum_adjustment.to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    push_name(&mut out, &info.family);
    push_name(&mut out, &info.subfamily);
    push_name(&mut out, &info.version);
    push_name(&mut out, &info.full_name);
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&0x5047_5342u32.to_le_bytes());
    out.extend_from_slice(&0x0000_04E4u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 12]);
    let total = (out.len() + body.len()) as u32;
    out[..4].copy_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGULAR: &[u8] = include_bytes!("../tests/data/boxy-regular.ttf");
    const BOLD: &[u8] = include_bytes!("../tests/data/boxy-bold.ttf");

    #[test]
    fn fonts_describe_themselves() {
        let regular = info(REGULAR).unwrap();
        assert_eq!(regular.family, "Boxy");
        assert_eq!(regular.subfamily, "Regular");
        assert_eq!(regular.full_name, "Boxy Regular");
        assert_eq!(regular.version, "Version 1.000");
        assert!(!regular.bold && !regular.italic);
        assert_eq!(regular.weight, 400);
        assert_eq!(regular.panose[0], 2);
        assert_eq!(regular.pitch_family(), 0x22);
        assert_eq!(regular.panose_hex().len(), 20);
        let bold = info(BOLD).unwrap();
        assert!(bold.bold && !bold.italic);
        assert_eq!(bold.weight, 700);
    }

    #[test]
    fn bad_fonts_are_refused() {
        assert!(matches!(info(b"<svg/>"), Err(Error::UnsupportedFont(_))));
        assert!(matches!(info(b"ttcf"), Err(Error::UnsupportedFont(_))));
        let mut restricted = REGULAR.to_vec();
        let os2 = table(REGULAR, b"OS/2").unwrap();
        let start = os2.as_ptr() as usize - REGULAR.as_ptr() as usize;
        restricted[start + 8..start + 10].copy_from_slice(&2u16.to_be_bytes());
        assert!(matches!(
            info(&restricted),
            Err(Error::FontEmbeddingRestricted(name)) if name == "Boxy Regular"
        ));
    }

    #[test]
    fn eot_header_matches_the_layout_powerpoint_writes() {
        let described = info(REGULAR).unwrap();
        let wrapped = eot(REGULAR, &described);
        let le32 = |at: usize| u32::from_le_bytes(wrapped[at..at + 4].try_into().unwrap());
        let le16 = |at: usize| u16::from_le_bytes(wrapped[at..at + 2].try_into().unwrap());
        assert_eq!(le32(0) as usize, wrapped.len());
        let body = &wrapped[wrapped.len() - le32(4) as usize..];
        assert!(body.len() < REGULAR.len());
        assert_eq!(le32(8), 0x0002_0002);
        assert_eq!(le32(12), COMPRESSED);
        assert_eq!(&wrapped[16..26], &described.panose);
        assert_eq!(wrapped[27], 0);
        assert_eq!(le32(28), 400);
        assert_eq!(le16(34), 0x504C);
        assert_eq!(le32(60), described.checksum_adjustment);
        assert_eq!(&wrapped[64..80], &[0u8; 16]);
        assert_eq!(le16(80), 0);
        assert_eq!(le16(82), 10);
        assert_eq!(&wrapped[84..94], b"B\0o\0x\0y\0\0\0");
        assert_eq!(body[0], 3);
        assert_eq!(
            &wrapped[wrapped.len() - body.len() - 12..][..4],
            &[0, 0, 0, 0]
        );
    }
}
