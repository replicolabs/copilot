# QUIC and stake-weighted QoS

How transactions actually get *in* — the transport and the admission-control
system that decides whose packets survive under load. This is the mechanism
behind two of the silent failure modes in `transaction-pipeline.md` (QoS drops
and connection throttling), and it explains why "who is sending" matters as much
as "what you send".

---

## Why QUIC (not TCP, not raw UDP)

The Fetch stage ingests transactions over **QUIC** (Quick UDP Internet
Connections), a transport originally from Google that combines the connection
and encryption handshakes and multiplexes many independent streams over one
connection.

**The problem with TCP — head-of-line (HOL) blocking.** Picture one connection
carrying transactions from 100 clients as a single ordered stream. If one packet
is lost, TCP stalls *all* downstream processing until that packet is
retransmitted. A single lost packet from one spammer can grind the whole
ingestion pipeline to a halt.

**QUIC's fix — multiplexing.** Each client (and each transaction) travels in its
own independent **stream** inside the shared connection. A lost packet blocks
only its own stream; streams from high-stake validators and critical apps keep
flowing. The Fetch stage receives clean, complete, reassembled streams rather
than a flood of unordered raw packets it would have to reassemble itself.

**Why not raw UDP?** Raw UDP would deliver individual unordered packets, forcing
Fetch to spend time and memory figuring out which packets belong to which
transaction while new packets flood in. QUIC pushes all stream reassembly below
the application, so Fetch reads a complete serialized transaction (or batch) per
stream, wraps it in a `Packet`, and pushes it to the SigVerify queue.

---

## mTLS and identity

QUIC's built-in mutual TLS means a client must complete a handshake **before**
sending any transaction data. That handshake proves the client's identity (its
node public key). Crucially, the TPU server then **looks up that public key's
stake** before accepting traffic — identity is bound to economic weight at the
transport layer, not bolted on later.

**0-RTT.** For known clients, QUIC allows sending transaction data in the very
first packet of a *resumed* connection (Zero Round-Trip), skipping the handshake
entirely. This is a meaningful latency win for senders maintaining persistent
connections (as a TPU client does).

---

## Stake-weighted Quality of Service (SWQoS)

Under load the validator cannot serve everyone, so admission is rationed by
**stake**:

- **Streams per connection scale with stake.** A client that has staked more SOL
  may open more concurrent streams (up to its allowance), buying more ingestion
  bandwidth. The network regulates connection speed via stake-weighted
  packets-per-second.
- **Bandwidth is proportional to stake.** A validator holding 1% of network
  stake is guaranteed roughly 1% of a server's connection capacity. Identity →
  stake → guaranteed share.
- **Connection caps:** roughly **2,000** concurrent connections for staked
  nodes versus **500** for unstaked nodes.
- **Throttling = dropped streams.** When a client exceeds its rate, the server
  drops its QUIC streams; that *is* the throttling signal. There's no polite
  "slow down" — packets simply stop being accepted.

When the network is busy, this gives economically-invested participants priority
and sheds load from no-/low-stake senders first. From a plain
client's perspective, that shedding is indistinguishable from any other drop
(`transaction-pipeline.md`, section 4).

---

## The three ports

The TPU separates traffic by arrival port so consensus traffic is never starved
by user traffic:

| Port | Carries | Why separate |
|---|---|---|
| `tpu` | normal user transactions | the bulk of contended traffic |
| `tpu_vote` | validator consensus votes | votes must never be blocked by user load — the chain's liveness depends on them (`commitment-and-finalization.md`) |
| `tpu_forwards` | transactions forwarded from the previous leader | lets an overloaded leader hand work to the next one |

This port-level split is mirrored deeper in the pipeline by physically separate
vote and non-vote processing lanes (`banking-stage-and-sealevel.md`).

---

## What this means operationally for Copilot

- Copilot is typically **not** a high-stake sender, so under heavy congestion its
  raw-TPU packets are exactly the ones SWQoS sheds first. This is a structural
  reason to submit through **Jito** (`jito-bundles.md`): the Block Engine path
  competes via a tip auction rather than relying on the sender's own stake to win
  QoS, and lands the bundle through a Jito-connected leader.
- When a submission silently never lands during a congestion spike, **QoS
  shedding is a real candidate** alongside expiry and outbidding — and it leaves
  no on-chain trace. The `watch` feed's congestion read is the cheapest way to
  see whether you're submitting into exactly the conditions where shedding is
  most aggressive (`../../watch/references/reading-the-feed.md`).
- The fix for QoS pressure is not "tip more" in the priority-fee sense; it's
  winning the Jito auction (tip) and/or waiting for the congestion to ease. Match
  the response to the cause (`../../diagnose/references/tip-strategy.md`).