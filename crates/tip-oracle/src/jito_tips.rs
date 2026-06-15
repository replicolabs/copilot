use serde::Serialize;
use serde_json::Value;

use crate::Error;

const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;
pub const TIP_FLOOR_URL: &str = "https://bundles.jito.wtf/api/v1/bundles/tip_floor";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TipFloor {
    pub p25: u64,
    pub p50: u64,
    pub p75: u64,
    pub p95: u64,
    pub p99: u64,
    pub ema_p50: u64,
}

impl TipFloor {
    pub fn from_response(body: &Value) -> Result<Self, Error> {
        let first = body
            .as_array()
            .and_then(|entries| entries.first())
            .ok_or(Error::Malformed("expected a non-empty array"))?;

        let sol_to_lamports = |key: &'static str| -> Result<u64, Error> {
            let sol = first
                .get(key)
                .and_then(Value::as_f64)
                .ok_or(Error::Malformed("missing or non-numeric percentile"))?;
            Ok((sol.max(0.0) * LAMPORTS_PER_SOL).round() as u64)
        };

        Ok(Self {
            p25: sol_to_lamports("landed_tips_25th_percentile")?,
            p50: sol_to_lamports("landed_tips_50th_percentile")?,
            p75: sol_to_lamports("landed_tips_75th_percentile")?,
            p95: sol_to_lamports("landed_tips_95th_percentile")?,
            p99: sol_to_lamports("landed_tips_99th_percentile")?,
            ema_p50: sol_to_lamports("ema_landed_tips_50th_percentile")?,
        })
    }
}

pub async fn fetch(client: &reqwest::Client, url: &str) -> Result<TipFloor, Error> {
    let body = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;
    TipFloor::from_response(&body)
}
