---
name: run
description: Submit tip-only Jito bundles with Copilot and track each transaction's lifecycle from submitted through processed, confirmed, and finalized, writing a committed log per run. Use when the user wants to land bundles, "run copilot", send a batch of submissions, produce lifecycle evidence, or land transactions on Solana through Jito with data-driven tips.
---

# run

Land bundles and follow each one to finality, writing `logs/lifecycle-run-NN.json`
per submission. This is Copilot's main loop: price a tip from live data, build a
bundle, submit it through Jito, and confirm landing over the stream.

> **Routing:** if this isn't the right skill for the request, consult `../SKILL_ROUTER.md` and switch.

## Prerequisites

- `.env` configured and `copilot watch` shows live, advancing slots (see `setup`).
- A funded payer keypair. **Mainnet** if landings are expected — Jito only lands
  on Jito-leader slots, which don't exist on devnet.

## Workflow

1. **Confirm intent.** Ask how many submissions, and whether to let the oracle
   price the tip (default, recommended) or pin a fixed tip. If they expect
   landings, confirm they're not on devnet.
2. **Run it:** `copilot run --count <N>` (add `--tip <lamports>` only to override
   the oracle). For each iteration Copilot:
   - logs the current leader and the next Jito-leader window (observability),
   - prices the tip from the live Jito landed-tip floor + congestion
     (`references/tip-pricing.md`),
   - builds a **tip-only** bundle — `[set_cu_limit, set_cu_price, tip_transfer]`,
     a valid minimal bundle whose only effect is paying the tip
     (`references/bundle-construction.md`),
   - submits it to the Block Engine, and
   - tracks the signature over the Yellowstone stream to finality, writing
     `logs/lifecycle-run-NN.json` (`references/lifecycle-tracking.md`).
3. **Read each result back.** Report the stage reached, the landed slot, and the
   latency deltas. A clean run reaches finalized with a tight processed→confirmed
   delta. For a full pass over the files, hand off to the `logs` skill.
4. **If something doesn't land,** don't hand-wave a cause — switch to the
   `diagnose` skill and reason from the signals.

## Why tip-only bundles

The default bundle carries no user payload — it exists to exercise and prove the
*infrastructure* (submission, landing, the full commitment timeline) at minimal
cost. A real integration passes its own instructions; the pipeline is identical.
Keeping the demo bundles tip-only is what makes a 10+ submission run cost a
fraction of a SOL.

## Non-negotiables

- **Tips come from the oracle or the user — never invented.** Don't suggest a
  number yourself; point at what the landed floor says (`references/tip-pricing.md`).
- **Landing is confirmed over the stream.** Never substitute `getSignatureStatuses`
  polling; it's slower and lossier exactly when congestion makes it matter.
- Keep bundles small and cheap. This is infrastructure proving, not moving funds.
- Report only what the log shows — don't claim a finalized landing from a
  confirmed-only entry.

## References

- `references/bundle-construction.md` — the instruction layout and the tip-last rule.
- `references/tip-pricing.md` — how the tip is priced from live data.
- `references/lifecycle-tracking.md` — how landing and commitment are tracked.