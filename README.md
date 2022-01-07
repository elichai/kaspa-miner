# Kaspa-miner
[![Build status](https://github.com/elichai/kaspa-miner/workflows/ci/badge.svg)](https://github.com/elichai/kaspa-miner/actions)
[![Latest version](https://img.shields.io/crates/v/kaspa-miner.svg)](https://crates.io/crates/kaspa-miner)
![License](https://img.shields.io/crates/l/kaspa-miner.svg)
[![dependency status](https://deps.rs/repo/github/elichai/kaspa-miner/status.svg)](https://deps.rs/repo/github/elichai/kaspa-miner)

A Rust binary for file encryption to multiple participants. 


## Installation
### From Sources
Install via `cargo install` not supported for latest version.

The regular version is still available at
```sh
cargo install kaspa-miner
```

### From Git Sources

If you are looking to build from the repository (for debug / extension), note that the plugins are additional
packages in the workspace. To compile a specific package, run the following command or any subset of it

```sh
cargo build --release -p kaspa-miner -p kaspacuda -p kaspaopencl
```


### From Binaries
The [release page](https://github.com/tmrlvi/kaspa-miner/releases) includes precompiled binaries for Linux, and Windows (for the GPU version).

### Removing Plugins
To remove a plugin simply remove the corresponding `dll`/`so` for the directory of the miner. 

* `libkaspacuda.so`, `libkaspacuda.dll`: Cuda support for Kaspa-Miner
* `libkaspaopencl.so`, `libkaspaopencl.dll`: OpenCL support for Kaspa-Miner

# Usage
To start mining you need to run [kaspad](https://github.com/kaspanet/kaspad) and have an address to send the rewards to.
There's a guide here on how to run a full node and how to generate addresses: https://github.com/kaspanet/docs/blob/main/Getting%20Started/Full%20Node%20Installation.md

Help:
```
kaspa-miner 
A Kaspa high performance CPU miner

USAGE:
    kaspa-miner [OPTIONS] --mining-address <MINING_ADDRESS>

OPTIONS:
    -a, --mining-address <MINING_ADDRESS>      The Kaspa address for the miner reward
        --cuda-device <CUDA_DEVICE>            Which CUDA GPUs to use [default: all]
        --cuda-disable                         
        --cuda-workload <CUDA_WORKLOAD>        Ratio of nonces to GPU possible parrallel run [defualt: 16]
        --cuda-workload-absolute               The values given by workload are not ratio, but absolute number of nonces [default: false]
    -d, --debug                                Enable debug logging level
        --devfund <DEVFUND_ADDRESS>            Mine a percentage of the blocks to the Kaspa devfund [default: kaspa:pzhh76qc82wzduvsrd9xh4zde9qhp0xc8rl7qu2mvl2e42uvdqt75zrcgpm00]
        --devfund-percent <DEVFUND_PERCENT>    The percentage of blocks to send to the devfund [default: 1]
    -h, --help                                 Print help information
        --mine-when-not-synced                 Mine even when kaspad says it is not synced, only useful when passing `--allow-submit-block-when-not-synced` to kaspad  [default: false]
        --opencl-device <OPENCL_DEVICE>        Which OpenCL GPUs to use on a specific platform
        --opencl-enable                        
        --opencl-platform <OPENCL_PLATFORM>    Which OpenCL platform to use (limited to one per executable)
        --opencl-workload <OPENCL_WORKLOAD>    Ratio of nonces to GPU possible parrallel run in OpenCL [defualt: 16]
        --opencl-workload-absolute             The values given by workload are not ratio, but absolute number of nonces in OpenCL [default: false]
    -p, --port <PORT>                          Kaspad port [default: Mainnet = 16111, Testnet = 16211]
    -s, --kaspad-address <KASPAD_ADDRESS>      The IP of the kaspad instance [default: 127.0.0.1]
    -t, --threads <NUM_THREADS>                Amount of miner threads to launch [default: number of logical cpus]
        --testnet                              Use testnet instead of mainnet [default: false]
```

To start mining you just need to run the following:

`./kaspa-miner --mining-address kaspa:XXXXX`

This will run the miner on all the available CPU cores.

# Devfund

The devfund is a fund managed by the Kaspa community in order to fund Kaspa development <br>
A miner that wants to mine a percentage into the dev-fund can pass the following flags: <br>
`kaspa-miner --mining-address= XXX --devfund=kaspa:precqv0krj3r6uyyfa36ga7s0u9jct0v4wg8ctsfde2gkrsgwgw8jgxfzfc98` <br>
and can pass `--devfund-precent=XX.YY` to mine only XX.YY% of the blocks into the devfund (passing `--devfund` without specifying a percent will default to 1%)

**This version automatically sets the devfund donation to the community designated address. 
To turn it off, run `--devfund-precent=0`**

# Donation Addresses

**Elichai**: `kaspa:qzvqtx5gkvl3tc54up6r8pk5mhuft9rtr0lvn624w9mtv4eqm9rvc9zfdmmpu`

**HauntedCook**: `kaspa:qz4jdyu04hv4hpyy00pl6trzw4gllnhnwy62xattejv2vaj5r0p5quvns058f`
