---
name: logs
description: Interpret Copilot's committed lifecycle logs — which bundles landed, how fast they moved through the commitment levels, and how healthy the cluster was. Use when the user wants to read the logs, review a run, check latency or the processed→confirmed delta, see what landed vs. failed, or summarize the run evidence in the logs/ directory.
---

# logs

Turn the `logs/lifecycle-run-*.json` files into a clear picture: what landed, how
fast, and how healthy the cluster was at submission time. This is reading the
evidence the project produces.

> **Routing:** if this isn't the right skill for the request, consult `../SKILL_ROUTER.md` and switch.

## Prerequisites

- A `logs/` directory with at least one `lifecycle-run-NN.json` (written by `run`
  or `inject`).

## Workflow

1. **Summarize** with `copilot logs` (reads every run, prints each one's stage and
   deltas, plus a landed/failed tally), or read the JSON directly for detail.
   Each entry's fields and the derived deltas are documented in
   `references/interpreting-logs.md`.
2. **Explain each run** to the user:
   - **Stage reached** — finalized is the goal; processed/confirmed-only means it
     landed but tracking stopped early or finality lagged.
   - **landed_slot** — the explorer cross-reference. A reviewer can paste it in.
   - **processed → confirmed delta** — the headline health signal. ~400–800ms
     (1–2 slots) is healthy; multi-second points to congestion/lagging
     votes/forks at submission time
     (`../solana-internals/references/commitment-and-finalization.md`).
   - **confirmed → finalized delta** — rooting lag, normally ~13s.
3. **Read the set as a whole.** Count landings vs. failures. A credible run log
   shows several real landings *and* a couple of genuine failures that were
   classified — including the injected one, which should be followed by a landed
   retry. Point out each `failure` field and connect it to the `diagnose` skill.

## Non-negotiables

- Report what the logs actually say. Don't infer a finalized landing from a
  confirmed-only entry, and don't invent slots.
- Slot numbers are real and checkable — treat them as the source of truth over
  any prose summary, including your own.

## References

- `references/interpreting-logs.md` — the entry schema, the deltas, what a healthy
  vs. stressed run looks like, and how the agent-reasoning log lines up with it.