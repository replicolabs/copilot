---
name: watch
description: Stream live slot, leader, and tip-floor information from the chain in real time. Use when the user wants to monitor the chain, see the current slot or leader, check the Jito tip floor, gauge whether the network is congested before submitting, or sanity-check that Copilot's feed is connected and healthy.
---

# watch

Surface what Copilot's live feed sees right now. Read-only — it submits nothing.
Use it to sanity-check the setup, gauge congestion before a run, or watch leader
windows go by.

> **Routing:** if this isn't the right skill for the request, consult `../SKILL_ROUTER.md` and switch.

## Prerequisites

- `.env` configured — at minimum `COPILOT_GRPC_URL` (slots/leader) and
  `COPILOT_RPC_URL` (tips).

## Workflow

1. **Run** `copilot watch`. On each new slot it prints:
   - processed / confirmed / finalized slots (all three should climb together),
   - the current leader,
   - and, refreshed periodically, the Jito tip floor (p50/p75/p95) and the
     congestion level.
2. **Interpret it for the user** (full guide in `references/reading-the-feed.md`):
   - confirmed trailing processed by ~1–2 slots and finalized by ~31 slots, all
     climbing = healthy. A stalled finalized slot = endpoint/cluster trouble.
   - a rising tip floor or a heavy p95/p99 tail = the auction is heating up;
     submissions will need a more competitive tip.
3. **Stop** with Ctrl-C when done.

## Non-negotiables

- Read-only. Never submit from this skill — that's the `run` skill.
- Explain the numbers from the references, not from improvisation. A "delta" or a
  "percentile" has a specific meaning; get it right
  (`../solana-internals/references/commitment-and-finalization.md`,
  `../run/references/tip-pricing.md`).

## References

- `references/reading-the-feed.md` — line-by-line meaning, the shared-state
  internals, healthy vs. congested vs. broken patterns, and using congestion
  before a run.
- `references/leader-scheduling.md` — the epoch leader schedule, slot→leader
  lookups, Jito-connected leader windows, and why a skipped leader is a distinct
  failure from under-pricing.