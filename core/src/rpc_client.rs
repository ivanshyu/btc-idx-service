use std::fmt::Display;

use anyhow::anyhow;
use bitcoin::{consensus::encode, Block, BlockHash};
use bitcoincore_rpc::json;
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("reqwest error: `{0}`")]
    Reqwest(#[from] reqwest::Error),

    #[error("rpc error: {0}")]
    RpcError(#[from] RpcError),

    #[error("anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),

    /// Other Error
    #[error("Other Error: {0}")]
    Other(Box<dyn std::error::Error + Send + Sync + 'static>),
}

#[derive(Serialize, Debug)]
struct RpcRequest<'a> {
    jsonrpc: String,
    id: String,
    method: String,
    params: &'a [serde_json::Value],
}

#[derive(Deserialize, Debug)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

impl<T> From<RpcResponse<T>> for Result<T, Error>
where
    T: DeserializeOwned,
{
    fn from(value: RpcResponse<T>) -> Self {
        if let Some(result) = value.result {
            Ok(result)
        } else if let Some(err) = value.error {
            Err(err.into())
        } else {
            Err(anyhow::anyhow!("unknown err").into())
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct RpcError {
    code: i64,
    message: String,
}

impl Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.code, self.message)
    }
}

impl std::error::Error for RpcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

pub struct BitcoinRpcClient {
    client: Client,
    url: String,
    user: Option<String>,
    password: Option<String>,
}

impl BitcoinRpcClient {
    pub fn new(url: String, user: Option<String>, password: Option<String>) -> Self {
        let client = Client::new();
        Self {
            client,
            url,
            user,
            password,
        }
    }

    fn into_json<T>(val: T) -> Result<serde_json::Value, Error>
    where
        T: serde::ser::Serialize,
    {
        serde_json::to_value(val).map_err(|e| anyhow!(e).into())
    }

    async fn request<R: DeserializeOwned>(
        &self,
        method: &str,
        params: &[serde_json::Value],
    ) -> Result<RpcResponse<R>, Error> {
        let rpc_request = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: "1".to_string(),
            method: method.to_string(),
            params,
        };

        let response = match (self.user.as_deref(), self.password.as_deref()) {
            (Some(user), Some(password)) => {
                self.client
                    .post(&self.url)
                    .basic_auth(user, Some(password))
                    .json(&rpc_request)
                    .send()
                    .await?
            }
            _ => {
                self.client
                    .post(&self.url)
                    .json(&rpc_request)
                    .send()
                    .await?
            }
        };

        if response.status().is_success() {
            response.json::<RpcResponse<R>>().await.map_err(Into::into)
        } else if let Err(err) = response.error_for_status() {
            Err(err.into())
        } else {
            Err(anyhow::anyhow!("unknown err").into())
        }
    }

    pub async fn get_block_count(&self) -> Result<usize, Error> {
        self.request::<usize>("getblockcount", &[]).await?.into()
    }

    // #NOTE: get hex encoded block, and then deserialize to domain type
    pub async fn get_block(&self, hash: &BlockHash) -> Result<Block, Error> {
        self.request::<String>("getblock", &[Self::into_json(hash)?, 0.into()])
            .await
            .map_err(Error::from)
            .and_then(|r| {
                r.result
                    .ok_or_else(|| anyhow::anyhow!("missing result").into())
            })
            .and_then(|s| encode::deserialize_hex(&s).map_err(|e| anyhow!(e).into()))
    }

    pub async fn get_block_header(
        &self,
        hash: &BlockHash,
    ) -> Result<json::GetBlockHeaderResult, Error> {
        self.request::<json::GetBlockHeaderResult>("getblockheader", &[Self::into_json(hash)?])
            .await?
            .into()
    }

    pub async fn get_block_hash(&self, height: usize) -> Result<BlockHash, Error> {
        self.request::<BlockHash>("getblockhash", &[height.into()])
            .await?
            .into()
    }
}
