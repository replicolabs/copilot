---
name: solana-internals
description: Deep reference on how Solana actually moves a transaction from submission to finality — the TPU pipeline, QUIC and stake-weighted QoS, Jito bundles and the block-engine auction, commitment levels and Tower BFT finalization, blockhash lifetime, and the compute/fee market. Use whenever you need to explain or reason about why a Copilot submission behaved the way it did, why a transaction failed or didn't land, how tips and priority fees work, or any "how does Solana do X" question that comes up while operating Copilot.
---

# Solana internals

This is Copilot's knowledge base for the chain it operates on. It is mechanics,
not commands — the other Copilot skills (`run`, `inject`, `logs`, `diagnose`)
point here when an explanation needs to be grounded in how Solana really works.

When a user asks *why* something happened — why a bundle didn't land, why an
expired blockhash isn't a fee problem, why finalized lags so far behind, why a
tip needs to clear a certain bar — read the relevant reference below and answer
from it. Cite the mechanism, don't hand-wave.

> **Routing:** if this isn't the right skill for the request, consult `../SKILL_ROUTER.md` and switch.

## What's here

| Topic | Read |
|---|---|
| How a transaction travels from client to block: no mempool, the slow (RPC) and fast (TPU-client/Gulf Stream) paths, the four TPU stages, and the multi-stage filter where transactions die silently | `references/transaction-pipeline.md` |
| The transport and admission control: QUIC multiplexing, mTLS, 0-RTT, the three TPU ports, and stake-weighted QoS (who gets dropped under load) | `references/quic-and-swqos.md` |
| How a leader executes a block: Sealevel's pre-declared-accounts model, hot-account contention, the PrioGraph/Greedy scheduler, worker threads, the SVM/Bank/InvokeContext split, and the Open→Frozen→Rooted Bank lifecycle | `references/banking-stage-and-sealevel.md` |
| Commitment levels (processed / confirmed / finalized), Tower BFT vote lockouts, optimistic confirmation and rollback, and why finalization lags the tip by ~31 slots | `references/commitment-and-finalization.md` |
| Blockhash lifetime: the ~150-slot validity window, why `BlockhashNotFound` is not a fee problem, why a finalized blockhash is the wrong choice, blockhash age as a signal, and durable nonces | `references/blockhash-lifetime.md` |
| The compute and fee market: compute units and the 48M-CU block cap, base vs priority fees, local fee markets, simulation-based CU sizing, and priority fee vs Jito tip | `references/fees-and-compute.md` |
| Jito bundles: atomic all-or-nothing execution, relayer/Block Engine/ShredStream, the off-chain auction, tip accounts and the tip-last rule, bundle statuses, and why a bundle only lands on a Jito-leader slot | `references/jito-bundles.md` |

## The one-paragraph model

A transaction is submitted, races through a leader's pipeline (or is forwarded to
the next leader via Gulf Stream), and — if it survives signature verification,
QoS, queue backpressure, and account contention — is executed and recorded in a
block. It is then **processed**, later **confirmed** when a supermajority votes,
and finally **finalized** when the block is rooted ~31 slots later. Along the way
the transaction is only valid while its blockhash is recent (~150 slots / ~60s),
it competes for limited block compute (~48M CU) via priority fees, and — if
submitted as a Jito **bundle** — it competes in an off-chain auction and lands
atomically only when a Jito-connected leader produces the slot. Most of Copilot
exists because each of those stages is a place where a transaction can fail for
reasons that aren't obvious from the outside.

## Non-negotiables

- Explain from these references, not from guesses. The numbers (150 slots, ~31
  slots, 48M CU, the QoS limits) are specific and load-bearing — get them right.
- When a cause is genuinely ambiguous (a transaction that simply never appears),
  say so. The pipeline drops transactions at several layers with no feedback;
  pretending otherwise is the mistake Copilot's agent exists to avoid.