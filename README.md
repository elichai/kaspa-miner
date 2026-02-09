# Kaspa CPU Miner (Testnets)



## Installation

### From Binaries
The [release page](https://github.com/kaspanet/cpuminer/releases) includes precompiled binaries for Linux, macOS and Windows.


# Usage
To start mining you need to run [kaspad](https://github.com/kaspanet/rusty-kaspa) and have an address to send the rewards to.
See the Rusty Kaspa testnet docs for running a full node and generating addresses: https://github.com/kaspanet/rusty-kaspa/blob/master/docs/

Help:
```
kaspa-miner 0.2.1
A Kaspa high performance CPU miner

USAGE:
    kaspa-miner [FLAGS] [OPTIONS] --mining-address <mining-address>

FLAGS:
    -d, --debug                   Enable debug logging level
    -h, --help                    Prints help information
        --mine-when-not-synced    Mine even when kaspad says it is not synced, only useful when passing `--allow-submit-
                                  block-when-not-synced` to kaspad  [default: false]
        --testnet                 Use testnet instead of mainnet [default: false]
    -V, --version                 Prints version information

OPTIONS:
    -s, --kaspad-address <kaspad-address>      The IP of the kaspad instance [default: 127.0.0.1]
    -a, --mining-address <mining-address>      The Kaspa address for the miner reward
    -t, --threads <num-threads>                Amount of miner threads to launch [default: number of logical cpus]
    -p, --port <port>                          Kaspad port [default: Mainnet = 16111, Testnet = 16211]
```

To start mining you just need to run the following:

`./kaspa-miner --mining-address kaspa:XXXXX`

This will run the miner on all the available CPU cores.
