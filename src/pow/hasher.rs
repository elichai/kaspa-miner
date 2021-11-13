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
pub(super) struct HeaderHasher(Blake2bState);

impl PowHasher {
    #[inline]
    pub(super) fn new() -> Self {
        static POW_HASHER: Lazy<PowHasher> =
            Lazy::new(|| Self(CShake256::new(PROOF_OF_WORK_DOMAIN)));
        (*POW_HASHER).clone()
    }

    pub(super) fn write<A: AsRef<[u8]>>(&mut self, data: A) {
        self.0.update(data)
    }

    #[inline]
    pub fn finalize(self) -> Hash {
        let mut out = [0u8; 32];
        self.0.finalize_xof().read(&mut out);
        out
    }
}

impl HeavyHasher {
    #[inline]
    pub(super) fn new() -> Self {
        static HEAVY_HASHER: Lazy<HeavyHasher> =
            Lazy::new(|| Self(CShake256::new(HEAVY_HASH_DOMAIN)));
        (*HEAVY_HASHER).clone()
    }

    pub(super) fn write<A: AsRef<[u8]>>(&mut self, data: A) {
        self.0.update(data)
    }

    #[inline]
    pub fn finalize(self) -> Hash {
        let mut out = [0u8; 32];
        self.0.finalize_xof().read(&mut out);
        out
    }
}

impl HeaderHasher {
    #[inline]
    pub(super) fn new() -> Self {
        Self(
            blake2b_simd::Params::new()
                .hash_length(32)
                .key(BLOCK_HASH_DOMAIN)
                .to_state(),
        )
    }

    pub(super) fn write<A: AsRef<[u8]>>(&mut self, data: A) {
        self.0.update(data.as_ref());
    }

    #[inline]
    pub fn finalize(self) -> Hash {
        let mut out = [0u8; 32];
        out.copy_from_slice(self.0.finalize().as_bytes());
        out
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
