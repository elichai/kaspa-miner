#include<stdint.h>
#include <assert.h>
#include "keccak-tiny.c"

#include <curand.h>
#include <curand_kernel.h>

typedef uint8_t Hash[32];

typedef union _uint256_t {
    uint64_t number[4];
    uint8_t hash[32];
} uint256_t;

#define BLOCKDIM 1024
#define MATRIX_SIZE 64
#define HALF_MATRIX_SIZE 32
#define QUARTER_MATRIX_SIZE 16
#define HASH_HEADER_SIZE 72

#define LT_U256(X,Y) (X.number[3] != Y.number[3] ? X.number[3] < Y.number[3] : X.number[2] != Y.number[2] ? X.number[2] < Y.number[2] : X.number[1] != Y.number[1] ? X.number[1] < Y.number[1] : X.number[0] < Y.number[0])

__constant__ uint8_t matrix[MATRIX_SIZE][MATRIX_SIZE];
__constant__ uint8_t hash_header[HASH_HEADER_SIZE];
__constant__ uint256_t target;


__device__ __inline__ uint32_t amul4bit(uint32_t packed_vec1[32], uint32_t packed_vec2[32]) {
    // We assume each 32 bits have four values: A0 B0 C0 D0
    unsigned int res = 0;
    #pragma unroll
    for (int i=0; i<QUARTER_MATRIX_SIZE; i++) {
        #if __CUDA_ARCH__ >= 610
        asm("dp4a.u32.u32" " %0, %1, %2, %3;": "=r" (res): "r" (packed_vec1[i]), "r" (packed_vec2[i]), "r" (res));
        #else
        char4 &a4 = *((char4*)&packed_vec1[i]);
        char4 &b4 = *((char4*)&packed_vec2[i]);
        res += a4.x*b4.x;
        res += a4.y*b4.y; // In our code, the second and forth bytes are empty
        res += a4.z*b4.z;
        res += a4.w*b4.w; // In our code, the second and forth bytes are empty
        #endif
    }

    return res;
}


extern "C" {
    //curandDirectionVectors64_t is uint64_t[64]
    __global__ void init(curandDirectionVectors64_t *seeds,  curandStateSobol64_t* states, const uint64_t state_count) {
        uint64_t workerId = threadIdx.x + blockIdx.x*blockDim.x;
        if (workerId < state_count) {
            curand_init(seeds[workerId], 0, states + workerId);
            curand(states + workerId);
        }
    }

    __global__ void matrix_mul(const Hash *hashes, const uint64_t hashes_len, Hash *outs)
    {
        int rowId = threadIdx.x + blockIdx.x*blockDim.x;
        int hashId = threadIdx.y + blockIdx.y*blockDim.y;
        //assert((rowId != 0) || (hashId != 0) );

        if (rowId < HALF_MATRIX_SIZE && hashId < hashes_len) {
            uchar4 packed_hash[QUARTER_MATRIX_SIZE] = {0};
            #pragma unroll
            for (int i=0; i<QUARTER_MATRIX_SIZE; i++) {
                packed_hash[i] = make_uchar4(
                    (hashes[hashId][2*i] & 0xF0) >> 4 ,
                    (hashes[hashId][2*i] & 0x0F),
                    (hashes[hashId][2*i+1] & 0xF0) >> 4,
                    (hashes[hashId][2*i+1] & 0x0F)
                );
            }
            uint32_t product1 = amul4bit((uint32_t *)(matrix[(2*rowId)]), (uint32_t *)(packed_hash)) >> 10;
            uint32_t product2 = amul4bit((uint32_t *)(matrix[(2*rowId+1)]), (uint32_t *)(packed_hash)) >> 10;


            outs[hashId][rowId] = hashes[hashId][rowId] ^ ((uint8_t)(product1 << 4) | (uint8_t)(product2));
            }
    }

    __global__ void pow_cshake(uint64_t *nonces, const uint64_t nonces_len, Hash *hashes, const bool generate, curandStateSobol64_t* states) {
        // assuming header_len is 72
        int nonceId = threadIdx.x + blockIdx.x*blockDim.x;
        if (nonceId < nonces_len) {
            if (generate) nonces[nonceId] = curand(states + nonceId);
            uint8_t input[216] = {
                0x01, 0x88, // left_encode(136)                  - cSHAKE256 specific
                0x01, 0x00, // left_encode(0)                    - No Domain
                0x01, 0x78, // left_encode customization string length
                0x50, 0x72, 0x6f, 0x6f, 0x66, 0x4f, 0x66, 0x57, 0x6f, 0x72, 0x6b, 0x48, 0x61, 0x73, 0x68, // ProofOfWorkHash
            };
            // header
            memcpy(input +  136, hash_header, HASH_HEADER_SIZE);
            // data
            // TODO: check endianity?
            memcpy(input +  208, (uint8_t *)(nonces + nonceId), 8);
            hash(hashes[nonceId], 32, input, 216, 136, 0x04);
        }
    }

    __global__ void heavy_hash_cshake(const uint64_t *nonces, const Hash *datas, const uint64_t data_len, uint64_t *final_nonce/*, Hash *all_hashes*/) {
        assert(blockDim.x <= BLOCKDIM);
        uint64_t dataId = threadIdx.x + blockIdx.x*blockDim.x;
        if (dataId < data_len) {
            uint8_t input[168] = {
                0x01, 0x88, // left_encode(136)                  - cSHAKE256 specific
                0x01, 0x00, // left_encode(0)                    - No Domain
                0x01, 0x48, // left_encode customization string length
                0x48, 0x65, 0x61, 0x76, 0x79, 0x48, 0x61, 0x73, 0x68, //HeavyHash
                // the rest is zeros
            };
            // data
            memcpy(input +  136, datas[dataId], 32);

            uint256_t working_hash;
            hash(working_hash.hash, 32, input, 168, 136, 0x04);
            if (LT_U256(working_hash, target)){
                atomicCAS((unsigned long long int*) final_nonce, 0, (unsigned long long int) nonces[dataId]);
            }
        }
    }
}