# OpenCL support for Kaspa-Miner

This is an experimental plugin to support opencl.

# Compiling to AMD
Download and install Radeon GPU Analyzer, which allows you to compile OpenCL for AMD

```shell
for arch in gfx906 gfx908 gfx1011 gfx1012 gfx1030 gfx1031 gfx1032
do 
  rga --O3 -s opencl -c "$arch" --OpenCLoption "-cl-finite-math-only -cl-mad-enable " -b plugins/opencl/resources/bin/kaspa-opencl.bin plugins/opencl/resources/kaspa-opencl.cl -D __FORCE_AMD_V_DOT8_U32_U4__=1 -D OPENCL_PLATFORM_AMD
done 

for arch in Ellesmere gfx1010
do 
  rga --O3 -s opencl -c "$arch" --OpenCLoption "-cl-finite-math-only -cl-mad-enable " -b plugins/opencl/resources/bin/kaspa-opencl.bin plugins/opencl/resources/kaspa-opencl.cl -D OPENCL_PLATFORM_AMD
done 
```