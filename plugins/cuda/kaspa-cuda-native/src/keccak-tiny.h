#ifndef KECCAK_FIPS202_H
#define KECCAK_FIPS202_H
#define __STDC_WANT_LIB_EXT1__ 1
#include <stdint.h>
#include <stdlib.h>

#define decshake(bits) \
  __device__ int shake##bits(uint8_t*, size_t, const uint8_t*, size_t);

#define decsha3(bits) \
  __device__ int sha3_##bits(uint8_t*, size_t, const uint8_t*, size_t);

decshake(128)
decshake(256)
decsha3(224)
decsha3(256)
decsha3(384)
decsha3(512)
#endif
