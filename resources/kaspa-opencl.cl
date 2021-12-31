/* TYPES */

typedef uchar uint8_t;
typedef char int8_t;
typedef ushort uint16_t;
typedef short int16_t;
typedef uint uint32_t;
typedef int int32_t;
typedef ulong uint64_t;
typedef long int64_t;

/* TINY KECCAK */
/** libkeccak-tiny
 *
 * A single-file implementation of SHA-3 and SHAKE.
 *
 * Implementor: David Leon Gil
 * License: CC0, attribution kindly requested. Blame taken too,
 * but not liability.
 */

/******** The Keccak-f[1600] permutation ********/

/*** Constants. ***/
constant static const uint8_t rho[24] = \
  { 1,  3,   6, 10, 15, 21,
    28, 36, 45, 55,  2, 14,
    27, 41, 56,  8, 25, 43,
    62, 18, 39, 61, 20, 44};
constant static const uint8_t pi[24] = \
  {10,  7, 11, 17, 18, 3,
    5, 16,  8, 21, 24, 4,
   15, 23, 19, 13, 12, 2,
   20, 14, 22,  9, 6,  1};

constant static const uint64_t RC[24] = \
  {1UL, 0x8082UL, 0x800000000000808aUL, 0x8000000080008000UL,
   0x808bUL, 0x80000001UL, 0x8000000080008081UL, 0x8000000000008009UL,
   0x8aUL, 0x88UL, 0x80008009UL, 0x8000000aUL,
   0x8000808bUL, 0x800000000000008bUL, 0x8000000000008089UL, 0x8000000000008003UL,
   0x8000000000008002UL, 0x8000000000000080UL, 0x800aUL, 0x800000008000000aUL,
   0x8000000080008081UL, 0x8000000000008080UL, 0x80000001UL, 0x8000000080008008UL};


/*** Helper macros to unroll the permutation. ***/
#define rol(x, s) (((x) << s) | ((x) >> (64 - s)))
#define REPEAT6(e) e e e e e e
#define REPEAT24(e) REPEAT6(e e e e)
#define REPEAT5(e) e e e e e
#define FOR5(v, s, e) \
  v = 0;            \
  REPEAT5(e; v += s;)

/*** Keccak-f[1600] ***/
static inline void keccakf(void* state) {
  uint64_t* a = (uint64_t*)state;
  uint64_t b[5] = {0};
  uint64_t t = 0;
  uint8_t x, y;

  //#pragma unroll
  for (int i = 0; i < 24; i++) {
    // Theta
    FOR5(x, 1,
         b[x] = 0;
         FOR5(y, 5,
              b[x] ^= a[x + y]; ))
    FOR5(x, 1,
         FOR5(y, 5,
              a[y + x] ^= b[(x + 4) % 5] ^ rol(b[(x + 1) % 5], 1); ))
    // Rho and pi
    t = a[1];
    x = 0;
    REPEAT24(b[0] = a[pi[x]];
             a[pi[x]] = rol(t, rho[x]);
             t = b[0];
             x++; )
    // Chi
    FOR5(y,
       5,
       FOR5(x, 1,
            b[x] = a[y + x];)
       FOR5(x, 1,
            a[y + x] = b[x] ^ ((~b[(x + 1) % 5]) & b[(x + 2) % 5]); ))
    // Iota
    a[0] ^= RC[i];
  }
}

/******** The FIPS202-defined functions. ********/

/*** Some helper macros. ***/

#define _(S) do { S } while (0)
#define FOR(i, ST, L, S) \
  _(for (size_t i = 0; i < L; i += ST) { S; })
#define mkapply_ds(NAME, S)                                          \
  static inline void NAME(uint8_t* dst,                              \
                          const uint8_t* src,                        \
                          size_t len) {                              \
    FOR(i, 1, len, S);                                               \
  }
