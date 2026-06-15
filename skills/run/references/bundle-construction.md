# Bundle construction

How Copilot assembles the transaction it submits, the rules that make it land,
and what is fixed vs. supplied. Grounded in the transaction model and the Jito
tip rules (`../../solana-internals/references/jito-bundles.md`, `../../solana-internals/references/fees-and-compute.md`).

---

## The transaction model (what we're building)

A Solana transaction is:

```
Transaction
├── Signatures[]            Ed25519 signatures from the required signers
└── Message
    ├── Header              # of required signers, # of read-only accounts
    ├── Account Keys[]      every account the transaction references
    ├── Recent Blockhash    expiry + replay protection (~150 slots)
    └── Instructions[]
        ├── Program ID      which program to invoke
        ├── Account Indexes indexes into Account Keys
        └── Data            serialized instruction args
```

Hard limits to respect:

| Constraint | Limit |
|---|---|
| Max serialized size | **1,232 bytes** |
| Max accounts (legacy) | 32 |
| Max accounts (v0 + Address Lookup Table) | 256 |
| Blockhash validity | ~150 slots (~60s) |
| Max CU per transaction | 1,400,000 |

Copilot builds **v0 (versioned)** transactions. v0 supports **Address Lookup
Tables (ALTs)**, which let a transaction reference up to 256 accounts by pointer
into an on-chain table instead of inlining each 32-byte key — essential when a
real payload touches many accounts and would otherwise blow the 1,232-byte cap.
Copilot's default tip-only bundle touches very few accounts, so it stays far
under the limit; ALTs matter when a caller supplies a fat payload.

---

## The instruction layout

A Copilot bundle is a single v0 transaction laid out in a fixed order:

```
[ SetComputeUnitLimit ]    ComputeBudget — the CU ceiling
[ SetComputeUnitPrice ]    ComputeBudget — priority fee, micro-lamports/CU
[ ...caller payload... ]   the actual work (empty for tip-only bundles)
[ tip transfer -> Jito ]   System transfer to a Jito tip account — MUST be last
```

- The two **ComputeBudget** instructions come **first** so the runtime knows the
  CU ceiling and the priority-fee rate before executing anything
  (`../../solana-internals/references/fees-and-compute.md`).
- The **payload** is whatever the caller wants done. Copilot's default `run` and
  `inject` flows use an **empty payload** — a tip-only bundle that is a valid,
  landable transaction whose only effect is paying the tip. A real integration
  drops its instructions here; nothing else about construction changes.
- The **tip transfer is always last** (next section).

---

## The tip-last rule (and why)

Jito's requirement: the tip transfer must be the **last instruction of the last
transaction** in the bundle. Bundles execute all-or-nothing, but a fork can still
drop a bundle after the fact; tip-last ensures you never pay the tip for work
that didn't land. It ties the tip to the bundle's success. Copilot's builder
appends the tip after the payload every time — this is not configurable, because
getting it wrong means paying for drops.

---

## What's supplied vs. fixed (the deliberately-dumb builder)

The builder chooses **none** of the values that matter; everything that varies is
an input. This separation is what keeps tip selection a real, data-driven
decision made by the oracle/agent rather than a constant baked into construction.

| Input | Source | Notes |
|---|---|---|
| **tip account** | random pick from live `getTipAccounts` set | spreads write-lock load across Jito's tip accounts (avoids a self-inflicted hot account — `../../solana-internals/references/banking-stage-and-sealevel.md`) |
| **tip lamports** | the oracle (or `--tip`) | validated ≥ 1,000-lamport protocol minimum; the build fails fast below that rather than eating a guaranteed rejection (`tip-pricing.md`) |
| **CU limit** | small constant for tip-only; simulation-derived for real payloads | a transfer + 2 budget ix is only a few hundred CU; over-requesting wastes priority fee |
| **CU price** | live priority-fee median | good citizenship in the normal fee market; secondary — the **tip** wins the bundle auction |
| **blockhash** | latest from the live geyser feed (processed) | freshest possible → max validity window; **never** finalized (`../../solana-internals/references/blockhash-lifetime.md`) |

---

## Signing and encoding

- The v0 message is built, then signed by the payer. Copilot loads the payer from
  `COPILOT_KEYPAIR` (a 64-byte keypair file or inline base58 — see
  `../../setup/references/environment.md`).
- The signed transaction is **bincode-serialized** and **base64-encoded** into the
  wire form `sendBundle` expects.
- Copilot enforces the **1–5 transaction** bundle-size rule locally before the
  network round-trip, so an oversized bundle fails with a clear local error
  instead of a remote one.

---

## Common construction failures (recognize these)

These are *construction/funding* problems — no tip or retry fixes them; the fix
is in how the transaction was built:

- **`TransactionTooLarge`** — exceeded 1,232 bytes. Use ALTs or split the work.
- **`BlockhashNotFound`** — the blockhash was already stale at submit (built on a
  too-old or finalized blockhash). Build on a fresh processed blockhash.
- **`InsufficientFundsForRent`** — an account would drop below rent-exempt. Fund
  it (`../../solana-internals/references/fees-and-compute.md`).
- **`InstructionError`** — a program returned an error; decode the code from the
  program's IDL. (For tip-only bundles this shouldn't occur — it's a payload
  issue.)
- **compute exceeded** — the CU limit was too low for the work; raise it from
  simulation, don't retry as-is.

Distinguish these sharply from **landing** failures (expiry, fee-too-low,
leader-skip, contention), which are about getting *included* rather than being
*well-formed* (`../../diagnose/references/failure-taxonomy.md`).