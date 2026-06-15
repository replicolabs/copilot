# Agent reasoning

The boundary between Copilot's deterministic plumbing and the model's judgement:
what the agent receives, how it's called, what it must return, how that's parsed
and logged, and several worked failure→decision examples. This is the file to
open when explaining *why* a decision was what it was.

---

## The boundary (the `agent` crate)

The `agent` crate is a thin, pure boundary to Claude. It has **no Solana logic and
no retry rules of its own** — it formats a context, calls the model, parses the
structured reply, and logs both. Everything domain-specific is either in the
context it's handed or in the model's reasoning. That separation is deliberate:
it's what lets us truthfully say the decision is the model's, not the code's. (If
you ever find yourself describing a fixed "if X then Y" recovery rule in the
code, that's a misread — the code carries no such rule.)

---

## The client mechanics

- **Endpoint:** `POST https://api.anthropic.com/v1/messages` with headers
  `x-api-key` and `anthropic-version: 2023-06-01`.
- **Credential:** read from the environment (`ANTHROPIC_API_KEY`), normally
  **inherited from the Claude Code session** — no key is prompted for
  (`../../setup/references/environment.md`).
- **Model:** `COPILOT_MODEL`, defaulting to a current Sonnet (`claude-sonnet-4-6`).
- **Determinism:** `temperature: 0.0`, `max_tokens` ~1024. Temperature 0 keeps the
  decision stable and reproducible for the same context — appropriate for an
  operational call, not a creative one.
- **Parsing:** the reply's text content is collected and the **first JSON object**
  is extracted (tolerant of any prose before/after it), then deserialized into an
  `AgentDecision`. A reply that can't be parsed is an **error**, not a silent
  default — Copilot will not invent a decision.

---

## The context the agent receives

A single JSON object combining:

- **the failure event** — `kind`, `confidence`, `rationale`, and the
  `alternatives` the classifier couldn't rule out
  (`../../diagnose/references/failure-taxonomy.md`),
- **the chain snapshot** — current slot/blockhash, the Jito landed-tip floor
  percentiles, the priority-fee distribution, and the congestion read,
- **the attempt** — the tip that was paid and the blockhash age at failure.

The **system prompt** grounds the model in the mechanics that matter, so it
reasons from cause rather than reflex:
- blockhashes expire (~150 slots) and the fix is a *fresh blockhash*, not a bigger
  tip;
- tips are priced from the landed floor and scaled by congestion;
- `compute_exceeded` can't be fixed by price;
- a skipped Jito leader is transient.
It frames the task as **one decision over this specific live context**, not a
general essay.

---

## The decision the agent returns

Strict JSON → `AgentDecision`:

```json
{
  "action": "retry",
  "new_tip_lamports": 16000,
  "reasoning": "Blockhash aged ~180 slots, past the 150-slot window — this is an expiry, not under-pricing. Refresh the blockhash and resubmit. Congestion is Low and the paid tip already sat at p75, so hold the tip.",
  "confidence": 0.9
}
```

- **action** — `retry`, `abort`, or `wait`.
- **new_tip_lamports** — optional; the tip for the retry, clamped to the
  1,000-lamport minimum. Omit to reuse the prior tip.
- **reasoning** — the model's natural-language justification.
- **confidence** — the model's own confidence in the call.

---

## The reasoning log

Every decision appends one line to `logs/agent-reasoning.jsonl`:

```
{"ts":"…","model":"claude-sonnet-4-6","context":{…the full input…},"decision":{…the JSON above…}}
```

This is the audit trail. To explain a decision: open the line and walk it — *here
is the live context the model saw* (the failure + the real tip floor + congestion
+ blockhash age), *here is what it returned*, *here is why that matches the
mechanics.* Because the context contains the actual percentiles and age from that
moment, the decision is **verifiably grounded in live data** — the bar the project
sets for "the agent makes a real decision."

---

## Worked examples (cause → expected decision)

**1. Expired blockhash, calm market.**
Context: `expired_blockhash` (0.9); blockhash age ~180 slots; paid 16,000; floor
p75 16,000; congestion Low.
Good decision: `retry`, `new_tip_lamports` ~16,000 (hold), reasoning names the
expiry and the fresh-blockhash fix. *Tell something's off:* a big tip bump here
with no congestion justification — that's the naive "pay more" reflex applied to a
validity failure.

**2. Fee too low, moderate congestion.**
Context: `fee_too_low` (0.6); paid 9,000 vs landed p50 12,000; floor p75 18,000,
p95 60,000; congestion Moderate.
Good decision: `retry`, `new_tip_lamports` ~18,000 (toward p75), reasoning: tip
was under the median, raise to the standard landing bar without chasing the tail.

**3. Fee too low, severe congestion.**
Context: `fee_too_low`; paid 18,000; p50 now 22,000, p95 110,000, p99 300,000,
heavy tail, median rising; congestion Severe.
Good decision: `retry`, `new_tip_lamports` ~110,000 (toward p95), reasoning: the
bar moved; p75 may no longer land, but p99 (300k) overpays for the extreme tail —
aim to clear the bar. *Tell:* jumping straight to 300k "to be safe."

**4. Leader skipped.**
Context: `leader_skipped` (0.8); no Jito leader produced the window; floor
unchanged.
Good decision: `wait` (or `retry` after a brief pause), tip **held**, reasoning:
no auction was lost — wait for a producing Jito leader; a higher tip changes
nothing (`../../watch/references/leader-scheduling.md`).

**5. Compute exceeded.**
Context: `compute_exceeded` (1.0); landed with an on-chain compute error.
Good decision: `abort`, no tip change, reasoning: the CU limit (or the work) must
change upstream; neither a retry-as-is nor a tip fixes it
(`../../solana-internals/references/fees-and-compute.md`).

**6. Ambiguous drop.**
Context: `dropped` (0.4); never landed; no dominant signal; alternatives include
expiry-adjacent and contention; congestion Moderate.
Good decision: the conservative one — a single `retry` with a **fresh blockhash**
at ~p75, then reassess; reasoning explicitly acknowledges the ambiguity rather
than feigning certainty. This low-confidence case is exactly where the model's
judgement over the full live context beats any fixed rule.

---

## Reading a decision critically

The point of the demo is the **reasoning**, not just the outcome. A retry can land
*despite* poor reasoning (e.g. a tip bump on an expired blockhash that happened to
also be fine on fee). When walking `agent-reasoning.jsonl`, check that the action
*and the justification* match the cause — and flag a mismatch even if the retry
landed. That's the difference between "it worked" and "it reasoned correctly,"
and the latter is what the project is demonstrating.