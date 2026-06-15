# Connectivity: endpoints, healthy output, and failures

What each endpoint feeds, how to read a healthy `watch`, and how to diagnose the
common connection failures — so "is it connected?" gets a definite answer before
anyone submits.

---

## What each endpoint feeds

| Variable | Feeds | Why it matters |
|---|---|---|
| `COPILOT_GRPC_URL` (+ `COPILOT_GRPC_X_TOKEN`) | the `geyser` feed (slots, blocks, blockhash) and the per-signature lifecycle tracker | **the heart of the stack.** Landing and commitment are observed over this stream, not by RPC polling. If this is wrong, nothing tracks. |
| `COPILOT_RPC_URL` | the tip oracle (`getRecentPrioritizationFees`) and the leader tracker (`getLeaderSchedule`, `getEpochInfo`) | prices tips and resolves upcoming leader windows |
| `COPILOT_BLOCK_ENGINE` | the `bundle` submitter | where bundles are auctioned and submitted; defaults to the Jito mainnet engine |
| `COPILOT_KEYPAIR` | the payer/signer | signs and pays for bundles; fund it lightly (~0.1 SOL) |

A provider like solinfra gives RPC + Yellowstone gRPC from one source (often
whitelisted by IP or domain), which is the simplest starting setup. The gRPC
endpoint is the one to get right first — it's both the chain view and the landing
confirmation.

---

## Why gRPC (Yellowstone) and not RPC polling

The whole stack is built on a **push** stream rather than **pull** polling, for
the reasons in `../../run/references/lifecycle-tracking.md`: a subscription
delivers the update the instant the node sees it, and it doesn't suffer the
rate-limiting and staleness that hit RPC nodes under congestion. RPC is kept only
for the supporting reads (prio fees, leader schedule) where a slightly stale
answer is fine.

---

## What healthy `copilot watch` looks like

```
slot 287,431,902 (confirmed 287,431,870, finalized 287,431,839) | leader 7Np…9aF | tip floor p50/p75/p95 = 9,000/18,500/74,000 lamports (Moderate)
```

Healthy means:
- the **processed slot climbs** roughly every 400ms,
- **confirmed** trails it by ~1–2 slots and **finalized** by ~31 slots, and both
  keep climbing in lockstep,
- a **current leader** is shown, and
- a **tip floor** appears after the first oracle refresh.

Read these against `../../watch/references/reading-the-feed.md` for the full
interpretation (healthy vs. congested vs. unhealthy).

---

## Diagnosing failures

| Symptom | Likely cause | Fix |
|---|---|---|
| `watch` hangs, no slots ever appear | wrong `COPILOT_GRPC_URL`/`x-token`, or the endpoint isn't a Yellowstone gRPC endpoint, or your IP/domain isn't whitelisted | verify the gRPC URL + token with the provider; confirm whitelisting. **Most common setup error.** |
| slots climb but no tip floor | RPC unreachable or rate-limited | check `COPILOT_RPC_URL`; tips can't be priced without it |
| processed climbs but **finalized is stuck** | endpoint serving stale/partial data, or the cluster is unhealthy | don't submit on it; switch endpoint or wait |
| `run`/`inject` errors on the keypair | `COPILOT_KEYPAIR` isn't a valid 64-byte keypair file or base58 secret, or the path is wrong | fix per `environment.md` |
| bundles submit but never land, feed otherwise healthy | not a connectivity problem — this is a *landing* question (mainnet? congestion? leader skip?) | go to `../../diagnose/SKILL.md` |

---

## Freshness vs. surface latency

A subtle trap: an RPC node can answer an HTTP request **quickly** while serving
data several slots **stale**. Fast response ≠ fresh data. This is why Copilot
trusts the gRPC stream for the slot/blockhash view and treats RPC as a supporting
feed. If `watch` looks healthy but submissions behave oddly (e.g. blockhashes
seem to expire faster than expected), suspect a **laggy RPC** before suspecting
the stack — the tip/leader reads may be lagging the true tip.

---

## The pre-submit checklist

1. `copilot --version` prints (binary installed).
2. `.env` has `COPILOT_GRPC_URL`, `COPILOT_RPC_URL`, `COPILOT_KEYPAIR` set
   (`environment.md`).
3. `copilot watch` shows processed/confirmed/finalized **all advancing**, a
   current leader, and a tip floor.
4. On **mainnet** if landings are expected (Jito only lands on Jito-leader slots,
   which don't exist on devnet — `environment.md`).

Only when 1–4 hold should you proceed to `../../run/SKILL.md`.