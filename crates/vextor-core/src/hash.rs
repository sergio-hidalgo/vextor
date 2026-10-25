//! FNV-1a 64-bit hashing (spec 14): content hashes that stay the same
//! across Rust versions and runs, for shared images in `.vxtr` files and
//! for the app's save checks and backups. Not cryptographic: equal hashes
//! are confirmed byte for byte where it matters.

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// A running FNV-1a 64 hash; feed it with [`Fnv1a::update`].
#[derive(Clone, Copy, Debug)]
pub struct Fnv1a(u64);

impl Default for Fnv1a {
    fn default() -> Self {
        Self(OFFSET)
    }
}

impl Fnv1a {
    /// Adds `bytes` to the hash.
    pub fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    /// The hash of everything added so far.
    #[must_use]
    pub fn finish(self) -> u64 {
        self.0
    }
}

/// The FNV-1a 64 hash of `bytes`.
#[must_use]
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = Fnv1a::default();
    h.update(bytes);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_test_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn hashing_in_pieces_equals_hashing_at_once() {
        let mut h = Fnv1a::default();
        h.update(b"foo");
        h.update(b"bar");
        assert_eq!(h.finish(), fnv1a(b"foobar"));
    }
}
