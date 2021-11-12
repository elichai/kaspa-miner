use crate::Hash;
use once_cell::sync::Lazy;
use sha3::digest::{ExtendableOutput, Update, XofReader};
use sha3::CShake256;

const PROOF_OF_WORK_DOMAIN: &[u8] = b"ProofOfWorkHash";
const HEAVY_HASH_DOMAIN: &[u8] = b"HeavyHash";

#[derive(Clone)]
pub(super) struct PowHasher(CShake256);

#[derive(Clone)]
pub(super) struct HeavyHasher(CShake256);

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
