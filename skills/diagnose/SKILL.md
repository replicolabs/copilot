---
name: diagnose
description: Diagnose why a submission failed or didn't land, decide whether and how to retry, and tune the tip to the cause. Use when the user asks why a bundle failed, wants a failure classified, suspects the fee was too low, asks whether to retry, wants to tune or raise a tip, or wants to understand the reasoning behind Copilot's (or the agent's) recovery decision.
---

# diagnose

Read a failure the way Copilot does, and reason about the right response — the
same job the retry agent does, made explicit for the user. The discipline here is
matching the fix to the cause, and pricing from data rather than instinct.

> **Routing:** if this isn't the right skill for the request, consult `../SKILL_ROUTER.md` and switch.

## Prerequisites

- A failed lifecycle entry (an unlanded run, or the injected attempt from
  `inject`), or a fresh failure the user just hit.

## Workflow

1. **Gather the signals** for the failure, from the lifecycle log and the run
   output: did it land? blockhash age vs. the ~150-slot window? did a Jito leader
   produce? the Block Engine's verdict? tip vs. the landed median? any on-chain
   error? These are the classifier's inputs
   (`references/failure-taxonomy.md`).
2. **Classify** by the decision order in `references/failure-taxonomy.md`. State
   the kind, your confidence, and — honestly — the alternatives you can't rule
   out. Don't manufacture certainty the signals don't support; ambiguity is a
   valid, important answer here.
3. **Decide the response**, matched to the cause and priced from data
   (`references/tip-strategy.md`):
   - `expired_blockhash` → retry with a **fresh blockhash**; hold the tip unless
     congestion rose.
   - `fee_too_low` → retry with a higher tip toward/above p75–p95, scaled by
     congestion — not a blind overpay.
   - `leader_skipped` → wait briefly, then retry; tip unchanged.
   - `compute_exceeded` → don't retry as-is; the CU limit must change upstream.
   - `bundle_failure` / `dropped` → judge from specifics; when truly ambiguous,
     prefer the conservative action.
4. **If the user wants the agent to decide it live,** that's the `inject` skill /
   `copilot inject`. You can then read `logs/agent-reasoning.jsonl` to explain
   exactly what context the model saw and what it returned
   (`../inject/references/agent-reasoning.md`).

## Non-negotiables

- **Match the fix to the cause.** The classic mistake is throwing more tip at an
  expired blockhash. Call it out wherever you see it — that's the whole reason
  Copilot reasons rather than reflexively retries
  (`../solana-internals/references/blockhash-lifetime.md`).
- **Tips come from the data, not from you.** Reason in terms of the landed
  percentiles and the congestion read, never a number you picked
  (`references/tip-strategy.md`).
- **Preserve ambiguity.** If two causes both fit, say so. The pipeline drops
  transactions silently at several layers; pretending the cause is always knowable
  is the failure mode the whole stack is designed against
  (`../solana-internals/references/transaction-pipeline.md`).

## References

- `references/failure-taxonomy.md` — the kinds, the signals, the decision order,
  and the recovery per kind.
- `references/tip-strategy.md` — how to reason about the retry tip for each cause.