# Interpreting lifecycle logs

The schema, the deltas, the patterns that distinguish a healthy run from a
stressed one, the injected pair, and how the lifecycle logs line up with the
agent-reasoning log. Use this to turn `logs/` into a clear, honest read.

---

## The entry schema

Each `logs/lifecycle-run-NN.json` is one submission
(`../../run/references/lifecycle-tracking.md`):

| Field | Meaning |
|---|---|
| `signature` | the transaction signature — its on-chain identity, explorer-searchable |
| `bundle_id` | the Jito bundle id (null if submission never returned one) |
| `tip_lamports` | the tip paid for this attempt |
| `submitted_at` | wall-clock at submission |
| `submitted_slot` | chain tip when sent |
| `landed_slot` | slot it landed in — **null means it never landed** |
| `processed_at` / `confirmed_at` / `finalized_at` | timestamps per level (null until reached) |
| `failure` | a classification string, present only when the `fault` crate tagged it |

`copilot logs` reads every run, prints each one's stage + deltas, and a
landed/failed tally; or read the JSON directly for full detail.

---

## The three deltas and what they mean

Derived (saturating) from the timestamps:

- **submitted → processed** — time from send to first inclusion. Short = found a
  Jito leader fast. Long (but eventually landed) = waited for a producing Jito
  window (`../../watch/references/leader-scheduling.md`).
- **processed → confirmed** — *the headline.* ~400–800ms (1–2 slots) = healthy;
  multi-second = the cluster was stressed at that moment (congestion, lagging
  votes, fork contest) (`../../solana-internals/references/commitment-and-finalization.md`).
- **confirmed → finalized** — rooting lag, ~13s normally. Structural, not a
  problem.

Stage reached: **finalized** is the goal; **confirmed/processed-only** means it
landed but tracking stopped early or finality lagged — report it as such, never
round up.

---

## What a healthy run looks like

- Most entries reach **finalized** with a tight processed→confirmed delta.
- `landed_slot` populated and roughly monotonic across the run.
- Tips track the oracle baseline and the congestion of the moment.

## What a stressed run looks like

- Wide processed→confirmed deltas across several entries (cluster under load).
- One or more entries with `landed_slot: null` and a `failure` — genuine
  non-landings. Recall the asymmetry: under user-load spikes, **inclusion can be
  hard while the confirm delta stays normal** (votes ride a protected lane —
  `../../solana-internals/references/banking-stage-and-sealevel.md`). So "several
  failures to land, but the ones that landed confirmed quickly" points at
  QoS/contention/expiry, not a consensus problem.

A run that mixes clean finalized landings with a couple of **classified,
recovered** failures is *stronger* evidence than an implausible all-green run —
because the failure path (classify → decide → retry → land) is the part that
proves the system works.

---

## The injected pair

`copilot inject` produces a recognizable two-entry signature:

1. a **failed** entry — `landed_slot: null`, `failure: expired_blockhash` (or
   similar), the doomed attempt;
2. immediately after, a **landed** entry — the agent-chosen retry, with a real
   `landed_slot`.

Cross-reference with `logs/agent-reasoning.jsonl`: the reasoning line between the
two should explain the refresh-and-resubmit decision and the tip call. Together
they are the end-to-end proof — real failure, real model decision, real recovery
(`../../inject/references/autonomous-retry.md`).

---

## A worked read

Suppose `copilot logs` shows 12 entries: 10 landed→finalized with
processed→confirmed deltas mostly ~500–700ms; entry 06 has `landed_slot: null`,
`failure: fee_too_low`, tip 9,000; entry 07 landed, tip 18,000, finalized,
delta ~600ms; entry 11 has `landed_slot: null`, `failure: expired_blockhash`,
tip 16,000; entry 12 landed, tip 16,000, finalized.

Read it back: *"10 of 12 landed cleanly and finalized with healthy ~600ms
confirm deltas, so the cluster was in good shape. Entry 06 was under-tipped
(9,000 below the landed median) and didn't land; the next attempt (07) raised to
18,000 and landed — a fee-too-low correction. Entry 11 was the injected expired
blockhash; it failed by design, and entry 12 is the agent's retry — same tip
(16,000, correctly held, since expiry isn't a pricing problem) on a fresh
blockhash, which finalized. The two failures are exactly the classify-and-recover
path working."* Every claim there is anchored to a field or a slot, not a vibe.

---

## Reconciling prose with slots

If any summary — the tool's, the agent's, or your own — disagrees with the raw
`landed_slot`, **the slot wins.** Slots are checkable on an explorer; prose isn't.
Always quote the slot when claiming a landing, and never infer a finalized landing
from a confirmed-only entry or invent a slot that isn't in the file.