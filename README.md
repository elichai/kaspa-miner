# Kaspa-miner
[![Build status](https://github.com/elichai/kaspa-miner/workflows/ci/badge.svg)](https://github.com/elichai/kaspa-miner/actions)
[![Latest version](https://img.shields.io/crates/v/kaspa-miner.svg)](https://crates.io/crates/kaspa-miner)
![License](https://img.shields.io/crates/l/kaspa-miner.svg)
[![dependency status](https://deps.rs/repo/github/elichai/kaspa-miner/status.svg)](https://deps.rs/repo/github/elichai/kaspa-miner)

A Rust binary for file encryption to multiple participants. 


## Installation
### From Sources
With Rust's package manager cargo, you can install kaspa-miner via:

```sh
cargo install kaspa-miner
```

### From Binaries
The [release page](https://github.com/elichai/kaspa-miner/releases) includes precompiled binaries for Linux, macOS and Windows.


# Usage
To start mining you need to run [kaspad](https://github.com/kaspanet/kaspad) and have an address to send the rewards to.
There's a guide here on how to run a full node and how to generate addresses: https://github.com/kaspanet/docs/blob/main/Getting%20Started/Full%20Node%20Installation.md

Help:
```
kaspa-miner 0.1.0
A Kaspa high performance CPU miner

USAGE:
    kaspa-miner [FLAGS] [OPTIONS] --mining-address <mining-address>

FLAGS:
    -d, --debug      Enable debug logging level
    -h, --help       Prints help information
        --testnet    Use testnet instead of mainnet [default: false]
    -V, --version    Prints version information

OPTIONS:
    -s, --kaspad-address <kaspad-address>    The IP of the kaspad instance [default: 127.0.0.1]
    -a, --mining-address <mining-address>    The Kaspa address for the miner reward
    -t, --threads <num-threads>              Amount of miner threads to launch [default: number of logical cpus]
                                             [default: 0]
    -p, --port <port>                        Kaspad port [default: Mainnet = 16111, Testnet = 16211]
```

To start mining you just need to run the following:

`./kaspa-miner --mining-addr kaspa:XXXXX`

This will run the miner on all the available CPU cores.