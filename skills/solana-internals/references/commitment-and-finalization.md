# Commitment and finalization

Why a transaction moves through three commitment levels, what each actually
guarantees, and why finalized trails the chain tip by a fixed, unavoidable gap.
Copilot timestamps each level for every submission, so this is the reference for
reading those numbers — and for explaining the processed→confirmed delta, the
single best health signal in a lifecycle log.

---

## The three levels

| Level | Guarantee | Typical latency | Use for |
|---|---|---|---|
| **processed** | The transaction was seen in a block by the node you're connected to. Optimistic — the block could still lose a fork race and be dropped. | ~400ms (the slot it landed in) | knowing it landed; the earliest possible signal |
| **confirmed** | A supermajority (≥⅔ of stake) has voted on the block. For practical purposes irreversible. | ~1–2s | the default bar for "it's done" |
| **finalized** | The block is rooted; reverting it would require violating an economically impossible number of vote lockouts. | ~12–15s | genuinely irreversible actions (payouts, settlement) |

These map directly onto the Bank lifecycle: **Open → processed**, **Frozen +
supermajority vote → confirmed**, **Rooted → finalized**
(`banking-stage-and-sealevel.md`).

Copilot records a timestamp and the relevant slot at each level, so every
lifecycle entry carries the full progression and the deltas between stages
(`../../run/references/lifecycle-tracking.md`).

---

## Tower BFT, in enough depth to reason about

Solana's consensus is **Tower BFT**, a Proof-of-History-optimized variant of
Practical BFT. The point of using PoH as a shared clock is to cut the O(n²)
message exchange of classic BFT down toward O(n): validators vote on the PoH hash
chain rather than chattering about ordering, and they compute timeouts *locally*
from the clock instead of waiting on network rounds.

The mechanism that produces finality:

- A validator votes to commit a block (identified by its blockhash + slot). Each
  vote carries the validator's **staked weight**.
- Each vote imposes an **exponential lockout**: after voting for a slot at time
  T, the validator must wait `2^n` slots before it can switch to a competing fork,
  where `n` is the number of consecutive votes it has made on this chain.
- So lockouts **double** with each consecutive vote: **1, 2, 4, 8, 16, 32, …**
  slots. The more a validator commits to a chain, the longer it is economically
  bound to it, which makes frivolous fork-switching prohibitively costly and
  suppresses forks.
- After roughly **32 consecutive votes**, the oldest vote's lockout is ~2³² slots
  — so large it is effectively permanent (on the order of decades). At that point
  the block is **rooted / finalized**.
- A vote is only valid if the PoH hash it votes on is actually present in the
  ledger, and once cast, a validator won't vote for any hash that isn't a
  descendant of that vote for at least the lockout period. Because every node
  computes the same PoH result deterministically, they reach the same conclusion
  without extra communication.

This rolling ~32-vote window is the source of the finalization lag below.

---

## Why finalized lags ~31 slots (and why you can't speed it up)

Because finality requires that rolling window of consecutive votes to accumulate,
a block becomes finalized roughly **31 slots (~13s) after** it is the tip. This
is structural, not a performance bug and not something a tip or a faster endpoint
can change.

**Operational consequences:**
- Don't wait for finalized when timing matters. **Confirmed (~1–2s)** is the
  practical "it's safe" bar; reserve finalized for truly irreversible actions.
- A *finalized* blockhash is already ~31 slots old the moment you read it — it has
  burned ~⅕ of its ~150-slot life before you've even signed. Always build against
  a **processed** or **confirmed** blockhash (`blockhash-lifetime.md`). This is
  the exact trap Copilot's expired-blockhash fault demonstrates.

---

## The processed → confirmed delta: the headline signal

The gap between processed and confirmed is the best read on **how healthy the
cluster was at the moment you submitted**:

- **Tight (~400–800ms, 1–2 slots):** votes are landing fast; your block was on
  the winning fork immediately. Healthy.
- **Wide (several seconds):** votes are lagging — congestion, a fork contest, or
  validators drifting behind the tip. The transaction still confirmed, but the
  cluster was under stress.

A consistently wide delta across a whole run is a signal to be more conservative
(tip nearer the top of the landed range, expect more retries), not evidence of a
per-transaction bug. Note an important asymmetry from `banking-stage-and-sealevel.md`:
votes ride a protected pipeline, so during user-load spikes **inclusion can get
hard while the confirm delta stays normal** — landings fail or need retries even
though the transactions that *do* land confirm quickly. Reading those two facts
together (hard to land, but normal deltas) points at QoS/contention/expiry rather
than a consensus problem.

---

## Optimistic confirmation and rollback risk

`processed` is **optimistic**: the node has seen the transaction in *a* block, but
that block could still lose a fork race within the rollback window and be
discarded, taking the transaction with it. Each subsequent vote on that fork
doubles the real time the network would have to stall to unwind it, so the
practical risk drops fast — by `confirmed` (supermajority) it's negligible for
almost everything, and by `finalized` it's gone. This is why Copilot treats a
processed-only entry as "landed, but don't claim it as final," and reports the
actual stage reached rather than rounding up (`../../logs/references/interpreting-logs.md`).

---

## How Copilot observes all this

Copilot does **not** poll `getSignatureStatuses`. It confirms landing by watching
the signature appear on a Yellowstone stream (processed, carrying the landed
slot), then reads confirmed and finalized from the shared geyser slot tips as
they cross the landed slot. Stream subscriptions, not RPC polling — polling is
both slower and lossier under exactly the congestion where it matters most. Full
mechanics in `../../run/references/lifecycle-tracking.md`.