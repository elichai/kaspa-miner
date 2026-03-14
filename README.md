# Kaspa CPU Miner (Testnets)

## Installation

### From Binaries
The [release page](https://github.com/kaspanet/cpuminer/releases) includes precompiled binaries for Linux, macOS and Windows.

# Usage
To start mining you need to run [kaspad](https://github.com/kaspanet/rusty-kaspa) and have an address to send the rewards to.
See the Rusty Kaspa testnet docs for running a full node and generating addresses: https://github.com/kaspanet/rusty-kaspa/blob/master/docs/

### Help:
```
A Kaspa high performance CPU miner

Usage: kaspa-miner [OPTIONS] --mining-address <MINING_ADDRESS>

Options:
  -a, --mining-address <MINING_ADDRESS>
          The Kaspa address for the miner reward
  -s, --kaspad-address <KASPAD_ADDRESS>
          The IP of the kaspad instance [default: 127.0.0.1]
  -p, --port <PORT>
          Kaspad port [default: Mainnet = 16110, Testnet = 16210]
  -d, --debug
          Enable debug logging level
      --testnet
          Use testnet instead of mainnet [default: false]
  -t, --threads <NUM_THREADS>
          Amount of miner threads to launch [default: number of logical cpus]
      --devfund <DEVFUND_ADDRESS>
          Mine a percentage of the blocks to the Kaspa devfund [default: Off]
      --devfund-percent <DEVFUND_PERCENT>
          The percentage of blocks to send to the devfund [default: 1]
      --mine-when-not-synced
          Mine even when kaspad says it is not synced, only useful when passing `--allow-submit-block-when-not-synced` to kaspad  [default: false]
      --throttle <THROTTLE>
          Throttle (milliseconds) between each pow hash generation (used for development testing)
      --altlogs
          Output logs in alternative format (same as kaspad)
  -h, --help
          Print help
  -V, --version
          Print version
```

### Running

`./kaspa-miner --testnet --mining-address kaspa:XXXXX`

This will run the miner on all the available CPU cores. Requires a testnet Kaspad on localhost.

### Docker

`docker run --rm kaspanet/cpuminer --testnet -s 123.123.123.123 -a kaspa:XXXXX`

Supply a valid testnet node with an open GRPC port to the -s parameter.

### Docker Compose

Create docker-compose.yaml:
```yaml
services:

  kaspa_miner_testnet_10:
    container_name: kaspa_miner_testnet_10
    image: kaspanet/cpuminer
    restart: unless-stopped
    cpus: 0.1 # Increase if necessary, remove to use all cores
    command: --testnet -s 123.123.123.123 -a kaspa:XXXXX

  kaspa_miner_testnet_12:
    container_name: kaspa_miner_testnet_12
    image: kaspanet/cpuminer
    restart: unless-stopped
    cpus: 0.1 # Increase if necessary, remove to use all cores
    command: --testnet -s 321.321.321.321 -a kaspa:XXXXX
```

Run in same directory:
`docker compose up -d`
