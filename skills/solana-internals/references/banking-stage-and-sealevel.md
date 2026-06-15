# The Banking Stage and Sealevel

How a leader actually *executes* transactions and turns them into a block. This
is the deepest layer most operators never see, but it explains two things that
matter directly to Copilot: why **hot-account contention** silently sinks
transactions, and what "the block filled up" really means for the 48M-CU cap.

---

## The big idea: pre-declared accounts → parallelism

Sealevel is Solana's parallel execution model. Its "secret sauce" is that **every
transaction must declare, up front, every account it will read or write**, and
whether each is read-only or writable. That declaration is a reservation system:
the runtime can look at a set of pending transactions and immediately tell which
can run together.

- Transaction A writes account X; transaction B reads account Y → **non-
  overlapping → run in parallel** on different threads.
- Transactions C and D both write account Z → **conflicting → must run
  sequentially** to avoid corrupting state.

This is the opposite of a black-box model where every transaction is processed
one-by-one for safety. Non-conflicting transactions ideally schedule in O(1)
relative to each other. The cost of that speed is the constraint that creates a
key failure mode: when many transactions want to **write-lock the same hot
account** (a popular AMM pool, a hot mint, a trending token's state), they
*cannot* be parallelized and must queue behind each other — so most are deferred
or dropped within a single slot, regardless of how well-formed they are
(`transaction-pipeline.md`, section 4).

---

## The Banking Stage pipeline

The Banking Stage is "a pipelined process inside a pipelined process." Its
journey:

### Ingress — three receiver channels
Raw packets arrive on `crossbeam_channel` receivers:
- `non_vote_receiver` — regular transactions,
- `tpu_vote_receiver` — votes received directly via the TPU,
- `gossip_vote_receiver` — votes received via gossip (older/forwarded).

### Deserialize + sanitize
A `PacketDeserializer` parses raw byte buffers, runs structural and signature
checks, and produces `SanitizedTransaction` objects. These land in a **buffer**
that decouples network ingress rate from execution rate — smoothing bursts so a
sudden flood doesn't directly stall execution.

### Schedule
A `SchedulerController` loop continuously pulls from the buffer and assigns
non-conflicting batches to workers under account-lock constraints. Two schedulers
exist, chosen by the validator's block-production config:
- **PrioGraphScheduler** — the sophisticated default. It builds a **dependency
  graph** of transactions keyed on the accounts they lock, finds sets that can run
  in parallel, and **prioritizes by priority fee** so high-fee transactions are
  scheduled first. This is the mechanism by which your **priority fee** actually
  buys you ordering and inclusion under contention (`fees-and-compute.md`).
- **GreedyScheduler** — a simpler, lower-overhead scheduler that favors speed of
  scheduling over optimal parallelism.

The scheduler's output is **batches of non-conflicting transactions**, sent to
workers over channels.

### Execute (worker threads)
The Banking Stage spawns a pool of workers — roughly **one per CPU core, minus
threads reserved for voting**. Each `ConsumeWorker` runs in its own thread and
holds a `Consumer` that calls the Bank's
`load_execute_and_commit_transactions(...)` for its batch: load the accounts from
AccountsDB, run the SVM, commit results. Workers keep small local buffers/priority
queues (e.g. up to ~64 transactions).

### Commit side effects
A `Committer` handles the aftermath of a committed transaction:
- sends final status (success/error) to the `TransactionStatusSender`, which is
  what RPC uses to answer `getSignatureStatuses` queries,
- updates the `PrioritizationFeeCache` with the fees actually paid — **this cache
  is the basis for `getRecentPrioritizationFees`**, which Copilot's tip oracle
  samples (`../../run/references/tip-pricing.md`),
- for votes, forwards vote info to the `ReplayVoteSender` so fork weights update.

### Record (PoH) and broadcast
Executed transactions are grouped into **Entries**, timestamped into the
Proof-of-History stream (a verifiable clock that orders events before consensus),
then handed to Broadcast for shredding and Turbine propagation
(`transaction-pipeline.md`).

---

## Votes get their own lane

Vote transactions do **not** go through the same rigorous scheduling as user
transactions. Votes are time-critical, generally don't conflict with each other,
and are how validators reach consensus — if votes are delayed or dropped, blocks
can't confirm and the chain slows or stalls. So votes get **dedicated,
streamlined pipelines** (separate ports, separate channels, separate processing)
guaranteeing them an unblocked path even when user load is extreme. This is why,
under congestion, *consensus keeps moving* even as user transactions are being
shed — and why finalization timing stays roughly stable while landing gets hard.

---

## The SVM, the Bank, and the InvokeContext (one level down)

It helps to keep three roles distinct:

- **The SVM** is the execution unit — a "dumb but powerful CPU core." It takes one
  compiled program plus the specific accounts it's authorized to touch, runs the
  bytecode, and reports state changes and compute consumed. It knows nothing of
  other transactions, slots, or consensus.
- **The Bank** is the state — a snapshot of the ledger for one slot, with methods
  to mutate accounts and commit transactions. Banks are chained (each has a
  parent), forming **Bankforks**, a tree of possible ledger states.
- **The Banking Stage** is the engine that drives inputs (packets) against the
  current Bank to produce the next state.

Execution of a transaction's instructions (including cross-program invocations)
happens inside a single **InvokeContext**, configured with: the
`TransactionContext` (the loaded, authorized mutable accounts), a per-batch
program cache, a read-only `EnvironmentConfig` (feature set, sysvars, callbacks)
that keeps execution deterministic, an optional `LogCollector`, an execution
**budget** (max compute units, CPI stack depth), and a **cost model** (lamports
per signature, write/data-size fees). The whole CPI chain shares one
InvokeContext, so **if any program in the chain fails, all state changes roll
back** — the atomicity guarantee, enforced at the runtime level. CPI also enforces
**privilege-escalation prevention**: a caller can't hand a callee more access than
it holds (e.g. passing a read-only account as writable).

---

## The Bank lifecycle: Open → Frozen → Rooted

This maps almost one-to-one onto the commitment levels Copilot tracks
(`commitment-and-finalization.md`):

| Bank state | What happens | Commitment analogue |
|---|---|---|
| **Open** | new Bank for the slot; mutable; validators add and execute transactions | a transaction landing here is **processed** |
| **Frozen** | slot complete (time up or all txs processed); no more state changes; the Bank computes its immutable **bank hash**, which other validators vote on; sysvars updated, rent collected | the block others vote toward → **confirmed** once a supermajority votes |
| **Rooted** | enough votes received that this Bank is a permanent part of the ledger; cannot be rolled back | **finalized** |

So "processed → confirmed → finalized" is the same story told from the consensus
side as Open → Frozen → Rooted.

---

## What an operator should take from this

- **Hot-account contention** is a first-class, invisible failure cause. If a
  submission targets a contended account during a busy slot, it can fail to land
  with no error — and a bigger Jito tip doesn't relieve write-lock contention.
  (Copilot's default bundles are tip-only and touch only the system program + a
  Jito tip account, so they avoid this — but it matters when explaining a user's
  *own* payload not landing.)
- Your **priority fee is the lever the PrioGraphScheduler reads** to order you
  ahead of competitors for the *same* contended accounts; it's distinct from the
  Jito tip that wins the bundle auction (`fees-and-compute.md`, `jito-bundles.md`).
- "The block is full" is the **48M-CU** ceiling being hit, not a byte limit; the
  scheduler is choosing the highest-fee non-conflicting set that fits.
- Consensus (votes) is insulated from user-load shedding, which is why
  finalization timing is stable even when inclusion is hard — useful when reading
  a run where landings struggled but the commitment deltas still looked normal
  (`../../logs/references/interpreting-logs.md`).