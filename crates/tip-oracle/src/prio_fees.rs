use serde::Serialize;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::response::RpcPrioritizationFee;

use crate::{Error, stats::Percentiles};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PrioFees {
    pub percentiles: Percentiles,
    pub slots_sampled: usize,
}

impl PrioFees {
    pub fn from_samples(samples: &[RpcPrioritizationFee]) -> Self {
        let fees: Vec<u64> = samples.iter().map(|s| s.prioritization_fee).collect();
        Self {
            slots_sampled: fees.len(),
            percentiles: Percentiles::from_unsorted(fees),
        }
    }
}

pub async fn fetch(rpc: &RpcClient) -> Result<PrioFees, Error> {
    let samples = rpc.get_recent_prioritization_fees(&[]).await?;
    Ok(PrioFees::from_samples(&samples))
}
