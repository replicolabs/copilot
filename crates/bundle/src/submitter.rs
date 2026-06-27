use jito_sdk_rust::JitoJsonRpcSDK;
use reqwest::Client;
use serde_json::{Value, json};
use solana_transaction::versioned::VersionedTransaction;

use crate::{Error, builder::encode_transaction, status::BundleStatus, tip_accounts::TipAccounts};

pub const MAINNET_BLOCK_ENGINE: &str = "https://mainnet.block-engine.jito.wtf/api/v1";
pub const MAX_BUNDLE_TXNS: usize = 5;

pub struct BundleSubmitter {
    sdk: JitoJsonRpcSDK,
    client: Client,
    base_url: String,
    uuid: Option<String>,
}

impl BundleSubmitter {
    pub fn new(base_url: &str, uuid: Option<String>) -> Self {
        if uuid.is_none() {
            tracing::warn!(
                "COPILOT_JITO_UUID not set — bundles will be marked Invalid by the block engine; \
                 please get a UUID"
            );
        } else {
            tracing::info!("bundle submitter ready (authenticated)");
        }
        Self {
            sdk: JitoJsonRpcSDK::new(base_url, None),
            client: Client::new(),
            base_url: base_url.to_string(),
            uuid,
        }
    }

    pub async fn tip_accounts(&self) -> Result<TipAccounts, Error> {
        let response = self
            .sdk
            .get_tip_accounts()
            .await
            .map_err(anyhow::Error::from)?;
        TipAccounts::from_response(&response)
    }

    pub async fn submit(&self, transactions: &[VersionedTransaction]) -> Result<String, Error> {
        if transactions.is_empty() || transactions.len() > MAX_BUNDLE_TXNS {
            return Err(Error::BundleSize(transactions.len()));
        }

        let encoded: Vec<Value> = transactions
            .iter()
            .map(|tx| encode_transaction(tx).map(Value::String))
            .collect::<Result<_, _>>()?;

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [encoded, {"encoding": "base64"}]
        });

        let mut req = self
            .client
            .post(format!("{}/bundles", self.base_url))
            .header("Content-Type", "application/json");
        if let Some(uuid) = &self.uuid {
            req = req.header("x-jito-auth", uuid);
        }
        let response: Value = req
            .json(&body)
            .send()
            .await
            .map_err(anyhow::Error::from)?
            .json()
            .await
            .map_err(anyhow::Error::from)?;

        response
            .get("result")
            .and_then(Value::as_str)
            .map(String::from)
            .ok_or_else(|| Error::BadResponse(response.to_string()))
    }

    pub async fn inflight_status(&self, bundle_id: &str) -> Result<BundleStatus, Error> {
        let response = self
            .sdk
            .get_in_flight_bundle_statuses(vec![bundle_id.to_owned()])
            .await?;
        BundleStatus::from_response(&response)
    }

    pub async fn status(&self, bundle_id: &str) -> Result<BundleStatus, Error> {
        let response = self
            .sdk
            .get_bundle_statuses(vec![bundle_id.to_owned()])
            .await?;
        BundleStatus::from_response(&response)
    }
}
