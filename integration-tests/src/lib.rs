use std::process::Command;
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use bigdecimal::BigDecimal;

fn run_bitcoin_cli(args: &[&str]) -> anyhow::Result<String> {
    let mut command_args = vec![
        "exec",
        "bitcoin-regtest",
        "bitcoin-cli",
        "-regtest",
        "-rpcuser=user",
        "-rpcpassword=password",
    ];
    command_args.extend(args);

    let output = Command::new("docker")
        .args(&command_args)
        .output()
        .expect("Failed to execute bitcoin-cli in docker");

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Command failed: {} from args: {}",
            String::from_utf8_lossy(&output.stderr),
            command_args.join(", ")
        ));
    }

    String::from_utf8(output.stdout)
        .map_err(|_| anyhow::anyhow!("Invalid UTF-8 output"))
        .map(|s| s.trim().to_string())
}

struct DockerTestFixture {
    container_name: String,
}

impl DockerTestFixture {
    fn new(container_name: &str) -> Self {
        // make sure the container is clean
        Command::new("docker")
            .args(["restart", container_name])
            .output()
            .expect("Failed to restart bitcoin container");

        thread::sleep(Duration::from_secs(3)); // wait for the container to be ready

        loop {
            if let Ok(blockchain_info) = run_bitcoin_cli(&["getblockchaininfo"]) {
                if !blockchain_info.is_empty() {
                    break;
                }
                thread::sleep(Duration::from_secs(3)); // wait for the container to be ready
                println!("Blockchain info: {}", blockchain_info);
            }
        }

        let _ = run_bitcoin_cli(&["createwallet", "test_wallet"]);
        let _ = run_bitcoin_cli(&["loadwallet", "test_wallet"]);

        let _ = run_bitcoin_cli(&["settxfee", "0.0001"]);

        Self {
            container_name: container_name.to_string(),
        }
    }
}

impl Drop for DockerTestFixture {
    fn drop(&mut self) {
        // Optional: clean up the container after the test
        Command::new("docker")
            .args(["stop", &self.container_name])
            .output()
            .expect("Failed to stop container");
    }
}

#[test]
fn test_bitcoin_transactions() {
    let _fixture = DockerTestFixture::new("bitcoin-regtest");

    // generate test addresses
    let addr1 = run_bitcoin_cli(&["getnewaddress", "test1", "bech32m"]).unwrap();
    let addr2 = run_bitcoin_cli(&["getnewaddress", "test2", "bech32m"]).unwrap();
    let addr3 = run_bitcoin_cli(&["getnewaddress", "test3", "bech32m"]).unwrap();

    println!("Test addresses:");
    println!("ADDR1: {}", addr1);
    println!("ADDR2: {}", addr2);
    println!("ADDR3: {}", addr3);

    // generate initial coins
    let _ = run_bitcoin_cli(&["generatetoaddress", "101", &addr1]);

    // test scenario 1: simple transfer
    let txid1 = run_bitcoin_cli(&["sendtoaddress", &addr2, "1.0"]).unwrap();
    println!("Simple transfer TXID: {}", txid1);

    // confirm the transaction
    let _ = run_bitcoin_cli(&["generatetoaddress", "1", &addr1]);

    // test scenario 2: multiple outputs
    let txid2 = run_bitcoin_cli(&["sendtoaddress", &addr3, "0.5"]).unwrap();
    println!("Multiple outputs TXID: {}", txid2);

    // confirm the transaction
    let _ = run_bitcoin_cli(&["generatetoaddress", "1", &addr1]);

    // get the final balances
    let balance1 = run_bitcoin_cli(&["getreceivedbyaddress", &addr1]).unwrap();
    let balance2 = run_bitcoin_cli(&["getreceivedbyaddress", &addr2]).unwrap();
    let balance3 = run_bitcoin_cli(&["getreceivedbyaddress", &addr3]).unwrap();

    println!("Final balances:");
    println!("ADDR1: {}", balance1);
    println!("ADDR2: {}", balance2);
    println!("ADDR3: {}", balance3);

    // add assertions to verify the results
    let balance2_decimal = balance2.parse::<BigDecimal>().unwrap();
    let balance3_decimal = balance3.parse::<BigDecimal>().unwrap();
    assert!(balance2_decimal >= BigDecimal::from_str("1.0").unwrap());
    assert!(balance3_decimal >= BigDecimal::from_str("0.5").unwrap());
}

#[test]
fn test_bitcoin_cli_connection() {
    let blockchain_info = run_bitcoin_cli(&["getblockchaininfo"]).unwrap();

    println!("Blockchain info: {}", blockchain_info);
    assert!(!blockchain_info.is_empty());
}
