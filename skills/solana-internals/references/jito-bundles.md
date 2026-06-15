# Jito bundles

Copilot submits through Jito, so the bundle path is essential to explaining what
lands, what doesn't, and why the tip is priced the way it is. This covers the
full path: bundle semantics, the Block Engine vs. the relayer, the auction, tip
accounts and the tip-last rule, regional endpoints, statuses, and the
Jito-leader-only constraint.

---

## What a bundle is

A **bundle** is an ordered list of **1–5 transactions** executed **atomically and
all-or-nothing** by a Jito-enabled leader: either every transaction lands, in the
given order, in a single block, or none do. No partial execution, no reordering.
That makes bundles the natural unit for sequenced actions (arbitrage,
liquidations, "do A then B atomically"), and a clean unit for Copilot to submit
and track.

Copilot enforces the 1–5 size rule **locally** before any network round-trip, so
an oversized bundle fails fast with a clear error rather than a remote rejection.

---

## The Jito stack: relayer, Block Engine, ShredStream

Three pieces matter:

- **Relayer** — a transaction proxy that sits in front of a Jito validator's TPU.
  It receives transactions, can hold them briefly, and forwards them, providing a
  controlled ingress (and a place where bundles and regular traffic meet the
  leader).
- **Block Engine** — the auction brain. It receives bundles from searchers,
  **simulates them off-chain** to check they execute and to evaluate their compute
  footprint and tip, runs a **high-speed auction** for contested blockspace, and
  forwards the **winning set** to the current Jito-connected leader to include.
- **ShredStream** — a low-latency shred-delivery service that gets block data to
  subscribers faster than normal Turbine propagation. (Not on Copilot's submit
  path, but part of why Jito infrastructure is latency-relevant; useful context if
  asked.)

Copilot talks to the **Block Engine** (`COPILOT_BLOCK_ENGINE`), submitting bundles
and querying their status.

---

## The auction: why the tip is the lever

You do **not** pay the leader directly. Inclusion is decided by the Block Engine's
**off-chain auction**:

1. Submit the bundle (base64-encoded transactions) to the Block Engine.
2. The engine **simulates** it off-chain — execution check, compute/profit
   evaluation, tip read.
3. Competing bundles are ranked (by tip, subject to simulation/validity), and the
   winning set is forwarded to the current Jito leader.

So **the tip is what wins inclusion**, not the priority fee (which is the lever in
the *normal* fee market — `fees-and-compute.md`). Tip what recently-landing
bundles paid (the landed-tip floor), scaled by how contested the moment is. This
is the entire basis of Copilot's tip oracle (`../../run/references/tip-pricing.md`).

This is also distinct from raw-TPU submission and SWQoS: the auction lets a
low-stake sender win contested blockspace by *paying*, rather than by holding
stake (`quic-and-swqos.md`). It's a core reason Copilot submits through Jito.

---

## Tip accounts and the tip-last rule

- The tip is an ordinary **SOL transfer to one of Jito's rotating tip accounts**,
  fetched live via `getTipAccounts`. Copilot **picks one at random** from that set
  to avoid piling write-lock contention onto a single tip account (which would be
  a hot account in the Sealevel sense — `banking-stage-and-sealevel.md`).
- The protocol minimum tip is **1,000 lamports**; in practice you tip at/above the
  recent landed floor. Copilot clamps any chosen tip to ≥1,000 and fails the build
  fast if asked for less.
- The tip transfer **must be the last instruction of the last transaction** in the
  bundle. Bundles are all-or-nothing, but the tip-last placement ensures that if a
  fork drops the bundle you haven't paid the tip for work that didn't land. Tying
  the tip to the bundle's success is the whole point. Copilot's builder always
  appends the tip last (`../../run/references/bundle-construction.md`).

---

## Why a bundle only lands on a Jito-leader slot

A bundle can only be included by a leader running Jito-enabled validator software.

- If the slot's leader **isn't Jito-connected**, the bundle isn't included there.
- If the targeted Jito leader **skips its slot** (produces no block at all), the
  bundle is **silently dropped** — there's nothing on-chain to see, and the next
  leader may not be Jito-connected either.

This is why Copilot tracks upcoming **Jito-connected** leader windows (best-effort,
via `getConnectedLeaders`) and why `leader_skipped` is a distinct failure kind
that must **not** be confused with a fee problem — the fix is to wait for a
producing Jito leader, not to tip more (`../../diagnose/references/failure-taxonomy.md`,
`../../watch/references/leader-scheduling.md`).

---

## Confirming a bundle landed — two views

Copilot uses both, and trusts the chain as authoritative:

- **Block Engine status** (`getInflightBundleStatuses` / `getBundleStatuses`) — the
  engine's own fast view of a bundle id: `Invalid` (not a known/valid bundle),
  `Pending` (in flight), `Failed` (lost the auction or failed simulation), or
  `Landed { slot }`. Fast, but it's the engine's view, not proof of chain state.
- **The chain itself** — authoritative. Copilot watches the transaction's
  **signature** on a Yellowstone stream; when it appears, the bundle landed, and
  the landed slot is the explorer-checkable proof
  (`../../run/references/lifecycle-tracking.md`).

A `Failed`/`Invalid` engine verdict feeds the classifier as `bundle_failure`
(often a lost auction or a simulation failure — the latter usually a construction
problem, not a price one).

---

## MEV context (brief)

The Block Engine is MEV infrastructure: simulating bundles off-chain to evaluate
profitability and form non-conflicting "entries" is exactly what lets searchers
compete for ordering (arbitrage, liquidations) without a public mempool to
front-run in. Copilot isn't an MEV strategy — it's transaction *infrastructure* —
but it rides the same rails, which is why the tip/auction model applies directly.

---

## How this maps to Copilot

- `bundle` builds `[set_cu_limit, set_cu_price, ...payload, tip_transfer]`, encodes
  base64, submits to the Block Engine, and exposes the status views above.
- `tip-oracle` prices the tip from the landed floor + congestion; the **tip**, not
  the priority fee, is what the auction reads.
- `inject` breaks the **blockhash** (not the tip) so the bundle can't land, then
  lets the agent reason that the recovery is a fresh blockhash — the demo's whole
  point is that the *cause* (expiry), not the lever people reach for (tip), drives
  the fix.