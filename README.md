# Bitcoin Indexer Service
## Complete Features
### Basic Requirements
- [o] Understand Bitcoin Ledger, Transaction and UTXO
- [o] Differentiate P2TR and other Bitcoin Locking Scripts
- [o] Design a pipeline that processes Bitcoin ledger (Block ⇒ Transaction ⇒ UTXO ⇒ Balance)
- [o] Design a scalable RDBMS schema
- [o] Store Aggregated data, such as Snapshot of Balance, if needed
- [o] Use de facto standard Bitcoin JSON RPC (QuickNode free tier) for developing and testing

### Feature Requirements
#### Architecture
#### 3rd party libraries choice
```
Bitcoin Libs:
- bitcoin: Bitcoin Core Libs

SQL Libs:
- sqlx: Postgres Lib for query and migration

Web Libs:
- actix-web: Web framework
- reqwest: HTTP client

Error Handling Libs:
- anyhow: Convenient Error type
- thiserror: Error type and error handling

Logging Libs:
- env_logger: Logging
- tracing: Distributed Tracing
```

### Advanced Requirements
- [o] Automatically indexing new Bitcoin blocks
- [o] Multithreading
- [o] Set up configuration options using configuration file
- [x] View logs on visualization dashboard (Kibana, Grafana, ...)
- [o] Docker Compose
- [x] Kubernetes
- [x] Deploy on cloud computing platforms (AWS, Azure, GCP, ...)
- [x] CI 
- [x] CD

## Run
### Build in local with docker
#### Start Postgres and Adminer(Visualize DB at http://localhost:8888)
```bash
local-pg
```

#### Start Bitcoin with QuickNode
```
No need to run any container, just use the default config
```

#### Start Bitcoin Testnet
```bash
local-testnet
```

#### Start Bitcoin Regtest
```bash
local-reg
```

To switch the network, go to `Network Config / Environment` for more details.

### Compile & Run

```bash
just local-mono
```
Note: 
- `just` is a build tool to simplify the build process like `Makefile` (https://just.systems/man/en/)
- `local-mono` will start the indexer, and auto scan from the `start_block` in the config file. If `start_block` is omitted, it will scan from the newest block queried from the chain.
### CLI

```bash
cargo run --bin bis indexer %SUBCOMMAND%
```
```
USAGE:
    bis indexer [OPTIONS] <SUBCOMMAND>

OPTIONS:
    -c, --config-file <CONFIG_FILE>    [env: BIS_CONFIG_FILE=]
    -h, --help                         Print help information
        --host <HOST>                  Host string in "${HOST}:${PORT}" format [env: BIS_HOST=]
                                       [default: 127.0.0.1:3030]
    -V, --version                      Print version information

SUBCOMMANDS:
    help          Print this message or the help of the given subcommand(s)
    pause         Pause the harvester
    resume        Resume the harvester
    scan-block    Scan blocks from `from` to `to`
    terminate     Terminate the harvester
```

For example:

```bash
cargo run --bin bis indexer scan-block --from 1000000 --to 1000001
```

```bash
cargo run --bin bis indexer pause
```

## Network Config / Environment

- (Default) mainnet (./deployment/config.toml)
- testnet (./deployment/config_staging.toml)
- regtest (./deployment/config_dev.toml)

You can set the config file path in `BIS_CONFIG_FILE` environment variable.

Note:
- `magic` is the network magic number of the network to query.
- `provider_url` is the URL of the Bitcoin node to query.
- `start_block` (Optional) is the block number to start the harvester from. If omitted, the newest block queried from the chain will be the `start_block` value.
- `poll_frequency_ms` is the frequency of the harvester to query the chain for new blocks.

database will create `config` table to store and assert the network config to prevent you switch to different network accidentally with existing data.


## API

### Raw APIs

#### Get Latest Block Height
- **GET** `/api/v1/raw/block/latest`
- **Response**
```json
{
    "latest_block_height": block_height
}
```

#### Get Block Data
- **GET** `/api/v1/raw/block/:block_hash`
- **Parameters**
  - `block_hash`: Block hash
- **Response**
```json
{
    "block_header_data": block_header_data
}
```

#### Get Transaction Data
- **GET** `/api/v1/raw/transaction/:transaction_id`
- **Parameters**
  - `transaction_id`: Transaction ID
- **Response**
```json
{
    "transaction_data": transaction_data
}
```

### Processed APIs

#### Get Current Balance
- **GET** `/api/v1/processed/p2tr/:p2tr_address/balance`
- **Parameters**
  - `p2tr_address`: Bech32m encoded address
- **Example**
  - `/api/v1/processed/p2tr/bc1qrqp9vfakep7wsze7h62crghz7h0kh5ry3ynf5v/balance`
- **Response** (balance in satoshi)
```json
{
    "curent_balance": current_balance
}
```

#### Get UTXOs
- **GET** `/api/v1/processed/p2tr/:p2tr_address/utxo`
- **Parameters**
  - `p2tr_address`: Bech32m encoded address
- **Example**
  - `/api/v1/processed/p2tr/bc1qrqp9vfakep7wsze7h62crghz7h0kh5ry3ynf5v/utxo`
- **Response**
```json
[
    {
        "transaction_id": transaction_id,
        "transaction_index": transaction_index,
        "satoshi": satoshi,
        "block_height": block_height
    },
    ...
]
```

#### Aggregated APIs
- **GET** `/api/v1/aggregated/p2tr/:p2tr_address`
- **Parameters**
  - `p2tr_address`: Bech32m encoded address
  - `time_span`: Time span for snapshots
    - `m`: Recent Month
    - `w`: Recent Week
    - `d`: Recent Day
  - `granularity`: Snapshot interval
    - `w`: Weekly
    - `d`: Daily
    - `h`: Hourly
- **Example**
  - `/api/v1/aggregated/p2tr/bc1qrqp9vfakep7wsze7h62crghz7h0kh5ry3ynf5v?time_span=w&granularity=d`
- **Response** (Example: 1 Week span with Daily granularity)
```json
{
    "1731772800": [{ "balance": "snapshooted_balance" }], // 2024.11.17
    "1731686400": [{ "balance": "snapshooted_balance" }], // 2024.11.16
    "1731600000": [{ "balance": "snapshooted_balance" }], // 2024.11.15
    "1731513600": [{ "balance": "snapshooted_balance" }], // 2024.11.14
    "1731427200": [{ "balance": "snapshooted_balance" }], // 2024.11.13
    "1731340800": [{ "balance": "snapshooted_balance" }], // 2024.11.12
    "1731254400": [{ "balance": "snapshooted_balance" }]  // 2024.11.11
}
```

## Reorg
TODO

## Graceful Shutdown
Once the indexer is running, you can press `Ctrl+C` or through the CLI `terminate` command to terminate the indexer gracefully, and then the aggregator will be notified and drain all the events in the channel.
Note: I didn't implement the detail behavior for the aggregator, it is stateless, so once aggregator is killed without processing all the events, the remaining events will be lost.