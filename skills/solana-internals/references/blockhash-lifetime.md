# Blockhash lifetime

Every transaction pins a recent blockhash, and that blockhash bounds the
transaction's validity in time. Getting this wrong is the most common reason a
well-formed transaction silently fails to land — and it is precisely the fault
Copilot's demo injects, because the *correct* fix is the opposite of the naive
instinct.

---

## What the blockhash is and does

A transaction's message contains a **recent blockhash** (alongside the account
keys, header, and instructions). It serves two jobs at once:

- **Liveness / expiry.** Transactions can't linger indefinitely; they age out.
- **Replay protection.** Once a blockhash leaves the cluster's recent set, the
  same signed bytes can't be replayed in a later, unrelated context.

The cluster accepts a transaction only while its blockhash is within roughly the
last **150 slots** of the tip — Solana's `MAX_PROCESSING_AGE`. At ~400ms/slot
that's about **60 seconds**. Past that window the cluster has dropped the
blockhash from its recent set and rejects the transaction with
**`BlockhashNotFound`**.

The cluster tracks this via a recent-blockhashes/“block height” mechanism: each
blockhash has a last-valid height, and the transaction is dead once the tip
passes it. (`lastValidBlockHeight` in client APIs is the same idea — the height
beyond which the transaction can no longer land.)

---

## Why `BlockhashNotFound` is NOT a fee problem

This is the single most important operational point in the whole knowledge base,
and the reason the inject demo exists. When a transaction fails because its
blockhash expired:

- It was **never under-priced.** It never got far enough for price to matter — the
  cluster rejected it on *validity*, before any scheduler ordering or Jito
  auction.
- Raising the tip or the priority fee does **nothing** for the next attempt's
  expiry risk.
- The correct fix is a **fresh blockhash**, then resubmit. The tip should change
  only if *congestion independently rose* in the meantime — and if it did, that's
  a separate, congestion-driven decision, not a fix for the expiry.

Mismatching the fix to the cause — throwing more tip at an expired blockhash — is
the canonical mistake, and exactly what an agent that reasons about the *cause*
gets right where a reflex rule ("failed? tip more") gets wrong. See
`../../diagnose/references/tip-strategy.md` and `../../inject/references/autonomous-retry.md`.

---

## Why you must never use a finalized blockhash

Finalized trails the tip by ~31 slots (`commitment-and-finalization.md`). So a
*finalized* blockhash is ~31 slots old the instant you read it — it has already
spent ~20% of its ~150-slot life before you sign. Under any submission delay,
congestion, or retry loop, that head start is enough to push it over the edge
mid-flight.

**Always build against a processed (or at worst confirmed) blockhash.** Copilot
reads the latest blockhash from the live geyser feed at **processed** commitment,
so every transaction starts with the freshest possible blockhash and the maximum
remaining validity window (`../../run/references/bundle-construction.md`).

---

## Blockhash age as a classification signal

Because Copilot knows the slot of the blockhash it used and the current tip, it
can compute the blockhash's **age in slots** at the moment of failure:

- age well past ~150 → expiry is essentially certain → `expired_blockhash` at high
  confidence → fresh-blockhash retry,
- age comfortably within the window → expiry is *not* the cause → look elsewhere
  (fee, leader skip, contention).

This turns a normally-invisible cause into a measurable one, which is why it's
high in the classifier's decision order (`../../diagnose/references/failure-taxonomy.md`).

The fault injector deliberately ages a real blockhash past the window (rotating
its bytes and applying a fixed transform so it is well-formed but stale, with an
apparent age beyond `MAX_PROCESSING_AGE`). The doomed attempt therefore genuinely
cannot land; only the agent-chosen retry, built on a fresh blockhash, should
(`../../inject/references/autonomous-retry.md`).

---

## Durable nonces — the escape hatch (for context)

Sometimes a transaction legitimately needs a long gap between signing and
submission — offline/air-gapped signing, multisig signature collection, scheduled
execution. The ~150-slot window makes a normal blockhash useless there, and there
is **no setting to extend the window**. The standard answer is a **durable
nonce**:

- A **nonce account** (owned by the System Program) stores a **durable
  blockhash** that does *not* expire with the tip.
- The transaction uses that stored nonce value in place of a recent blockhash,
  and its **first instruction must be `AdvanceNonceAccount`**, which consumes the
  current nonce and rotates it to a new value.
- Because the stored nonce only changes when explicitly advanced, the transaction
  stays valid until it's used (or the nonce is advanced by something else).
- Replay protection is preserved: once advanced, the old nonce can't be reused.

Copilot's flows are real-time, so it doesn't use durable nonces — but this is the
correct pointer when someone asks "what if I can't submit within 60 seconds?" The
answer is durable nonces, not a longer window (there isn't one) and not a bigger
tip.

---

## Quick reference

- Validity window: ~**150 slots** (`MAX_PROCESSING_AGE`), ~**60s**.
- Expiry error: **`BlockhashNotFound`**.
- Fix for expiry: **fresh blockhash**, resubmit. *Not* a tip change.
- Never build on a **finalized** blockhash (~31 slots pre-aged).
- Long signing-to-submit gap: **durable nonce account** + `AdvanceNonceAccount`
  first.