#[cfg(test)]
mod tests {
    use bis_core::bitcoin::aggregator::Aggregator;
    use bis_core::bitcoin::harvester::Harvester;
    use bis_core::bitcoin::harvester::{client::Client, processor::Processor};
    use bis_core::bitcoin::types::{AggregatorMsg, BTC_NETWORK};
    use bis_core::sqlx_postgres::{bitcoin as db, connect_and_migrate, EMBEDDED_MIGRATE};

    use std::process::Command;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use atb::logging::init_logger;
    use bigdecimal::BigDecimal;
    use bitcoin::Network;
    use sqlx::postgres::PgPool;
    use tokio::sync::mpsc::{self, UnboundedSender};
    use tokio::sync::Notify;

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
        _container_name: String,
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
                println!("Checking regtest container is ready");
                if let Ok(block_count) = run_bitcoin_cli(&["getblockcount"]) {
                    assert_eq!(block_count, "0");
                    break;
                }
                thread::sleep(Duration::from_secs(3)); // wait for the container to be ready
            }

            let _ = run_bitcoin_cli(&["createwallet", "test_wallet"]);
            let _ = run_bitcoin_cli(&["loadwallet", "test_wallet"]);

            let _ = run_bitcoin_cli(&["settxfee", "0.0001"]);

            Self {
                _container_name: container_name.to_string(),
            }
        }
    }

    impl Drop for DockerTestFixture {
        fn drop(&mut self) {
            let _ = run_bitcoin_cli(&["stop"]);

            std::thread::sleep(std::time::Duration::from_secs(2));

            let _ = Command::new("docker")
                .args([
                    "exec",
                    "bitcoin-regtest",
                    "rm",
                    "-rf",
                    "/bitcoin/.bitcoin/regtest",
                ])
                .output();
        }
    }

    async fn connect_postgres() -> PgPool {
        let database_url =
            format!("postgres://postgres:123456@localhost:5432/integration_tests?sslmode=disable");

        let pool = connect_and_migrate(&database_url, 5)
            .await
            .expect("db creation should succeed. qed");

        EMBEDDED_MIGRATE
            .run(&pool)
            .await
            .expect("migration should succeed. qed");

        pool
    }

    async fn create_harvester(
        pg_pool: PgPool,
        event_sender: UnboundedSender<AggregatorMsg>,
    ) -> anyhow::Result<Harvester> {
        let client = Client::new(
            "bitcoin client".to_owned(),
            "http://localhost:18443".to_owned(),
            Some("user".to_owned()),
            Some("password".to_owned()),
        );

        BTC_NETWORK
            .set(Network::Regtest)
            .expect("BTC_NETWORK should not be set");

        client.assert_environment(&pg_pool).await?;

        let client = Arc::new(client);

        let processor =
            Processor::new(pg_pool, client.clone(), Network::Regtest, event_sender).await?;

        Ok(Harvester::new(
            client.clone(),
            processor,
            Some(0),
            None,
            1000,
            "bitcoin harvester".to_owned(),
        )
        .await)
    }

    async fn create_aggregator(
        pg_pool: PgPool,
    ) -> anyhow::Result<(UnboundedSender<AggregatorMsg>, Aggregator)> {
        let (sender, receiver) = mpsc::unbounded_channel();

        let shutdown_notify = Arc::new(Notify::new());
        let aggregator = Aggregator::new(pg_pool, receiver, shutdown_notify);
        Ok((sender, aggregator))
    }

    #[test]
    fn test_bitcoin_transactions() {
        let _fixture = DockerTestFixture::new("bitcoin-regtest");

        // options for logging indexer for debugging
        // init_logger("info,sqlx=info", true);

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime needed to continue. qed");

        let (harvester, aggregator) = rt.block_on(async move {
            let pg_pool = connect_postgres().await;

            println!("created postgres pool");

            if db::get_latest_block_num(&pg_pool).await.unwrap().is_some() {
                panic!("Test database is not empty, please drop it before running the test");
            }

            let (event_sender, aggregator) = create_aggregator(pg_pool.clone()).await.unwrap();

            println!("created aggregator");

            let harvester = create_harvester(pg_pool, event_sender).await.unwrap();

            println!("created harvester");

            (harvester, aggregator)
        });

        let handler = harvester.handle();

        let _ = rt.spawn(async move {
            harvester.start().await.unwrap();
        });

        let _ = rt.spawn(async move {
            aggregator.start().await.unwrap();
        });

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
        let balance1 = get_onchain_balance(&addr1).unwrap();
        let balance2 = get_onchain_balance(&addr2).unwrap();
        let balance3 = get_onchain_balance(&addr3).unwrap();

        let end = run_bitcoin_cli(&["getblockcount"])
            .unwrap()
            .parse::<usize>()
            .unwrap();

        rt.block_on(async move {
            loop {
                let current_indexer_block = db::get_latest_block_num(&handler.storage.conn)
                    .await
                    .unwrap()
                    .unwrap_or_default();

                println!("Current indexer block: {}", current_indexer_block);

                if current_indexer_block >= end {
                    break;
                }

                thread::sleep(Duration::from_secs(1));
            }
            println!("Final balances:");
            println!("ADDR1: {}", balance1);

            let db_balance1 = db::get_btc_balance(&handler.storage.conn, &addr1)
                .await
                .unwrap()
                .unwrap()
                .amount;
            assert_eq!(balance1, db_balance1);

            println!("ADDR2: {}", balance2);
            let db_balance2 = db::get_btc_balance(&handler.storage.conn, &addr2)
                .await
                .unwrap()
                .unwrap()
                .amount;
            assert_eq!(balance2, db_balance2);

            println!("ADDR3: {}", balance3);
            let db_balance3 = db::get_btc_balance(&handler.storage.conn, &addr3)
                .await
                .unwrap()
                .unwrap()
                .amount;
            assert_eq!(balance3, db_balance3);

            handler.terminate().await.unwrap();
        });
    }

    fn get_onchain_balance(address: &str) -> anyhow::Result<BigDecimal> {
        let balance: BigDecimal =
            run_bitcoin_cli(&["listunspent", "0", "9999999", &format!("[\"{address}\"]")])
                .map(|s| {
                    let json = serde_json::Value::from_str(&s).unwrap();
                    json.as_array().unwrap().clone()
                })?
                .into_iter()
                .map(|utxo| {
                    let amount = utxo.get("amount").unwrap();
                    BigDecimal::from_str(amount.to_string().as_str()).unwrap()
                })
                .sum();

        Ok(balance * BigDecimal::from(100_000_000))
    }
}