#define mkapply_sd(NAME, S)                                          \
  static inline void NAME(const uint8_t* src,                        \
                          uint8_t* dst,                              \
                          size_t len) {                              \
    FOR(i, 1, len, S);                                               \
  }

mkapply_ds(xorin, dst[i] ^= src[i])  // xorin
mkapply_sd(setout, dst[i] = src[i])  // setout

#define P keccakf
#define Plen 200

// Fold P*F over the full blocks of an input.
#define foldP(I, L, F) \
  while (L >= rate) {  \
    F(a, I, rate);     \
    P(a);              \
    I += rate;         \
    L -= rate;         \
  }

/** The sponge-based hash construction. **/
inline static int hash(uint8_t* out, size_t outlen,
                       const uint8_t* in, size_t inlen,
                       size_t rate, uint8_t delim) {
  if ((out == NULL) || ((in == NULL) && inlen != 0) || (rate >= Plen)) {
    return -1;
  }
  uint8_t a[Plen] = {0};
  // Absorb input.
  foldP(in, inlen, xorin);
  // Xor in the DS and pad frame.
  a[inlen] ^= delim;
  a[rate - 1] ^= 0x80;
  // Xor in the last block.
  xorin(a, in, inlen);
  // Apply P
  P(a);
  // Squeeze output.
  foldP(out, outlen, setout);
  setout(a, out, outlen);
  return 0;
}

/* RANDOM NUMBER GENERATOR BASED ON MWC64X                          */
/* http://cas.ee.ic.ac.uk/people/dt10/research/rngs-gpu-mwc64x.html */

inline static ulong MWC128X(ulong2 *state)
{
    enum { A=18446744073709550874UL };
    ulong x=(*state).x, c=(*state).y;  // Unpack the state
    ulong res=x^c;                     // Calculate the result
    ulong hi=mul_hi(x,A);              // Step the RNG
    x=x*A+c;
    c=hi+(x<c);
    *state=(ulong2)(x,c);             // Pack the state back up
    return res;                       // Return the next result
}

/* KERNEL CODE */

#pragma OPENCL EXTENSION cl_khr_int64_base_atomics: enable

typedef uint8_t Hash[32];
typedef uint64_t uint256_t[4];

/*typedef union _uint256_t {
    uint64_t number[4];
    uint8_t hash[32];
} uint256_t;*/

#define BLOCKDIM 1024
#define MATRIX_SIZE 64
#define HALF_MATRIX_SIZE 32
#define QUARTER_MATRIX_SIZE 16
#define HASH_HEADER_SIZE 72

#define LT_U256(X,Y) (X[3] != Y[3] ? X[3] < Y[3] : X[2] != Y[2] ? X[2] < Y[2] : X[1] != Y[1] ? X[1] < Y[1] : X[0] < Y[0])

static constant uint8_t pow_header[216] = {
    0x01, 0x88, // left_encode(136)                  - cSHAKE256 specific
    0x01, 0x00, // left_encode(0)                    - No Domain
    0x01, 0x78, // left_encode customization string length
    0x50, 0x72, 0x6f, 0x6f, 0x66, 0x4f, 0x66, 0x57, 0x6f, 0x72, 0x6b, 0x48, 0x61, 0x73, 0x68, // ProofOfWorkHash
};

static constant uint8_t heavy_header[168] = {
    0x01, 0x88, // left_encode(136)                  - cSHAKE256 specific
    0x01, 0x00, // left_encode(0)                    - No Domain
    0x01, 0x48, // left_encode customization string length
    0x48, 0x65, 0x61, 0x76, 0x79, 0x48, 0x61, 0x73, 0x68, //HeavyHash
    // the rest is zeros
};


// kernel void init(global void *seeds,  global void* states, global const uint64_t state_count) {
// }

