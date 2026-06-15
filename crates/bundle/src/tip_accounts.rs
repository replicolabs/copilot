use rand::seq::SliceRandom;
use serde_json::Value;
use solana_pubkey::Pubkey;

use crate::Error;
pub const MIN_TIP_LAMPORTS: u64 = 1_000;

#[derive(Debug, Clone)]
pub struct TipAccounts {
    accounts: Vec<Pubkey>,
}

impl TipAccounts {
    pub fn from_response(body: &Value) -> Result<Self, Error> {
        let accounts: Vec<Pubkey> = body
            .get("result")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(|s| s.parse().ok())
                    .collect()
            })
            .unwrap_or_default();

        if accounts.is_empty() {
            return Err(Error::NoTipAccounts);
        }
        Ok(Self { accounts })
    }

    pub fn from_accounts(accounts: Vec<Pubkey>) -> Result<Self, Error> {
        if accounts.is_empty() {
            return Err(Error::NoTipAccounts);
        }
        Ok(Self { accounts })
    }

    pub fn pick(&self) -> Pubkey {
        *self
            .accounts
            .choose(&mut rand::thread_rng())
            .expect("tip account set is non-empty by construction")
    }

    pub fn count(&self) -> usize {
        self.accounts.len()
    }
}
