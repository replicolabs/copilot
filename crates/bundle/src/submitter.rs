use jito_sdk_rust::JitoJsonRpcSDK;
use serde_json::Value;
use solana_transaction::versioned::VersionedTransaction;

use crate::{Error, builder::encode_transaction, status::BundleStatus, tip_accounts::TipAccounts};

pub const MAINNET_BLOCK_ENGINE: &str = "https://mainnet.block-engine.jito.wtf/api/v1";
pub const MAX_BUNDLE_TXNS: usize = 5;

pub struct BundleSubmitter {
    sdk: JitoJsonRpcSDK,
}

impl BundleSubmitter {
    pub fn new(base_url: &str) -> Self {
        Self {
            sdk: JitoJsonRpcSDK::new(base_url, None),
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

        let response = self
            .sdk
            .send_bundle(Some(Value::Array(encoded)), None)
            .await?;

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
