use crate::Hash;
use std::num::Wrapping;

pub(super) struct XoShiRo256PlusPlus {
    s0: Wrapping<u64>,
    s1: Wrapping<u64>,
    s2: Wrapping<u64>,
    s3: Wrapping<u64>,
}

impl XoShiRo256PlusPlus {
    #[inline]
    pub(super) fn new(hash: Hash) -> Self {
        Self { s0: Wrapping(hash.0[0]), s1: Wrapping(hash.0[1]), s2: Wrapping(hash.0[2]), s3: Wrapping(hash.0[3]) }
    }

    #[inline]
    pub(super) fn u64(&mut self) -> u64 {
        let res = self.s0 + Wrapping((self.s0 + self.s3).0.rotate_left(23));
        let t = self.s1 << 17;
        self.s2 ^= self.s0;
        self.s3 ^= self.s1;
        self.s1 ^= self.s2;
        self.s0 ^= self.s3;

        self.s2 ^= t;
        self.s3 = Wrapping(self.s3.0.rotate_left(45));

        res.0
    }
}
