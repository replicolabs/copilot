---
name: inject
description: Run Copilot's autonomous-retry showcase — deliberately inject an expired blockhash, classify the resulting failure, send the live context to Claude, and execute the model's structured retry decision. Use when the user wants to see the AI make a real decision, run the fault-injection demo, demonstrate autonomous retry, show "the agent deciding", or prove that Copilot's recovery is model-driven rather than hardcoded.
---

# inject

This is Copilot's one real operational decision made by an agent: when a bundle
fails, **the model — not a hardcoded branch — decides whether and how to retry.**
The `inject` command triggers that on demand by manufacturing a failure with a
known cause, so the agent's reasoning is observable end to end.

> **Routing:** if this isn't the right skill for the request, consult `../SKILL_ROUTER.md` and switch.

## Prerequisites

- `.env` configured, `copilot watch` shows live slots, payer funded, **mainnet**.
- An authenticated Claude Code session — the agent inherits it; there is no API
  key to set.

## Workflow

1. **Set expectations** so the AI step is unmistakable, not buried in logs. Tell
   the user they'll see four moments:
   - an expired blockhash injected deliberately,
   - the failed attempt classified by the `fault` crate,
   - the live context (failure + chain snapshot + tip floor + congestion) sent to
     Claude, which returns a **structured JSON decision**,
   - Copilot executing that decision — typically a retry with a *fresh blockhash*
     and the agent's chosen tip — and the retry landing.
2. **Run** `copilot inject`. Narrate the printed stages as they appear: the
   injection, the `classified <kind> (confidence) — <rationale>` line, and the
   `agent decision: <action> (confidence) — <reasoning>` line. The mechanics of
   each step are in `references/autonomous-retry.md`.
3. **Show the receipts** — two artifacts get written:
   - `logs/lifecycle-run-NN.json` for **both** the failed attempt and the landed
     retry. The retry's `landed_slot` is cross-referenceable on a Solana explorer.
   - `logs/agent-reasoning.jsonl` — the exact context sent and decision returned,
     one JSON line per call. Open it with the user; this is the proof the decision
     was real model output over live data, not a code path
     (`references/agent-reasoning.md`).
4. **Tie it back to the mechanics.** The right move for an expired blockhash is a
   *fresh blockhash*, not a bigger tip — so a good decision keeps the tip roughly
   where it was unless congestion independently rose. If the agent raised the tip,
   check whether the congestion read justified it. Ground this in
   `../solana-internals/references/blockhash-lifetime.md` and
   `../diagnose/references/tip-strategy.md`.

## Why an expired blockhash is the fault to inject

It's the cleanest teaching failure: a high-confidence, well-understood cause
whose *correct* fix (fresh blockhash) is the opposite of the naive instinct
(pay more). That gap is exactly what a thinking agent should navigate and a
hardcoded rule often gets wrong. The injector ages the blockhash past the
~150-slot window so the doomed attempt genuinely cannot land — only the
agent-chosen retry should.

## Non-negotiables

- **The decision is the model's.** Never describe this as rule-based or
  pre-scripted — the entire point is live data → Claude reasoning → structured
  JSON → execution. Copilot has no local retry logic to fall back on.
- **Don't fake it.** If the agent can't be reached (no session), say so plainly;
  never hand-author a decision to make the demo "work".
- **Don't claim a landing that isn't in the log.** The injected attempt fails by
  design; only the retry should land, and only if `landed_slot` says so.

## References

- `references/autonomous-retry.md` — the full inject → classify → decide → execute
  loop, step by step.
- `references/agent-reasoning.md` — what context the agent gets, what JSON it
  returns, and how to read `agent-reasoning.jsonl`.