# Cuda Support For Kaspa-Miner

## Building

The plugin is a shared library file that resides in the same library as the miner. 
You can build the library by running
```sh
cargo build -p kaspacuda
```

This version includes a precompiled PTX, which would work with most modern GPUs. To compile the PTX youself,
you have to clone the project:

```sh
git clone https://github.com/tmrlvi/kaspa-miner.git
cd kaspa-miner
# Using cuda nvcc that supports sm_30 (e.g., 9.2)
nvcc kaspa-cuda-native/src/kaspa-cuda.cu -std=c++11 -O3 --restrict --ptx --gpu-architecture=compute_30 --gpu-code=sm_30 -o ./resources/kaspa-cuda-sm30.ptx -Xptxas -O3 -Xcompiler -O3
# Using cuda nvcc from a recent cuda (e.g. 11.5)
nvcc kaspa-cuda-native/src/kaspa-cuda.cu -std=c++11 -O3 --restrict --ptx --gpu-architecture=compute_61 --gpu-code=sm_61 -o ./resources/kaspa-cuda-sm61.ptx -Xptxas -O3 -Xcompiler -O3 
cargo build --release
```
