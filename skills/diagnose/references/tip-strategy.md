# Tip strategy on a retry

The tip is the lever that wins a Jito bundle auction — but only when the failure
was actually about price. The discipline is to change the tip **only** for the
causes where price is the lever, and to keep it where the data says otherwise.
This is where most naive retry logic goes wrong, and the judgement the agent is
there to get right.

---

## The principle

Match the response to the cause, and price every change from the **live landed-tip
percentiles** (p25/p50/p75/p95/p99) and the **congestion read**, never a guess.
Those come from the same feeds `run` uses (`../../run/references/tip-pricing.md`).
"Tune the tip" means *move it to a data-justified point*, not *make it bigger*.

Two facts anchor all of it:
- The **Jito tip wins the auction**; the priority fee is a separate lever in the
  normal fee market (`../../solana-internals/references/fees-and-compute.md`,
  `../../solana-internals/references/jito-bundles.md`).
- Many failures aren't about price at all (expiry, leader skip, compute,
  contention), and for those a tip change is wasted money or actively the wrong
  move.

---

## Per-cause strategy

### expired_blockhash — keep the tip (usually)
The transaction never lost an auction; it was rejected on **validity** before
price mattered. Refresh the blockhash and resubmit at roughly the same tip. Raise
it **only** if the congestion read climbed between attempts — and if so, frame the
raise as a congestion response, not a fix for the expiry. Throwing more tip at an
expired blockhash is the canonical mistake
(`../../solana-internals/references/blockhash-lifetime.md`).

### fee_too_low — raise decisively, but to a point
The paid tip sat below the landed median, so it was outbid. Move toward/above the
landing bar:
- **Moderate** congestion → aim around **p75** (the standard landing bar),
- **High/Severe** (heavy p95/p99 tail) → aim nearer **p95**.
Don't jump to p99 reflexively — that's overpaying for the extreme tail. The goal
is to clear the *current* bar, which the percentiles tell you exactly.

### leader_skipped — keep the tip, wait
No block was produced for the target Jito window, so price had nothing to do with
it. **Hold the tip** and retry into the next Jito-leader window
(`../../watch/references/leader-scheduling.md`). Raising the tip just overpays the
next attempt for no benefit — there was no auction to lose.

### compute_exceeded — don't touch the tip; don't retry as-is
It landed and failed on its **CU budget**. No tip changes that. The CU limit (or
the work) must change upstream, or abort
(`../../solana-internals/references/fees-and-compute.md`).

### bundle_failure — read the verdict
- **Simulation failed** → almost always a **construction** problem (a bad
  instruction, a bad account). Fix the transaction; a tip change won't help
  (`../../run/references/bundle-construction.md`).
- **Lost the auction** → treat like `fee_too_low`: raise toward p75–p95 by
  congestion.

### dropped — conservative, then reassess
Genuinely ambiguous. The honest move is a single **fresh-blockhash retry at ~p75**
and then reassess, rather than escalating blindly. This low-confidence case is
exactly where reasoning over the full live context beats a fixed rule.

---

## Worked numeric examples

Landed-tip floor and congestion drive the number. Same five scenarios as the
agent examples, shown as the *strategy* an operator (or the agent) should apply:

| Cause | Paid | Floor (p50/p75/p95/p99) | Congestion | Decision |
|---|---|---|---|---|
| expired_blockhash | 16k | 8k/16k/40k/90k | Low | fresh blockhash, **hold 16k** |
| fee_too_low | 9k | 12k/18k/60k/150k | Moderate | fresh blockhash, **raise → ~18k (p75)** |
| fee_too_low | 18k | 22k/40k/110k/300k | Severe | fresh blockhash, **raise → ~110k (p95)**, not 300k |
| leader_skipped | 16k | 8k/16k/40k/90k | Low | **hold 16k, wait** for a Jito leader |
| compute_exceeded | 16k | — | — | **abort / fix CU upstream**; tip irrelevant |

Note the third row: in a Severe auction, **p75 may no longer land** — the bar has
moved up — so the correct raise is toward p95. But p99 (300k) is the extreme tail;
paying it "to be safe" is exactly the overpay the data lets you avoid.

---

## How to justify a tip out loud

Always in terms of the data: *"the paid tip was ~9,000, below the landed p50 of
~12,000, and congestion is Moderate, so raise to ~p75 (~18,000) to clear the
current bar."* Or, for a hold: *"the failure was an expired blockhash, not
under-pricing, and the tip already sat at p75 in a calm market, so refresh the
blockhash and keep the tip."* If you can't point to a **percentile and a
congestion state**, you're guessing — and guessing at tips is precisely the
behavior Copilot replaces.

---

## The one-line discipline

**Change the tip only for `fee_too_low` and a lost-auction `bundle_failure`,
scaled by congestion to clear the bar; for everything else, fix the actual cause
and leave the tip alone.**