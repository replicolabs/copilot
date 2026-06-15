# Lifecycle tracking

How Copilot follows a submission from the moment it's sent to finalized, what
ends up in the log, and why it's all stream-driven. This is the evidence the
whole project is judged on, so the mechanism — not just the output — matters.

---

## The principle: streams, never polling

Copilot confirms landing and commitment **entirely over Yellowstone gRPC
subscriptions**, never by polling `getSignatureStatuses`. Two reasons:

- **Speed.** A subscription pushes the update the instant the node sees it; polling
  adds a round-trip per check and a polling interval of latency.
- **Loss.** Under the congestion where landing is hardest, RPC nodes rate-limit
  and serve stale data (`../../solana-internals/references/transaction-pipeline.md`). A poll
  can miss the window or report a stale "not found." The stream sees the block
  directly.

This is the same reason the `geyser` crate exists: one always-on stream feeds the
shared chain view, and the tracker rides it.

---

## Phase A — landing

When a bundle is submitted, the tracker opens a Yellowstone subscription
**filtered to the exact transaction signature** at **processed** commitment.

- The **first update that arrives is the landing.** It carries the slot the
  transaction was included in → stamp `landed_slot` and `processed_at`.
- If **nothing arrives within the landing deadline**, the transaction **never
  landed**. The tracker returns that outcome, and the caller treats it as an
  expiry/drop for classification (`../../diagnose/references/failure-taxonomy.md`).

Why filter by signature: it's exact and cheap — the node only pushes the one
transaction we care about, the moment it appears, with its slot.

---

## Phase B — commitment

Once `landed_slot` is known, confirmed and finalized are read from the **shared
geyser slot tips** the always-on feed maintains (three atomics: processed,
confirmed, finalized slot — `../../watch/references/reading-the-feed.md`):

- when the **confirmed tip crosses** `landed_slot` → stamp `confirmed_at`,
- when the **finalized tip crosses** `landed_slot` → stamp `finalized_at`.

No extra subscriptions and no polling — the slot tips are already advancing on the
feed; the tracker just watches them pass the landed slot. This mirrors the Bank's
Open→Frozen→Rooted progression for that slot
(`../../solana-internals/references/commitment-and-finalization.md`).

---

## The log entry schema

Each `logs/lifecycle-run-NN.json`:

| Field | Meaning |
|---|---|
| `signature` | the transaction signature — the on-chain identity |
| `bundle_id` | the Jito bundle id (null if submission never returned one) |
| `tip_lamports` | the tip paid for this attempt |
| `submitted_at` | wall-clock at submission |
| `submitted_slot` | chain tip when sent |
| `landed_slot` | slot it landed in — **null means it never landed** |
| `processed_at` / `confirmed_at` / `finalized_at` | timestamps per level (null until reached) |
| `failure` | a classification string, present only when the `fault` crate tagged it |

Three latency **deltas** are derived on demand:
`submitted → processed`, `processed → confirmed`, `confirmed → finalized`
(all saturating, so a missing later stage never produces a negative number).

---

## Why the slot numbers are the proof

`landed_slot` is the **explorer-checkable** fact: anyone — a teammate, a judge —
can paste it into a Solana explorer and see the transaction in that block. This is
why the logs are committed and why Copilot records real slots rather than prose.
The run log is *verifiable*, not a claim. When any summary disagrees with
`landed_slot`, the slot wins (`../../logs/references/interpreting-logs.md`).

---

## Deadlines

- **Landing deadline** ≈ 90s by default. A blockhash lives ~60s; the extra margin
  covers a slow-to-produce leader before concluding the transaction expired. The
  `inject` demo uses a **shorter** landing deadline, since the injected attempt is
  *expected* to fail and there's no point waiting a full window.
- **Finalize deadline** ≈ 45s after landing. Finalized lags ~13s, so this is
  comfortable headroom; if finalized isn't observed within it, the entry is
  reported as **confirmed-only** rather than assumed final.

Both deadlines are configurable on the tracker. The defaults are tuned so a
genuine non-landing is concluded promptly without false negatives, and a genuine
landing is always followed to finality when the network is healthy.

---

## Reading outcomes honestly

- `landed_slot` populated, all three timestamps set → **finalized**. Report the
  deltas.
- `landed_slot` populated, `finalized_at` null → **confirmed-only** (or
  processed-only). Landed, but don't claim finalization that the log doesn't show.
- `landed_slot` null with a `failure` → a genuine non-landing; hand to `diagnose`.

A run that mixes several clean finalized landings with a couple of classified,
recovered failures is *stronger* evidence than an implausible all-green run —
because the failure path (classify → decide → retry → land) is the part that
proves the system actually works (`../../inject/references/autonomous-retry.md`).