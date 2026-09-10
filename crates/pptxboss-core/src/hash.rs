//! A fast, non-cryptographic hasher for the short keys the reader hashes in
//! bulk: ZIP item names, part names, relationship ids and XML names. The
//! standard library's SipHash is DoS-resistant and far slower than needed
//! for in-process maps over already-parsed input.
//!
//! The construction is FxHash: fold each machine word of the key into an
//! accumulator with a rotate, an xor and a multiply by a fixed odd
//! constant. It is unseeded, so iteration order is stable across runs.

use std::hash::{BuildHasherDefault, Hasher};

/// A [`std::collections::HashMap`] using [`FxHasher`].
pub type FastMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// A [`std::collections::HashSet`] using [`FxHasher`].
pub type FastSet<K> = std::collections::HashSet<K, BuildHasherDefault<FxHasher>>;

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;
const ROTATE: u32 = 5;

#[derive(Default)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(ROTATE) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            let mut chunk = [0u8; 8];
            chunk.copy_from_slice(&bytes[..8]);
            self.add(u64::from_le_bytes(chunk));
            bytes = &bytes[8..];
        }
        if bytes.len() >= 4 {
            let mut chunk = [0u8; 4];
            chunk.copy_from_slice(&bytes[..4]);
            self.add(u64::from(u32::from_le_bytes(chunk)));
            bytes = &bytes[4..];
        }
        for &byte in bytes {
            self.add(u64::from(byte));
        }
    }

    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    #[inline]
    fn write_u16(&mut self, value: u16) {
        self.add(u64::from(value));
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_keys_hash_equal_and_differ_from_neighbours() {
        let hash = |key: &str| {
            let mut hasher = FxHasher::default();
            hasher.write(key.as_bytes());
            hasher.finish()
        };
        assert_eq!(hash("ppt/slides/slide1.xml"), hash("ppt/slides/slide1.xml"));
        assert_ne!(hash("ppt/slides/slide1.xml"), hash("ppt/slides/slide2.xml"));
        assert_ne!(hash("rId1"), hash("rId2"));
    }

    #[test]
    fn fast_map_round_trips() {
        let mut map: FastMap<&str, u32> = FastMap::default();
        map.insert("a", 1);
        map.insert("b", 2);
        assert_eq!(map.get("a"), Some(&1));
        assert_eq!(map.get("c"), None);
    }
}
