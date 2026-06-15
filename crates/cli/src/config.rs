use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use bundle::MAINNET_BLOCK_ENGINE;
use solana_keypair::Keypair;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;

#[derive(Debug, Clone)]
pub struct Config {
    pub rpc_url: String,
    pub grpc_url: String,
    pub grpc_x_token: Option<String>,
    pub block_engine: String,
    pub model: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let rpc_url = require("COPILOT_RPC_URL")?;
        let grpc_url = require("COPILOT_GRPC_URL")?;
        let grpc_x_token = optional("COPILOT_GRPC_X_TOKEN");
        let block_engine =
            optional("COPILOT_BLOCK_ENGINE").unwrap_or_else(|| MAINNET_BLOCK_ENGINE.to_owned());
        let model = optional("COPILOT_MODEL");

        Ok(Self {
            rpc_url,
            grpc_url,
            grpc_x_token,
            block_engine,
            model,
        })
    }

    pub fn rpc_client(&self) -> Arc<RpcClient> {
        Arc::new(RpcClient::new(self.rpc_url.clone()))
    }
}

pub fn load_keypair() -> Result<Keypair> {
    let source = require("COPILOT_KEYPAIR")?;

    if Path::new(&source).is_file() {
        let contents = fs::read_to_string(&source)
            .with_context(|| format!("reading keypair file at {source}"))?;
        let bytes: Vec<u8> = serde_json::from_str(contents.trim())
            .with_context(|| format!("parsing keypair file at {source} as a JSON byte array"))?;
        Keypair::try_from(bytes.as_slice())
            .map_err(|_| anyhow!("keypair file at {source} is not a valid 64-byte ed25519 keypair"))
    } else {
        Ok(Keypair::from_base58_string(&source))
    }
}

fn require(key: &str) -> Result<String> {
    optional(key).ok_or_else(|| anyhow!("{key} is not set; export it (see .env.example)"))
}

fn optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}
