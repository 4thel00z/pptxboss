//! Transcoding of UTF-16 XML parts to UTF-8 (ECMA-376 Part 2 allows both
//! encodings for XML parts; every parser in this crate reads UTF-8).

/// The UTF-8 bytes of `bytes` when they are a UTF-16 XML part (a byte
/// order mark or `<?` in either byte order), None when they are not.
/// Unpaired surrogates become U+FFFD.
pub fn utf16_xml_to_utf8(bytes: &[u8]) -> Option<Vec<u8>> {
    let (little_endian, start) = match bytes {
        [0xff, 0xfe, ..] => (true, 2),
        [0xfe, 0xff, ..] => (false, 2),
        [b'<', 0, b'?', 0, ..] => (true, 0),
        [0, b'<', 0, b'?', ..] => (false, 0),
        _ => return None,
    };
    let (pairs, _) = bytes[start..].as_chunks::<2>();
    let units = pairs.iter().map(|pair| match little_endian {
        true => u16::from_le_bytes(*pair),
        false => u16::from_be_bytes(*pair),
    });
    let mut out = String::with_capacity(bytes.len() / 2);
    for ch in char::decode_utf16(units) {
        out.push(ch.unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    Some(out.into_bytes())
}

/// Whether `bytes` look like a UTF-16 XML part; see [`utf16_xml_to_utf8`].
pub fn is_utf16_xml(bytes: &[u8]) -> bool {
    matches!(
        bytes,
        [0xff, 0xfe, ..] | [0xfe, 0xff, ..] | [b'<', 0, b'?', 0, ..] | [0, b'<', 0, b'?', ..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16le(text: &str, bom: bool) -> Vec<u8> {
        let mut out = Vec::new();
        if bom {
            out.extend([0xff, 0xfe]);
        }
        for unit in text.encode_utf16() {
            out.extend(unit.to_le_bytes());
        }
        out
    }

    #[test]
    fn transcodes_both_byte_orders_with_and_without_a_mark() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><a>héllo 😀</a>";
        assert_eq!(
            utf16_xml_to_utf8(&utf16le(xml, true)).unwrap(),
            xml.as_bytes()
        );
        assert_eq!(
            utf16_xml_to_utf8(&utf16le(xml, false)).unwrap(),
            xml.as_bytes()
        );
        let mut big_endian = vec![0xfe, 0xff];
        for unit in xml.encode_utf16() {
            big_endian.extend(unit.to_be_bytes());
        }
        assert_eq!(utf16_xml_to_utf8(&big_endian).unwrap(), xml.as_bytes());
    }

    #[test]
    fn leaves_utf8_and_binaries_alone() {
        assert!(utf16_xml_to_utf8(b"<?xml version=\"1.0\"?><a/>").is_none());
        assert!(utf16_xml_to_utf8(&[0x89, b'P', b'N', b'G']).is_none());
        assert!(utf16_xml_to_utf8(&[0xff, 0xd8, 0xff, 0xe0]).is_none());
        assert!(!is_utf16_xml(b""));
    }

    #[test]
    fn a_lone_surrogate_becomes_the_replacement_character() {
        let mut bytes = utf16le("<a>", true);
        bytes.extend(0xd800u16.to_le_bytes());
        bytes.extend(utf16le("</a>", false));
        assert_eq!(
            utf16_xml_to_utf8(&bytes).unwrap(),
            "<a>\u{fffd}</a>".as_bytes()
        );
    }
}
