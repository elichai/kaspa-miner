#include<stdint.h>
#include <assert.h>
#include "keccak-tiny.c"

#include <curand.h>
#include <curand_kernel.h>

typedef uint16_t MatrixRow[64];
typedef uint8_t Hash[32];
typedef uint64_t testme[64];

typedef union _uint256_t {
    uint64_t number[4];
    uint8_t hash[32];
} uint256_t;

#define BLOCKDIM 1024

#define LT_U256(X,Y) (X.number[3] != Y.number[3] ? X.number[3] < Y.number[3] : X.number[2] != Y.number[2] ? X.number[2] < Y.number[2] : X.number[1] != Y.number[1] ? X.number[1] < Y.number[1] : X.number[0] < Y.number[0])


__device__ __inline__ uint32_t amul4bit(uint32_t packed_vec1[32], uint32_t packed_vec2[32]) {
    // We assume each 32 bits have four values: A0 B0 C0 D0
    unsigned int res = 0;
    #pragma unroll
    for (int i=0; i<32; i++) {
        #if __CUDA_ARCH__ >= 610
        asm("dp4a.u32.u32" " %0, %1, %2, %3;": "=r" (res): "r" (packed_vec1[i]), "r" (packed_vec2[i]), "r" (res));
        #else
        char4 &a4 = *((char4*)&packed_vec1[i]);
        char4 &b4 = *((char4*)&packed_vec2[i]);
        res += a4.x*b4.x;
        //c += a4.y*b4.y; // In our code, the second and forth bytes are empty
        res += a4.z*b4.z;
        // c += a4.w*b4.w; // In our code, the second and forth bytes are empty
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

    __global__ void matrix_mul(const MatrixRow *rows, const uint64_t rows_len, const Hash *hashes, const uint64_t hashes_len, Hash *outs)
    {
        int rowId = threadIdx.x + blockIdx.x*blockDim.x;
        int hashId = threadIdx.y + blockIdx.y*blockDim.y;
        //assert((rowId != 0) || (hashId != 0) );

        if (rowId < rows_len/2 && hashId < hashes_len) {
            uint16_t packed_hash[64] = {0};
            #pragma unroll
            for (int i=0; i<32; i++) {
                packed_hash[2*i] = (uint16_t)((hashes[hashId][i] & 0xF0) >> 4 );
                packed_hash[2*i+1] = (uint16_t)((hashes[hashId][i] & 0x0F));
            }
            uint32_t product1 = amul4bit((uint32_t *)(rows[(2*rowId)]), (uint32_t *)(packed_hash)) >> 10;
            uint32_t product2 = amul4bit((uint32_t *)(rows[(2*rowId+1)]), (uint32_t *)(packed_hash)) >> 10;


            outs[hashId][rowId] = hashes[hashId][rowId] ^ ((uint8_t)(product1 << 4) | (uint8_t)(product2));
            }
    }

    __global__ void pow_cshake(const uint8_t *header, uint64_t *nonces, const uint64_t nonces_len, Hash *hashes, const bool generate, curandStateSobol64_t* states) {
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
            memcpy(input +  136, header, 72);
            // data
            // TODO: check endianity?
            memcpy(input +  208, (uint8_t *)(nonces + nonceId), 8);
            hash(hashes[nonceId], 32, input, 216, 136, 0x04);
        }
    }

    __global__ void heavy_hash_cshake(const uint64_t *nonces, const Hash *datas, const uint64_t data_len, uint64_t *final_nonces, Hash *hashes/*, Hash *all_hashes*/) {
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

            __shared__ uint256_t working_hashes[BLOCKDIM]; // Shared within the block
            __shared__ uint64_t working_nonces[BLOCKDIM];

            hash(working_hashes[threadIdx.x].hash, 32, input, 168, 136, 0x04);
            working_nonces[threadIdx.x] = nonces[dataId];

            //memcpy(all_hashes + dataId, working_hashes[threadIdx.x].hash, 32);
            __syncthreads();

            // Find the minimal hash - reduce step
            for (uint64_t size = blockDim.x/2; size>0; size/=2) {
                if (threadIdx.x<size) {
                    if (
                        (working_nonces[threadIdx.x+size] != 0) &&
                        (LT_U256(working_hashes[threadIdx.x+size], working_hashes[threadIdx.x]))
                        ){
                        //memcpy(working_hashes[threadIdx.x].number, datas[dataId], 32);
                        working_hashes[threadIdx.x] = working_hashes[threadIdx.x+size];
                        working_nonces[threadIdx.x] = working_nonces[threadIdx.x+size];
                    }
                }
                __syncthreads();
            }
            if (threadIdx.x == 0) {
                final_nonces[blockIdx.x] = working_nonces[0];
                //hashes[blockIdx.x] = working_hashes[0];
                memcpy(hashes + blockIdx.x, working_hashes[0].hash, 32);
            }
        }
    }
}