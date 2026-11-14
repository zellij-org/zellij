use super::*;

impl ApcFilter {
    pub fn params(&self) -> &[u8] {
        &self.params
    }
}

impl Image {
    pub fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in &self.pixels {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}
