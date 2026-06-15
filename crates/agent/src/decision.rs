use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Retry,
    Abort,
    Wait,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDecision {
    pub action: Action,
    pub new_tip_lamports: Option<u64>,
    pub reasoning: String,
    pub confidence: f64,
}

impl AgentDecision {
    pub fn should_retry(&self) -> bool {
        matches!(self.action, Action::Retry)
    }

    pub fn parse(text: &str) -> Result<Self, Error> {
        let json = extract_json_object(text).ok_or_else(|| Error::Parse {
            detail: "no JSON object found in model output".to_owned(),
            raw: text.to_owned(),
        })?;
        serde_json::from_str(json).map_err(|err| Error::Parse {
            detail: err.to_string(),
            raw: text.to_owned(),
        })
    }
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end >= start).then(|| &text[start..=end])
}
