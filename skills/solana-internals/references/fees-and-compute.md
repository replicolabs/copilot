# Fees and compute

How blockspace is metered and priced on Solana — the scarce resource a
transaction competes for, and the two distinct prices it pays. This underlies
the ComputeBudget instructions Copilot sets on every bundle and the priority-fee
half of the tip oracle.

---

## Compute units (CU)

Every instruction consumes **compute units**. Three numbers anchor everything:

| Limit | Value |
|---|---|
| Default budget per instruction | 200,000 CU |
| Max per **transaction** | 1,400,000 CU |
| Max per **block** | 48,000,000 CU |

The **per-block 48M-CU cap is the scarce network resource** every slot — "the
block is full" means this ceiling was reached, and the scheduler is picking the
highest-fee non-conflicting set that fits (`banking-stage-and-sealevel.md`).

Rough per-operation costs (orders of magnitude, not contractual):

| Operation | ~CU |
|---|---|
| SHA-256 hash | ~100 |
| Ed25519 signature verify | ~800 |
| Log byte (`msg!`) | ~5/byte |
| CPI overhead | ~1,000 |
| Account creation | ~3,000–5,000 |
| SPL token transfer | ~3,000–5,000 |
| Complex DeFi swap | ~100,000–400,000 |

You declare the CU ceiling with a ComputeBudget **`SetComputeUnitLimit`**
instruction. If execution exceeds the requested limit, the transaction **fails
with a compute error** — and retrying the *same* transaction won't help: the
limit (or the work) must change. That's why Copilot classifies `compute_exceeded`
as not-retryable-as-is (`../../diagnose/references/failure-taxonomy.md`). Copilot's
tip-only bundle does only a transfer plus the two budget instructions — a few
hundred CU — so it sets a small, safe ceiling.

**Right-sizing matters both ways:** over-request and you overpay (priority fee =
price × units, below) and crowd the block; under-request and you fail. The
standard technique is to **simulate** the transaction, read `unitsConsumed`, and
set the limit to that plus a ~10% buffer.

---

## The two prices

### Base fee
A flat **5,000 lamports per signature** (~0.000005 SOL). Paid regardless of
compute used; **50% burned, 50% to the validator**. It is not a competitive lever
— everyone pays it.

### Priority fee
The competitive lever for ordinary blockspace. You set a **price per CU** in
**micro-lamports** via a ComputeBudget **`SetComputeUnitPrice`** instruction, and
you pay:

```
priority fee = compute_unit_price (micro-lamports/CU) × compute_units_used
```

Higher priority fees win contested blockspace because the **PrioGraphScheduler
orders by priority fee** (`banking-stage-and-sealevel.md`). The fees actually
paid feed the validator's `PrioritizationFeeCache`, which is what
`getRecentPrioritizationFees` reports — the data the tip oracle samples
(`../../run/references/tip-pricing.md`).

Copilot sets **both** budget instructions on every bundle transaction, in order:
limit first, price second, then the payload, then the tip last
(`../../run/references/bundle-construction.md`).

---

## Local fee markets

Priority fees are **local, not global**. Congestion is per-account (really, per
write-lock): the price to touch one hot account (a popular pool, a hot mint) can
spike enormously while the rest of the chain stays cheap. There is no single
network-wide "gas price." This is *why* the oracle samples the live
`getRecentPrioritizationFees` distribution rather than assuming a constant — the
right number depends on what you're touching and when.

It also connects back to contention: a high local fee on a hot account is the
market's signal that many writers want that same write-lock (the very thing that
serializes them in Sealevel). Paying the local priority fee is how you buy
ordering *among* those competitors.

---

## Priority fee vs. Jito tip — two different auctions

The distinction trips people up constantly, so be precise:

| | **Priority fee** | **Jito tip** |
|---|---|---|
| What it is | CU price set via ComputeBudget | a SOL transfer to a Jito tip account |
| Who decides | the leader's PrioGraphScheduler | Jito's off-chain Block Engine auction |
| What it wins | ordering within the normal fee market | inclusion of your *bundle* (`jito-bundles.md`) |
| Copilot's use | set from the live prio-fee median (good citizenship) | the dominant lever, priced from the landed-tip floor |

For a Jito bundle, the **tip is what lands you**; the priority fee is secondary.
Copilot still sets a sensible CU price from the prio-fee median, but it does not
rely on it to win the bundle auction. So when a user asks "did we pay enough?",
the answer is almost always about the **tip relative to the landed-tip floor**,
not the priority fee (`../../diagnose/references/tip-strategy.md`).

---

## Rent (brief, for completeness)

Accounts must hold a minimum lamport balance to be **rent-exempt**; an operation
that would drop an account below that minimum fails (`InsufficientFundsForRent`).
This rarely touches Copilot's tip-only bundles (which create no accounts), but
it's a real on-chain error to recognize when explaining a user's own payload
failing — and, unlike expiry or fee issues, it's a *construction/funding* problem
no retry or tip fixes.

---

## Operator takeaways

- Set CU limits from **simulation + buffer**, not the 200k default, to avoid both
  overpaying and failing.
- The number that determines whether a **bundle** lands is the **tip vs. the
  landed-tip floor**, not the priority fee.
- A `compute_exceeded` failure is an upstream fix (raise the limit or reduce the
  work), never a tip change.
- "Block full" = the 48M-CU cap; "this account is expensive right now" = a hot
  local fee market on that specific write-lock.