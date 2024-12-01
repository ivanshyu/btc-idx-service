use crate::bitcoin::{
    harvester::processor::Error,
    types::{BlockInfo, BTC_NETWORK},
};
use crate::rpc_client::BitcoinRpcClient;
use crate::sqlx_postgres::{get_config, upsert_config};

use std::convert::Into;
use std::fmt::Debug;
use std::sync::Arc;

use bitcoin::{BlockHash, Network};
use sqlx::PgPool;

pub struct Client {
    name: String,
    inner: Arc<BitcoinRpcClient>,
}

impl Client {
    pub fn new(
        name: String,
        url: String,
        rpc_user: Option<String>,
        rpc_pwd: Option<String>,
    ) -> Self {
        log::info!(
            "Connecting to Bitcoin node: {}, user: {:?}/{:?}",
            url,
            rpc_user,
            rpc_pwd
        );

        let rpc_client = BitcoinRpcClient::new(url, rpc_user, rpc_pwd);
        Self {
            name,
            inner: Arc::new(rpc_client),
        }
    }

    pub fn inner(&self) -> &BitcoinRpcClient {
        &self.inner
    }

    pub async fn get_tip_number(&self) -> Result<usize, Error> {
        self.inner.get_block_count().await.map_err(Into::into)
    }

    pub async fn scan_block(&self, number: Option<usize>) -> Result<BlockInfo, Error> {
        let (header, block) = match number {
            Some(num) => {
                let hash = self.inner.get_block_hash(num).await?;

                futures::try_join!(
                    self.inner.get_block_header(&hash),
                    self.inner.get_block(&hash)
                )?
            }
            _ => {
                let num = self.inner.get_block_count().await?;
                let hash = self.inner.get_block_hash(num).await?;

                futures::try_join!(
                    self.inner.get_block_header(&hash),
                    self.inner.get_block(&hash)
                )?
            }
        };
        Ok(BlockInfo {
            header,
            body: block,
        })
    }

    pub async fn scan_block_by_hash(&self, hash: &BlockHash) -> Result<BlockInfo, Error> {
        let (header, block) = futures::try_join!(
            self.inner.get_block_header(hash),
            self.inner.get_block(hash)
        )?;

        Ok(BlockInfo {
            header,
            body: block,
        })
    }

    pub async fn assert_environment(&self, pool: &PgPool) -> Result<(), Error> {
        let env = *BTC_NETWORK.get().unwrap();
        //compare db info
        if let Some(info) = get_config::<_, Network>(pool, "network_info").await? {
            if env != info {
                return Err(Error::Other(
                    "Client: database has different network info".into(),
                ));
            }
        }

        upsert_config(pool, "network_info", &env).await?;
        log::debug!("{:?}", env);

        Ok(())
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("name", &self.name).finish()
    }
}
