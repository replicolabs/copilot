# The transaction pipeline (TPU)

How a transaction physically travels from a client to inclusion in a block, and
the many points at which it can be dropped without explanation. This is the
foundation for reading failures (`diagnose`) and for understanding why landing
is never guaranteed — it underlies almost everything else Copilot does.

---

## 1. There is no mempool

On Bitcoin/Ethereum, a submitted transaction waits in a global **mempool** — an
unordered pool of pending transactions (20k–100k+ under load) that block
producers pick from. Solana has **no global mempool**. Instead, transactions are
forwarded directly toward the validators scheduled to produce upcoming blocks
(Gulf Stream, below). The consequences run through everything:

- There is no public "pending" state to inspect. A transaction is either being
  raced to a leader or it isn't.
- There is no mempool-level fee auction visible to everyone; prioritization is
  decided per-leader by the scheduler (`banking-stage-and-sealevel.md`).
- A transaction that isn't included by the current leader isn't "waiting" — it
  must have been forwarded to the next leader, or it's gone.

The pipeline still uses bounded in-memory queues (MPMC channels) with FIFO
ordering *between stages inside a leader*; that's different from a network-wide
mempool.

---

## 2. Two paths to the leader

### Slow path — the RPC client

`sendTransaction` over JSON-RPC goes to a **single RPC node**, which relays the
transaction to the current leader on the client's behalf.

- **Upside:** feedback. The RPC path integrates with commitment levels, so the
  client can learn when the transaction is processed/confirmed/finalized, and
  preflight simulation can catch errors before sending.
- **Downsides:** an extra network hop and a single point of contention. During
  congestion, the same few RPC nodes are hit by thousands of clients and begin
  rate-limiting or dropping. The RPC node is a middleman whose health you don't
  control.

This is what `solana-cli`, Anchor tests, and web3.js use by default.

### Fast path — the TPU client (and Gulf Stream)

Block producers and latency-sensitive senders push transactions **straight into
the data plane**, bypassing the RPC middleman:

1. The client still needs an RPC connection, but only to call
   `getLeaderSchedule` / `getEpochInfo` and learn which validators lead the
   current slot and the next several slots.
2. It serializes the transaction into its raw binary "wire transaction"
   (`Vec<u8>`).
3. It **fanouts** — sends that wire transaction in parallel to the TPU ports of
   the current leader *and the next N leaders* (`fanout_slots`), over a pool of
   persistent QUIC connections.

This client-side fanout is effectively Gulf Stream from the sender's side: it no
longer matters if the current leader is busy, offline, or drops the packet,
because the next leader (and the one after) already holds the transaction and can
include it the moment its slot begins. Jito's submission path is a specialized,
auction-gated version of this fast path (`jito-bundles.md`).

The TPU client is built as an asynchronous agent: persistent background tokio
tasks coordinated with `Arc<AtomicBool>`, `Arc<RwLock<LeaderCache>>`, and
`JoinHandle`s, holding QUIC connections open for the client's lifetime.

**Where Copilot sits:** Copilot submits through Jito's Block Engine (an auctioned
fast path) and confirms landing over a Yellowstone gRPC stream rather than RPC
polling. It uses RPC only for the supporting reads (leader schedule, recent
prioritization fees). See `../../run/references/lifecycle-tracking.md`.

---

## 3. The four pipeline stages

A leader's TPU is pipelined so stages run in parallel on different hardware —
while Banking executes batch N, SigVerify is verifying N+1, Fetch is reading N+2,
and Broadcast is shipping N-1. Throughput is bounded by the slowest stage, not
the sum.

### Fetch
The entry point. Reads transaction packets off **QUIC** streams (typically one
stream per transaction packet), coalesces packets that arrive close in time into
batches (e.g. 128), and forwards them to SigVerify. **Stake-weighted QoS** is
enforced here (`quic-and-swqos.md`). Transactions arrive on one of three ports,
which the validator distinguishes:
- `tpu` — normal user transactions (transfers, swaps, mints, program calls).
- `tpu_vote` — validator consensus votes.
- `tpu_forwards` — transactions handed on from the previous leader that it
  couldn't fit.

