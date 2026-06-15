# Reading the watch feed

Line-by-line meaning of `copilot watch`, the patterns that distinguish healthy
from congested from broken, and what's happening under the hood. Use this to turn
a scrolling feed into a clear read for the user.

---

## The line

```
slot 287,431,902 (confirmed 287,431,870, finalized 287,431,839) | leader 7Np…9aF | tip floor p50/p75/p95 = 9,000/18,500/74,000 lamports (Moderate)
```

| Field | Meaning |
|---|---|
| **slot** | the processed tip; advances ~every 400ms |
| **confirmed** | supermajority-voted tip; trails processed by ~1–2 slots |
| **finalized** | rooted tip; trails by ~31 slots (~13s) — structural, not a problem (`../../solana-internals/references/commitment-and-finalization.md`) |
| **leader** | who is producing the current slot |
| **tip floor** | recent *landed* Jito tip percentiles, refreshed ~every 10s |
| **(level)** | congestion read derived from the tip-floor shape |

---

## What's under the hood

The feed is the `geyser` crate's live view, held in lock-free shared state so the
rest of the stack can read it without contention:

- three **atomics** for the processed / confirmed / finalized slot tips, updated
  as the Yellowstone stream pushes slot updates,
- an **ArcSwap** holding the latest `BlockhashInfo` (blockhash + its slot), so any
  reader gets the freshest blockhash without locking — this is the processed
  blockhash bundles are built on (`../../run/references/bundle-construction.md`),
- a **watch channel** broadcasting new slot tips to subscribers (the lifecycle
  tracker's Phase B reads these crossing the landed slot —
  `../../run/references/lifecycle-tracking.md`).

`watch` simply renders this shared state each time it changes. It opens no
submission path — it is strictly read-only.

---

## Healthy vs. congested vs. broken

**Healthy.** All three slots climb steadily; confirmed ~1–2 behind processed,
finalized ~31 behind; tip floor stable; congestion Low/Moderate. Safe to submit;
the oracle's p75 baseline will land.

**Congested (but healthy).** Tip floor rising and/or p95 far above p50 (heavy
tail); congestion High/Severe. The chain is fine — the **auction** is hot. Expect
to tip nearer p75–p95 and to see more retries. This is the moment to either wait
or size tips up before a sensitive run. Note from
`../../solana-internals/references/banking-stage-and-sealevel.md`: votes ride a
protected lane, so you can be in real congestion (hard to land) while the
confirmed delta still looks normal — congestion shows up in the **tip floor and
tail**, not necessarily in the commitment deltas.

**Broken endpoint/cluster.** Processed climbs but **finalized is stuck**, or slots
don't advance at all. Don't submit. Usually a bad gRPC endpoint or a node serving
stale data (`../../setup/references/connectivity.md`). Also watch for "fast but
stale": if blockhashes seem to expire faster than the ~150-slot window should
allow, the RPC/feed may be lagging the true tip (surface latency vs. freshness).

---

## Using congestion before a run

`watch` is the cheap way to read the room before `run`:

- **Low/Moderate** → the oracle's p75 baseline lands fine; proceed.
- **High/Severe** → the baseline is already elevated (p75 rises with the market);
  be ready for the agent to push a retry toward p95 if the first attempt is
  outbid (`../../run/references/tip-pricing.md`, worked examples C/D).

A heavy p95/p99 tail with a flat-ish p50 is the signature of a *contested* auction
— a few bundles paying a lot to get in — which is exactly when an under-tipped
submission gets outbid.

---

## Leader windows

The leader shown rotates every slot. A bundle can only land on a **Jito-connected**
leader's slot, so the more useful question is "when is the next Jito leader?" —
covered in `leader-scheduling.md`. `watch` is where you see leaders go by in real
time; `run` logs the next Jito window before each submission as observability.