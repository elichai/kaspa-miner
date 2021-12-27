use crate::Hash;
use blake2b_simd::State as Blake2bState;

const BLOCK_HASH_DOMAIN: &[u8] = b"BlockHash";

#[derive(Clone, Copy)]
pub(super) struct PowHasher([u64; 25]);

#[derive(Clone, Copy)]
pub(super) struct HeavyHasher;

#[derive(Clone)]
pub struct HeaderHasher(Blake2bState);

impl PowHasher {
    // The initial state of `cSHAKE256("ProofOfWorkHash")`
    // [10] -> 1123092876221303310 ^ 0x04(padding byte) = 1123092876221303306
    // [16] -> 10306167911662716186 ^ 0x8000000000000000(final padding) = 1082795874807940378
    #[rustfmt::skip]
    const INITIAL_STATE: [u64; 25] = [
        1242148031264380989, 3008272977830772284, 2188519011337848018, 1992179434288343456, 8876506674959887717,
        5399642050693751366, 1745875063082670864, 8605242046444978844, 17936695144567157056, 3343109343542796272,
        1123092876221303306, 4963925045340115282, 17037383077651887893, 16629644495023626889, 12833675776649114147,
        3784524041015224902, 1082795874807940378, 13952716920571277634, 13411128033953605860, 15060696040649351053,
        9928834659948351306, 5237849264682708699, 12825353012139217522, 6706187291358897596, 196324915476054915,
    ];
    #[inline(always)]
    pub(super) fn new(pre_pow_hash: Hash, timestamp: u64) -> Self {
        let mut start = Self::INITIAL_STATE;
        for (&pre_pow_word, state_word) in pre_pow_hash.0.iter().zip(start.iter_mut()) {
            *state_word ^= pre_pow_word;
        }
        start[4] ^= timestamp;
        Self(start)
    }

    #[inline(always)]
    pub(super) fn finalize_with_nonce(mut self, nonce: u64) -> Hash {
        self.0[9] ^= nonce;
        super::keccak::f1600(&mut self.0);
        Hash::new(self.0[..4].try_into().unwrap())
    }
}

impl HeavyHasher {
    // The initial state of `cSHAKE256("ProofOfWorkHash")`
    // [4] -> 16654558671554924254 ^ 0x04(padding byte) = 16654558671554924250
    // [16] -> 9793466274154320918 ^ 0x8000000000000000(final padding) = 570094237299545110
    #[rustfmt::skip]
    const INITIAL_STATE: [u64; 25] = [
        4239941492252378377, 8746723911537738262, 8796936657246353646, 1272090201925444760, 16654558671554924250,
        8270816933120786537, 13907396207649043898, 6782861118970774626, 9239690602118867528, 11582319943599406348,
        17596056728278508070, 15212962468105129023, 7812475424661425213, 3370482334374859748, 5690099369266491460,
        8596393687355028144, 570094237299545110, 9119540418498120711, 16901969272480492857, 13372017233735502424,
        14372891883993151831, 5171152063242093102, 10573107899694386186, 6096431547456407061, 1592359455985097269,
    ];
    #[inline(always)]
    pub(super) fn hash(in_hash: Hash) -> Hash {
        let mut state = Self::INITIAL_STATE;
        for (&pre_pow_word, state_word) in in_hash.0.iter().zip(state.iter_mut()) {
            *state_word ^= pre_pow_word;
        }
        super::keccak::f1600(&mut state);
        Hash::new(state[..4].try_into().unwrap())
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
        Hash::from_le_bytes(self.0.finalize().as_bytes().try_into().expect("this is 32 bytes"))
    }
}

pub trait Hasher {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self;
}

impl Hasher for HeaderHasher {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
        self.write(data);
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::pow::hasher::{HeavyHasher, PowHasher};
    use crate::Hash;
    use sha3::digest::{ExtendableOutput, Update, XofReader};
    use sha3::CShake256;

    const PROOF_OF_WORK_DOMAIN: &[u8] = b"ProofOfWorkHash";
    const HEAVY_HASH_DOMAIN: &[u8] = b"HeavyHash";

    #[test]
    fn test_pow_hash() {
        let timestamp: u64 = 5435345234;
        let nonce: u64 = 432432432;
        let pre_pow_hash = Hash::from_le_bytes([42; 32]);
        let hasher = PowHasher::new(pre_pow_hash, timestamp);
        let hash1 = hasher.finalize_with_nonce(nonce);

        let hasher = CShake256::new(PROOF_OF_WORK_DOMAIN)
            .chain(pre_pow_hash.to_le_bytes())
            .chain(timestamp.to_le_bytes())
            .chain([0u8; 32])
            .chain(nonce.to_le_bytes());
        let mut hash2 = [0u8; 32];
        hasher.finalize_xof().read(&mut hash2);
        assert_eq!(Hash::from_le_bytes(hash2), hash1);
    }

    #[test]
    fn test_heavy_hash() {
        let val = Hash::from_le_bytes([42; 32]);
        let hash1 = HeavyHasher::hash(val);

        let hasher = CShake256::new(HEAVY_HASH_DOMAIN).chain(val.to_le_bytes());
        let mut hash2 = [0u8; 32];
        hasher.finalize_xof().read(&mut hash2);
        assert_eq!(Hash::from_le_bytes(hash2), hash1);
    }
}

#[cfg(all(test, feature = "bench"))]
mod benches {
    extern crate test;

    use self::test::{black_box, Bencher};
    use super::{HeavyHasher, PowHasher};
    use crate::Hash;

    #[bench]
    pub fn bench_pow_hash(bh: &mut Bencher) {
        let timestamp: u64 = 5435345234;
        let mut nonce: u64 = 432432432;
        let pre_pow_hash = Hash::from_le_bytes([42; 32]);
        let mut hasher = PowHasher::new(pre_pow_hash, timestamp);

        bh.iter(|| {
            for _ in 0..100 {
                black_box(&mut hasher);
                black_box(&mut nonce);
                black_box(hasher.finalize_with_nonce(nonce));
            }
        });
    }

    #[bench]
    pub fn bench_heavy_hash(bh: &mut Bencher) {
        let mut data = Hash::from_le_bytes([42; 32]);
        bh.iter(|| {
            for _ in 0..100 {
                black_box(&mut data);
                black_box(HeavyHasher::hash(data));
            }
        });
    }
}