### SigVerify
Batch-verifies Ed25519 signatures, **GPU-accelerated** because signature
verification is massively parallel and would otherwise bottleneck the CPU-bound
Banking stage. Also dedupes packets and **load-sheds** (discards excess packets)
when the system is overloaded. Packets with invalid signatures are dropped here.
Vote and non-vote packets run on two distinct pipelines.

### Banking
The heart — executes transactions via the SVM. A scheduler groups
**non-conflicting** transactions (by their pre-declared account locks) and feeds
them to worker threads for parallel execution; results update account state and
are timestamped into the Proof-of-History stream as Entries. Atomicity is
absolute: **if any single instruction in a transaction fails, the whole
transaction fails** and its state changes roll back. Full detail in
`banking-stage-and-sealevel.md`.

### Broadcast
Takes ordered, validated Entries, **shreds** them (splits into ~1280-byte
packets), applies Reed-Solomon **erasure coding** (so a validator needs only
~67% of shreds to reconstruct the block), signs them, and propagates via the
**Turbine** fanout tree. Stake also weights Turbine propagation, so
higher-staked nodes receive shreds earlier.

(The TPU runs when a node is the **leader**, producing blocks. Its mirror, the
**TVU**, runs when the node is validating another leader's block.)

---

## 4. Where transactions die — the multi-stage filter

This section is the operational core. From the sender's side, every one of these
looks identical — "it didn't land" — and **most produce no feedback at all**.
That silence is precisely why Copilot reasons from observable signals instead of
trusting a single error string, and why an AI agent earns its place here.

1. **Hardware / OS.** The NIC and the OS UDP buffers have finite capacity;
   packets are dropped before the application layer is ever aware of them.
2. **QoS.** The validator deliberately drops packets from low-/no-stake
   connections to protect capacity for high-stake vote and state traffic
   (`quic-and-swqos.md`). A 1%-stake sender gets ~1% of connection capacity.
3. **Queue backpressure.** Inter-stage queues are bounded; if any stage
   bottlenecks it stops pulling from its input queue. The transaction may be
   accepted at ingress yet never become visible on-chain.
4. **Hot-account contention.** The scheduler may hold thousands of valid
   transactions all trying to write-lock the *same* account in the same slot;
   most cannot proceed and are deferred or dropped (`banking-stage-and-sealevel.md`).
5. **Outbid / expired.** The transaction is seen but passed over because someone
   paid a higher priority fee for the same contested resource, and then its
   blockhash expires before another leader takes it (`blockhash-lifetime.md`).

The honest takeaways for an agent:
- A non-landing transaction usually cannot tell you *which* of these happened.
- The right response depends entirely on which one it was, and the only way to
  narrow it is to combine the signals Copilot *can* observe — blockhash age,
  whether a Jito leader produced, the Block Engine verdict, the tip versus the
  landed market, any on-chain error. See `../../diagnose/references/failure-taxonomy.md`.
- "Just retry harder / tip more" is wrong for most of these causes. Matching the
  fix to the cause is the entire discipline.

---

## 5. How this maps to Copilot

- `geyser` keeps a live view of the chain (slots, leader, blockhash) over
  Yellowstone gRPC — the same stream the lifecycle tracker confirms landings on.
- `leader` tracks the schedule so Copilot knows the upcoming leader windows
  (`../../watch/references/leader-scheduling.md`).
- `bundle` builds and submits through Jito's auctioned fast path
  (`jito-bundles.md`, `../../run/references/bundle-construction.md`).
- `lifecycle` follows each submission across commitment levels over the stream,
  never by RPC polling (`../../run/references/lifecycle-tracking.md`).
- `fault` + `agent` exist *because* of section 4: when a transaction dies in the
  filter with no clear reason, classification surfaces the ambiguity and the
  model decides the response.