//! CRC-32 as used by ZIP (IEEE 802.3 polynomial 0x04C11DB7, reflected),
//! computed eight bytes per step with a slicing-by-8 table.

const POLY: u32 = 0xedb8_8320;

const fn build_tables() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = match crc & 1 {
                0 => crc >> 1,
                _ => (crc >> 1) ^ POLY,
            };
            bit += 1;
        }
        tables[0][i] = crc;
        i += 1;
    }
    let mut table = 1;
    while table < 8 {
        let mut i = 0;
        while i < 256 {
            let previous = tables[table - 1][i];
            tables[table][i] = (previous >> 8) ^ tables[0][(previous & 0xff) as usize];
            i += 1;
        }
        table += 1;
    }
    tables
}

static TABLES: [[u32; 256]; 8] = build_tables();

/// CRC-32 of `data`.
pub fn crc32(data: &[u8]) -> u32 {
    update(0, data)
}

/// Continues a CRC-32 computation: `update(crc32(a), b) == crc32(a ++ b)`.
pub fn update(crc: u32, data: &[u8]) -> u32 {
    let mut crc = !crc;
    let (chunks, remainder) = data.as_chunks::<8>();
    for chunk in chunks {
        let low = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ crc;
        let high = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        crc = TABLES[7][(low & 0xff) as usize]
            ^ TABLES[6][((low >> 8) & 0xff) as usize]
            ^ TABLES[5][((low >> 16) & 0xff) as usize]
            ^ TABLES[4][(low >> 24) as usize]
            ^ TABLES[3][(high & 0xff) as usize]
            ^ TABLES[2][((high >> 8) & 0xff) as usize]
            ^ TABLES[1][((high >> 16) & 0xff) as usize]
            ^ TABLES[0][(high >> 24) as usize];
    }
    for &byte in remainder {
        crc = TABLES[0][((crc ^ u32::from(byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414f_a339
        );
    }

    #[test]
    fn sliced_path_matches_byte_path_on_every_length() {
        let data: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        for len in 0..data.len() {
            let slice = &data[..len];
            let bytewise = slice.iter().fold(0u32, |crc, byte| update(crc, &[*byte]));
            assert_eq!(crc32(slice), bytewise, "length {len}");
        }
    }

    #[test]
    fn update_continues_a_computation() {
        let whole = crc32(b"hello, world");
        let split = update(crc32(b"hello, "), b"world");
        assert_eq!(whole, split);
    }
}
