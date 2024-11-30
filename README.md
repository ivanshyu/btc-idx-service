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

## Run

### Compile & Run

```bash
just local-mono
```
Note: 
- `just` is a build tool to simplify the build process.
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

## Graceful Shutdown
Once the indexer is running, you can press `Ctrl+C` or through the CLI `terminate` command to terminate the indexer gracefully, and then the aggregator will be notified and drain all the events in the channel.
Note: I didn't implement the detail behavior for the aggregator, it is stateless, so once aggregator is killed without processing all the events, the remaining events will be lost.