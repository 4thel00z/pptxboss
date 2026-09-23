//! Pixel dimensions read from image headers.

use crate::ImageFormat;

/// Width and height in pixels of a PNG, JPEG, GIF or BMP; None for other
/// formats or truncated data.
pub(crate) fn dimensions(data: &[u8]) -> Option<(u32, u32)> {
    match ImageFormat::sniff(data)? {
        ImageFormat::Png => png(data),
        ImageFormat::Jpeg => jpeg(data),
        ImageFormat::Gif => gif(data),
        ImageFormat::Bmp => bmp(data),
        ImageFormat::Tiff => None,
    }
}

fn be32(data: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

fn be16(data: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 2] = data.get(at..at + 2)?.try_into().ok()?;
    Some(u16::from_be_bytes(bytes) as u32)
}

fn le16(data: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 2] = data.get(at..at + 2)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes) as u32)
}

fn le32(data: &[u8], at: usize) -> Option<i32> {
    let bytes: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
    Some(i32::from_le_bytes(bytes))
}

fn nonzero(width: u32, height: u32) -> Option<(u32, u32)> {
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

fn png(data: &[u8]) -> Option<(u32, u32)> {
    if data.get(12..16)? != b"IHDR" {
        return None;
    }
    nonzero(be32(data, 16)?, be32(data, 20)?)
}

/// Walks the marker segments to the first start-of-frame marker.
fn jpeg(data: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2usize;
    loop {
        if *data.get(at)? != 0xFF {
            return None;
        }
        let marker = *data.get(at + 1)?;
        if marker == 0xFF {
            at += 1;
            continue;
        }
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            at += 2;
            continue;
        }
        let length = be16(data, at + 2)? as usize;
        let start_of_frame =
            (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if start_of_frame {
            return nonzero(be16(data, at + 7)?, be16(data, at + 5)?);
        }
        at += 2 + length;
    }
}

fn gif(data: &[u8]) -> Option<(u32, u32)> {
    nonzero(le16(data, 6)?, le16(data, 8)?)
}

fn bmp(data: &[u8]) -> Option<(u32, u32)> {
    let width = le32(data, 18)?;
    let height = le32(data, 22)?;
    nonzero(width.unsigned_abs(), height.unsigned_abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_gif_bmp_headers() {
        let mut png_bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        png_bytes.extend_from_slice(b"IHDR");
        png_bytes.extend_from_slice(&640u32.to_be_bytes());
        png_bytes.extend_from_slice(&480u32.to_be_bytes());
        assert_eq!(dimensions(&png_bytes), Some((640, 480)));
        assert_eq!(dimensions(&png_bytes[..20]), None);

        let mut gif_bytes = b"GIF89a".to_vec();
        gif_bytes.extend_from_slice(&300u16.to_le_bytes());
        gif_bytes.extend_from_slice(&200u16.to_le_bytes());
        assert_eq!(dimensions(&gif_bytes), Some((300, 200)));

        let mut bmp_bytes = vec![0u8; 26];
        bmp_bytes[..2].copy_from_slice(b"BM");
        bmp_bytes[18..22].copy_from_slice(&32i32.to_le_bytes());
        bmp_bytes[22..26].copy_from_slice(&(-16i32).to_le_bytes());
        assert_eq!(dimensions(&bmp_bytes), Some((32, 16)));
        assert_eq!(dimensions(b"<svg/>"), None);
    }

    #[test]
    fn jpeg_skips_segments_to_the_frame_header() {
        let mut jpeg_bytes = vec![0xFF, 0xD8];
        jpeg_bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46]);
        jpeg_bytes.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x03, 0x00]);
        jpeg_bytes.extend_from_slice(&[0xFF, 0xC2, 0x00, 0x0B, 0x08]);
        jpeg_bytes.extend_from_slice(&1080u16.to_be_bytes());
        jpeg_bytes.extend_from_slice(&1920u16.to_be_bytes());
        jpeg_bytes.extend_from_slice(&[0x03, 0, 0, 0, 0, 0, 0]);
        assert_eq!(dimensions(&jpeg_bytes), Some((1920, 1080)));
        assert_eq!(dimensions(&jpeg_bytes[..12]), None);
        assert_eq!(dimensions(&[0xFF, 0xD8, 0xFF, 0xD9, 0x00]), None);
    }
}