kernel void heavy_hash(
    global const uint8_t read_only hash_header[HASH_HEADER_SIZE],
    global const uint8_t read_only matrix[MATRIX_SIZE][MATRIX_SIZE],
    global const uint256_t read_only target,
    global ulong2 *random_state,
    global uint64_t write_only *final_nonce,
    global uint64_t write_only *final_hash
) {
    uint8_t buffer[216];
    int nonceId = get_global_id(0);

    private uint64_t nonce = MWC128X(random_state + nonceId);

    for(int i=0; i<216; i++) buffer[i] = pow_header[i];
    // header
    for(int i=0; i<HASH_HEADER_SIZE; i++) buffer[136+i] = hash_header[i];
    // data
    for(int i=0; i<8; i++) buffer[208+i] = ((uint8_t *)&nonce)[i];

    Hash hash_;
    hash(hash_, 32, buffer, 216, 136, 0x04);

    uchar16 hash_part[4];
    for (int i=0; i<4; i++) {
         hash_part[i] = (uchar16)(
            (hash_[8*i] & 0xF0) >> 4,
            (hash_[8*i] & 0x0F),
            (hash_[8*i+1] & 0xF0) >> 4,
            (hash_[8*i+1] & 0x0F),
            (hash_[8*i+2] & 0xF0) >> 4,
            (hash_[8*i+2] & 0x0F),
            (hash_[8*i+3] & 0xF0) >> 4,
            (hash_[8*i+3] & 0x0F),
            (hash_[8*i+4] & 0xF0) >> 4,
            (hash_[8*i+4] & 0x0F),
            (hash_[8*i+5] & 0xF0) >> 4,
            (hash_[8*i+5] & 0x0F),
            (hash_[8*i+6] & 0xF0) >> 4,
            (hash_[8*i+6] & 0x0F),
            (hash_[8*i+7] & 0xF0) >> 4,
            (hash_[8*i+7] & 0x0F)
        );
    }

    for (int rowId=0; rowId<32; rowId++){
        ushort16 product1 = 0;
        ushort16 product2 = 0;
        for (int i=0; i<4; i++) {
            product1 += convert_ushort16(vload16(i, matrix[(2*rowId)])*hash_part[i]);
            product2 += convert_ushort16(vload16(i, matrix[(2*rowId+1)])*hash_part[i]);
        }
        product1.s01234567 = product1.s01234567 + product1.s89abcdef;
        product1.s0123 = product1.s0123 + product1.s4567;
        product1.s01 = product1.s01 + product1.s23;
        product1.s0 = product1.s0 + product1.s1;

        product2.s01234567 = product2.s01234567 + product2.s89abcdef;
        product2.s0123 = product2.s0123 + product2.s4567;
        product2.s01 = product2.s01 + product2.s23;
        product2.s0 = product2.s0 + product2.s1;

        product1.s0 >>= 10;
        product2.s0 >>= 10;
        hash_[rowId] = hash_[rowId] ^ ((uint8_t)(product1.s0 << 4) | (uint8_t)(product2.s0));
    }

    for(int i=0; i<168; i++) buffer[i] = heavy_header[i];
    // data
    for(int i=0; i<32; i++) buffer[136+i] = hash_[i];

    hash(hash_, 32, buffer, 168, 136, 0x04);
    if (LT_U256(((uint64_t *)hash_), target)){
        //printf("%lu: %lu < %lu: %d %d\n", nonce, ((uint64_t *)hash_)[3], target[3], ((uint64_t *)hash_)[3] < target[3], LT_U256((uint64_t *)hash_, target));
        atomic_cmpxchg(final_nonce, 0, nonce);
    }
    /*if (nonceId==1) {
        printf("%lu: %lu < %lu: %d %d\n", nonce, ((uint64_t *)hash_)[3], target[3], ((uint64_t *)hash_)[3] < target[3]);
        atomic_cmpxchg(final_nonce, 0, nonce);
        for(int i=0;i<4;i++) final_hash[i] = ((uint64_t *)hash_)[i];
    }*/
}

