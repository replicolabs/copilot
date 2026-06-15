use std::sync::Arc;

use serde::Serialize;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;

use crate::{
    Error,
    jito_tips::{self, TIP_FLOOR_URL, TipFloor},
    prio_fees::{self, PrioFees},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CongestionLevel {
    Low,
    Moderate,
    High,
    Severe,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Congestion {
    pub level: CongestionLevel,
    pub tip_tail_ratio: f64,
    pub median_rising: bool,
}

impl Congestion {
    fn assess(jito: &TipFloor) -> Self {
        let tip_tail_ratio = jito.p95 as f64 / jito.p50.max(1) as f64;
        let median_rising = jito.p50 > jito.ema_p50;
        let level = match tip_tail_ratio {
            r if r >= 100.0 => CongestionLevel::Severe,
            r if r >= 20.0 => CongestionLevel::High,
            r if r >= 4.0 && median_rising => CongestionLevel::High,
            r if r >= 4.0 => CongestionLevel::Moderate,
            _ if median_rising => CongestionLevel::Moderate,
            _ => CongestionLevel::Low,
        };
        Self {
            level,
            tip_tail_ratio,
            median_rising,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct TipSuggestion {
    pub jito_tip_floor: TipFloor,
    pub prio_fees: PrioFees,
    pub congestion: Congestion,
}

impl TipSuggestion {
    pub fn baseline_tip_lamports(&self) -> u64 {
        self.jito_tip_floor.p75
    }
}

#[derive(Clone)]
pub struct TipOracle {
    http: reqwest::Client,
    rpc: Arc<RpcClient>,
    tip_floor_url: String,
}

impl TipOracle {
    pub fn new(rpc: Arc<RpcClient>) -> Self {
        Self::with_tip_floor_url(rpc, TIP_FLOOR_URL.to_owned())
    }

    pub fn with_tip_floor_url(rpc: Arc<RpcClient>, tip_floor_url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            rpc,
            tip_floor_url,
        }
    }

    pub async fn snapshot(&self) -> Result<TipSuggestion, Error> {
        let (jito_tip_floor, prio_fees) = tokio::try_join!(
            jito_tips::fetch(&self.http, &self.tip_floor_url),
            prio_fees::fetch(&self.rpc),
        )?;
        let congestion = Congestion::assess(&jito_tip_floor);
        Ok(TipSuggestion {
            jito_tip_floor,
            prio_fees,
            congestion,
        })
    }
}
