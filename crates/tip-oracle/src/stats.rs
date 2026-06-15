use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Percentiles {
    pub p25: u64,
    pub p50: u64,
    pub p75: u64,
    pub p95: u64,
    pub p99: u64,
}

impl Percentiles {
    pub fn from_unsorted(mut samples: Vec<u64>) -> Self {
        if samples.is_empty() {
            return Self::ZERO;
        }
        samples.sort_unstable();
        Self {
            p25: percentile(&samples, 0.25),
            p50: percentile(&samples, 0.50),
            p75: percentile(&samples, 0.75),
            p95: percentile(&samples, 0.95),
            p99: percentile(&samples, 0.99),
        }
    }

    pub const ZERO: Self = Self {
        p25: 0,
        p50: 0,
        p75: 0,
        p95: 0,
        p99: 0,
    };
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    debug_assert!(!sorted.is_empty(), "percentile of empty slice");
    if sorted.len() == 1 {
        return sorted[0];
    }
    let fraction = fraction.clamp(0.0, 1.0);
    let rank = fraction * (sorted.len() - 1) as f64;
    let base = rank.floor() as usize;
    let rest = rank - base as f64;
    if rest == 0.0 || base + 1 >= sorted.len() {
        return sorted[base];
    }
    let lo = sorted[base] as f64;
    let hi = sorted[base + 1] as f64;
    (lo + (hi - lo) * rest).round() as u64
}
