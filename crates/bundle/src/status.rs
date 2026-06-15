use serde_json::Value;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleStatus {
    Invalid,
    Pending,
    Failed,
    Landed { slot: u64 },
}

impl BundleStatus {
    pub fn from_response(body: &Value) -> Result<Self, Error> {
        let entry = body
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(Value::as_array)
            .and_then(|values| values.first());

        let Some(entry) = entry else {
            return Ok(BundleStatus::Invalid);
        };

        let slot = entry
            .get("landed_slot")
            .or_else(|| entry.get("slot"))
            .and_then(Value::as_u64);

        let status = entry
            .get("status")
            .or_else(|| entry.get("confirmation_status"))
            .and_then(Value::as_str)
            .ok_or_else(|| Error::BadResponse(entry.to_string()))?;

        Ok(match status.to_ascii_lowercase().as_str() {
            "landed" | "processed" | "confirmed" | "finalized" => match slot {
                Some(slot) => BundleStatus::Landed { slot },
                None => BundleStatus::Pending,
            },
            "pending" => BundleStatus::Pending,
            "failed" => BundleStatus::Failed,
            "invalid" => BundleStatus::Invalid,
            _ => return Err(Error::BadResponse(status.to_owned())),
        })
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, BundleStatus::Landed { .. } | BundleStatus::Failed)
    }
}
