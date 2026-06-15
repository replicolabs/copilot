# Tip pricing

Copilot never hardcodes a tip. Every tip is priced from two live feeds, and the
agent adjusts it on a retry. This is the reference for how the number is derived,
how to explain it, and worked examples across congestion regimes.

---

## The two live feeds

### 1. Jito landed-tip floor
`GET https://bundles.jito.wtf/api/v1/bundles/tip_floor` returns the percentile
distribution of tips that **recently won** their auction:

```
p25, p50, p75, p95, p99   (+ an EMA of p50)
```

Values come back in **SOL** and are converted to **lamports** at the edge. This
is the going rate for inclusion *right now* — what bundles that actually landed
just paid. It is the primary input, because the Jito **auction**, not the normal
fee market, decides whether a bundle lands (`../../solana-internals/references/jito-bundles.md`).

### 2. Recent prioritization fees
`getRecentPrioritizationFees` over RPC, summarized into the same percentile
shape. This reflects the **normal fee market** (the PrioGraphScheduler's ordering
lever — `../../solana-internals/references/fees-and-compute.md`) and is sourced from the
validators' `PrioritizationFeeCache` (`../../solana-internals/references/banking-stage-and-sealevel.md`).
It's a secondary input: it tells you how contested ordinary blockspace is, which
informs the CU price Copilot sets and the congestion read, but it isn't what wins
the bundle auction.

---

## The congestion read

From the **shape** of the landed-tip distribution the oracle derives a level —
**Low / Moderate / High / Severe** — from two cues:

- **tip tail ratio** — how far the high percentiles (p95/p99) sit above the
  median (p50). A heavy tail means a handful of bundles are paying a lot to get
  in: the auction is contested even if the median looks calm.
- **median rising** — whether p50 is trending upward over recent samples.

This is a *shape* read, not a magic threshold. It tells the agent **how hard the
auction is** without dictating an amount. (It also lines up with what the `watch`
feed shows in real time — `../../watch/references/reading-the-feed.md`.)

---

## The baseline

The oracle's suggested starting tip is the **p75 landed tip** — what the top
quartile of recently-landed bundles paid. It's a sensible default for actually
getting in without overpaying, and it's fully data-driven: cheap market → small
p75; hot market → p75 rises on its own. `copilot run` uses this baseline directly
unless `--tip` overrides it.

---

## Who decides on a retry

A **retry** tip is the agent's call, not the oracle's. The agent receives the
failure classification, the tip that was paid, the landed-tip percentiles, and
the congestion read, then reasons cause-first
(`../../inject/references/agent-reasoning.md`, `../../diagnose/references/tip-strategy.md`):

- **expired_blockhash** → usually **hold** the tip; the problem was the blockhash,
  not the price. Raise only if congestion *also* climbed.
- **fee_too_low** → raise toward/above **p75–p95**, scaled by congestion.
- **leader_skipped** → hold the tip; wait for a producing Jito leader.
- **compute_exceeded** → a tip change can't fix it.

Any chosen tip is clamped to the **1,000-lamport** protocol minimum.

---

## Worked examples

**A. Calm market, first submission.**
Floor: p50 8k, p75 16k, p95 40k lamports; tail ratio modest, median flat →
**Low**. Oracle baseline = **p75 ≈ 16k**. `run` submits at 16k. It lands. Nothing
to explain beyond "tipped the top-quartile rate in a calm auction."

**B. Expired-blockhash failure, market unchanged.**
Paid 16k, failed; blockhash age measured at ~180 slots → `expired_blockhash`
(high confidence). Floor still p75 16k, congestion still Low. Correct decision:
**retry with a fresh blockhash, hold tip at ~16k.** Raising it would be treating a
validity failure as a pricing failure — the canonical mistake.

**C. Fee-too-low failure, moderate congestion.**
Paid 9k; landed p50 is 12k so the tip was under the median → `fee_too_low`. Floor
p75 18k, p95 60k; tail moderate → **Moderate**. Correct decision: **raise toward
p75 (~18k)** — enough to clear the current landing bar without chasing the tail.

**D. Fee-too-low failure, severe congestion.**
Paid 18k; p50 has jumped to 22k, p95 110k, p99 300k, tail very heavy, median
rising → **Severe**. The bar moved. Correct decision: **raise toward p95 (~110k)**
— in a hot auction p75 may no longer land, but jumping to p99 (300k) overpays for
the extreme tail. Aim to clear the bar, not to win at any cost.

**E. Leader-skipped failure.**
Paid 16k; no Jito leader produced the target window → `leader_skipped`. Floor
unchanged. Correct decision: **hold 16k, wait** for the next Jito-leader slot.
Raising the tip buys nothing — there was no auction to lose.

---

## How to explain any number

Always in terms of the **landed percentiles + congestion**, never a number you
picked: *"p75 of recently-landed tips is ~18,500 and the tail is heavy (High), so
the oracle suggested ~18,500; the agent held there because the failure was an
expired blockhash, not under-pricing."* If you can't point to a percentile and a
congestion state, you're guessing — and guessing at tips is exactly the behavior
Copilot replaces.