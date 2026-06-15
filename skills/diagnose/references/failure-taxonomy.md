# Failure taxonomy

How Copilot's `fault` crate classifies a non-landing or failed submission. It
reads hard signals where it can and **surfaces ambiguity** where it can't — and
that ambiguity is precisely what the retry agent reasons over. This is the
reference for the kinds, the signals, the decision order, and the recovery per
kind.

---

## Why classification (not just retrying)

A non-landing transaction usually can't tell you *why* — the pipeline drops
transactions at several layers with no feedback
(`../../solana-internals/references/transaction-pipeline.md`, section 4). The
right response differs completely by cause: a fresh blockhash, a higher tip, a
wait, or an upstream fix. So Copilot first infers the most likely cause from what
*is* observable, attaches a confidence and the alternatives it couldn't rule out,
and lets the **agent** decide the response. Classification explains; it does not
decide.

---

## The signals

Collected for each failure from across the stack:

| Signal | Source | Tells us |
|---|---|---|
| **landed** | lifecycle tracker | was it ever seen at processed? |
| **blockhash_age_slots** vs **~150** | submitted slot vs. tip | did the blockhash expire? (`../../solana-internals/references/blockhash-lifetime.md`) |
| **jito_leader_produced** | leader tracking | did a Jito leader actually produce the target window? (`../../watch/references/leader-scheduling.md`) |
| **outcome** | Block Engine status | submitted / failed / rejected / landed (`../../solana-internals/references/jito-bundles.md`) |
| **tip_lamports** vs **recent_landed_tip_p50** | oracle | was the tip competitive? (`tip-strategy.md`) |
| **onchain_error** | tracker / RPC | if it landed but failed (compute exceeded, insufficient funds, instruction error) |

Most of these are *absences* (no landing, no produced block) rather than
messages. The classifier's job is to infer cause from the pattern.

---

## The kinds

| Kind | Meaning | Typical fix |
|---|---|---|
| `expired_blockhash` | blockhash aged past the ~150-slot window before inclusion | **fresh blockhash**, resubmit; raise tip only if congestion also rose |
| `fee_too_low` | tip below what's recently been landing | raise toward/above the landed percentiles, scaled by congestion |
| `compute_exceeded` | landed but exhausted its CU budget | not retryable as-is — raise the CU limit upstream or abort |
| `bundle_failure` | Block Engine failed/rejected it (lost auction, simulation failure) | depends; a sim failure is usually construction, a lost auction is a tip question |
| `leader_skipped` | the target Jito leader produced no block; bundle dropped | transient — wait for a producing Jito leader; tip unchanged |
| `dropped` | never landed, no single cause dominant | genuinely ambiguous — the agent decides |
| `unknown` | not classifiable / not actually a failure | — |

---

## The decision order

The classifier ranks by **signal strength**, highest-confidence first, and stops
at the first that fits:

1. **on-chain error present** → authoritative. `compute_exceeded` at confidence
   **1.0** when the error says so; other on-chain errors (insufficient funds,
   instruction error) classified accordingly. (If it landed and *failed* on-chain,
   that's decisive — no inference needed.)
2. **landed with no error** → **not a failure** at all; stop.
3. **blockhash aged past the window** → `expired_blockhash` (~**0.9**). Decisive
   because age is measured directly against `MAX_PROCESSING_AGE`.
4. **no Jito leader produced** the target window → `leader_skipped` (~**0.8**).
5. **Block Engine reported failed/rejected** → `bundle_failure` (~**0.7**).
6. **tip below the recent landed median** → `fee_too_low` (~**0.6**).
7. **none decisive** → `dropped` (~**0.4**), with the surviving alternatives
   listed.

The ordering reflects how *trustworthy* each signal is: an on-chain error is
ground truth; a measured blockhash age is nearly so; a tip-below-median is
suggestive but not conclusive (you can be under the median and still land, or
above it and still get outbid on a hot account). Hence the descending confidence.

---

## Confidence, rationale, alternatives — and why they exist

Every classification carries a **confidence**, a **rationale**, and the
**alternatives** it couldn't rule out. This shape is the whole point:

- **High confidence + clear cause** (an on-chain compute error, a decisively-aged
  blockhash) → warrants a **decisive** action.
- **Low confidence + live alternatives** (`dropped`) → warrants a **conservative**
  action, and is exactly the case where the agent's judgement over the full live
  context earns its keep.

The classifier never manufactures certainty the signals don't support. Preserving
ambiguity honestly is a feature — it's the antidote to the "every failure looks
the same, so just retry harder" trap that the silent multi-stage filter sets.

---

## Recovery per kind (summary; depth in tip-strategy.md)

- `expired_blockhash` → fresh blockhash, **hold tip** (unless congestion rose).
- `fee_too_low` → fresh blockhash + **raise tip** toward p75–p95 by congestion.
- `leader_skipped` → **wait** for a producing Jito leader, hold tip.
- `compute_exceeded` → **don't retry as-is**; fix the CU limit upstream or abort.
- `bundle_failure` → if simulation failed, it's **construction** (fix the tx); if
  the auction was lost, treat like `fee_too_low`.
- `dropped` → the **conservative** retry: fresh blockhash at ~p75, then reassess.
- `unknown` / not-a-failure → no action.

The discipline throughout: **match the fix to the cause**, and price any tip
change from the live data, never instinct (`tip-strategy.md`). The classic
mistake the agent must avoid is treating every non-landing as under-pricing.