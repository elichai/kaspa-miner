use crate::Hash;
use blake2b_simd::State as Blake2bState;
use once_cell::sync::Lazy;
use sha3::digest::{ExtendableOutput, Update, XofReader};
use sha3::CShake256;

const PROOF_OF_WORK_DOMAIN: &[u8] = b"ProofOfWorkHash";
const HEAVY_HASH_DOMAIN: &[u8] = b"HeavyHash";
const BLOCK_HASH_DOMAIN: &[u8] = b"BlockHash";

#[derive(Clone)]
pub(super) struct PowHasher(CShake256);

#[derive(Clone)]
pub(super) struct HeavyHasher(CShake256);

#[derive(Clone)]
pub struct HeaderHasher(Blake2bState);

impl PowHasher {
    #[inline(always)]
    pub(super) fn new() -> Self {
        static POW_HASHER: Lazy<PowHasher> = Lazy::new(|| Self(CShake256::new(PROOF_OF_WORK_DOMAIN)));
        (*POW_HASHER).clone()
    }

    pub(super) fn write<A: AsRef<[u8]>>(&mut self, data: A) {
        self.0.update(data)
    }

    #[inline(always)]
    pub fn finalize(self) -> Hash {
        let mut out = [0u8; 32];
        self.0.finalize_xof().read(&mut out);
        Hash(out)
    }
}

impl HeavyHasher {
    #[inline(always)]
    pub(super) fn new() -> Self {
        static HEAVY_HASHER: Lazy<HeavyHasher> = Lazy::new(|| Self(CShake256::new(HEAVY_HASH_DOMAIN)));
        (*HEAVY_HASHER).clone()
    }

    pub(super) fn write<A: AsRef<[u8]>>(&mut self, data: A) {
        self.0.update(data)
    }

    #[inline(always)]
    pub fn finalize(self) -> Hash {
        let mut out = [0u8; 32];
        self.0.finalize_xof().read(&mut out);
        Hash(out)
    }
}

impl HeaderHasher {
    #[inline(always)]
    pub fn new() -> Self {
        Self(blake2b_simd::Params::new().hash_length(32).key(BLOCK_HASH_DOMAIN).to_state())
    }

    pub fn write<A: AsRef<[u8]>>(&mut self, data: A) {
        self.0.update(data.as_ref());
    }

    #[inline(always)]
    pub fn finalize(self) -> Hash {
        let mut out = [0u8; 32];
        out.copy_from_slice(self.0.finalize().as_bytes());
        Hash(out)
    }
}

pub trait Hasher {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self;
}

impl Hasher for PowHasher {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
        self.write(data);
        self
    }
}

impl Hasher for HeavyHasher {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
        self.write(data);
        self
    }
}

impl Hasher for HeaderHasher {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
        self.write(data);
        self
    }
}

#[cfg(all(test, feature = "bench"))]
mod benches {
    extern crate test;

    use self::test::{black_box, Bencher};
    use super::{PowHasher, HeavyHasher};
    use crate::Hash;
    use crate::pow::hasher::Hasher;

    #[bench]
    pub fn bench_pow_hash(bh: &mut Bencher) {
        let mut hasher = PowHasher::new();
        let timestamp: u64 = 5435345234;
        let mut nonce: u64 = 432432432;
        let pre_pow_hash = Hash([42; 32]);
        hasher.update(pre_pow_hash).update(timestamp.to_le_bytes()).update([0u8; 32]);

        bh.iter(|| {
            for _ in 0..10 {
                black_box(&mut hasher);
                black_box(&mut nonce);
                let mut hasher = hasher.clone();
                hasher.update(nonce.to_le_bytes());
                black_box(hasher.finalize());
            }
        });
    }

    #[bench]
    pub fn bench_heavy_hash(bh: &mut Bencher) {
        let mut data = [42; 32];
        bh.iter(|| {
            for _ in 0..10 {
                black_box(&mut data);
                let mut hasher = HeavyHasher::new();
                hasher.write(data);
                black_box(hasher.finalize());
            }
        });
    }
}
