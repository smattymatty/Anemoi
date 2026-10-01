//! A state hash that agrees across platforms: FNV-1a 64 over little-endian,
//! fixed-width integers. `std`'s `DefaultHasher` promises neither.

use std::hash::Hasher;

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Debug)]
pub struct StableHasher(u64);

impl Default for StableHasher {
    fn default() -> Self {
        Self(OFFSET)
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(PRIME);
        }
    }

    fn write_u16(&mut self, n: u16) {
        self.write(&n.to_le_bytes());
    }

    fn write_u32(&mut self, n: u32) {
        self.write(&n.to_le_bytes());
    }

    fn write_u64(&mut self, n: u64) {
        self.write(&n.to_le_bytes());
    }

    fn write_u128(&mut self, n: u128) {
        self.write(&n.to_le_bytes());
    }

    /// Lengths and enum tags are `usize`/`isize`: 8 bytes on every target.
    fn write_usize(&mut self, n: usize) {
        self.write_u64(n as u64);
    }

    fn write_isize(&mut self, n: isize) {
        self.write_i64(n as i64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv(bytes: &[u8]) -> u64 {
        let mut h = StableHasher::default();
        h.write(bytes);
        h.finish()
    }

    /// The published FNV-1a 64 test vectors.
    #[test]
    fn matches_the_reference_vectors() {
        assert_eq!(fnv(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn integers_hash_as_fixed_width_little_endian() {
        let hashed = |f: &dyn Fn(&mut StableHasher)| {
            let mut h = StableHasher::default();
            f(&mut h);
            h.finish()
        };
        let bytes = 0x0102_0304_0506_0708u64.to_le_bytes();
        assert_eq!(hashed(&|h| h.write_u64(0x0102_0304_0506_0708)), fnv(&bytes));
        assert_eq!(hashed(&|h| h.write_u32(0x0506_0708)), fnv(&bytes[..4]));
        assert_eq!(hashed(&|h| h.write_u16(0x0708)), fnv(&bytes[..2]));
        assert_eq!(hashed(&|h| h.write_usize(7)), fnv(&7u64.to_le_bytes()));
        assert_eq!(hashed(&|h| h.write_isize(-1)), fnv(&[0xff; 8]));
        let wide = 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10u128;
        assert_eq!(hashed(&|h| h.write_u128(wide)), fnv(&wide.to_le_bytes()));
    }
}
