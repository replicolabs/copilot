use serde_json::Value;

use crate::{Error, client::AnthropicClient, decision::AgentDecision, reasoning_log::ReasoningLog};

const SYSTEM_PROMPT: &str = "\
You are Copilot's autonomous retry decision engine for Solana transactions submitted as Jito bundles. \
A bundle has just failed to land. You will receive, as JSON, the failure classification (kind, confidence, \
rationale, alternatives) and live network context (chain tips, the blockhash age, the tip that was paid, the \
recent Jito landed-tip percentiles, a congestion read, whether a Jito leader produced, and recent lifecycle \
outcomes). Decide what Copilot should do next.

Ground your reasoning in how Solana actually behaves:
- A blockhash is valid for ~150 slots (~60s). If it expired, the fix is a fresh blockhash, not a higher tip; \
the tip only needs to rise if congestion also rose.
- A tip below the recent landed median/p75 is the usual cause of a fee-driven non-landing; the fix is to raise \
the tip toward or above the landing percentiles, scaled by the congestion read — not to overpay blindly.
- A skipped leader is transient: the bundle was simply never given a block. Waiting briefly and retrying is \
usually right; the tip rarely needs to change.
- Compute-unit exhaustion is not fixable by retrying the same transaction; abort unless something upstream changes.
- Higher confidence in a clear cause warrants a more decisive action; genuine ambiguity warrants a conservative one.

Choose exactly one action:
- \"retry\": resubmit with a refreshed blockhash and, if you judge it necessary, an adjusted tip.
- \"abort\": stop; retrying cannot help.
- \"wait\": hold briefly, then the caller may retry (e.g. a transient skipped leader).

Respond with ONLY a single JSON object, no markdown and no prose, in exactly this shape:
{\"action\": \"retry|abort|wait\", \"new_tip_lamports\": <integer or null>, \"reasoning\": \"<one or two sentences>\", \"confidence\": <number between 0 and 1>}
Set \"new_tip_lamports\" to null when you are not changing the tip.";

pub struct RetryAgent {
    client: AnthropicClient,
    log: Option<ReasoningLog>,
}

impl RetryAgent {
    pub fn new(client: AnthropicClient) -> Self {
        Self { client, log: None }
    }

    pub fn with_log(mut self, log: ReasoningLog) -> Self {
        self.log = Some(log);
        self
    }
    pub async fn decide(&self, context: &Value) -> Result<AgentDecision, Error> {
        let rendered =
            serde_json::to_string_pretty(context).unwrap_or_else(|_| context.to_string());
        let user = format!(
            "A bundle submission just failed. Failure and live network context:\n\n```json\n{rendered}\n```\n\nDecide the action. Respond with ONLY the decision JSON."
        );

        let raw = self.client.complete(SYSTEM_PROMPT, &user).await?;
        let decision = AgentDecision::parse(&raw)?;

        if let Some(log) = &self.log {
            log.record(context, &decision, self.client.model())?;
        }
        Ok(decision)
    }
}
