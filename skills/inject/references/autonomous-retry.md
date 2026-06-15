# The autonomous-retry loop

The full sequence `copilot inject` runs, step by step. This is the project's
flagship: a closed loop of **live chain data → model reasoning → executed
decision**, with a real (manufactured) failure, a real model call, and a real
recovery — each stage observable and logged.

---

## Why an expired blockhash is the fault we inject

Of all the failure causes (`../../diagnose/references/failure-taxonomy.md`), an
expired blockhash is the cleanest teaching case:

- It is **high-confidence and measurable** — blockhash age vs. the ~150-slot
  window is a hard number, not a guess (`../../solana-internals/references/blockhash-lifetime.md`).
- Its **correct fix is the opposite of the naive instinct.** A reflex rule says
  "failed? tip more." The right move for expiry is a **fresh blockhash, same
  tip.** That gap is exactly what a *reasoning* agent navigates and a hardcoded
  rule gets wrong — so it's the ideal way to show the decision is real.

---

## Step 0 — snapshot the world

Before injecting anything, Copilot captures a live snapshot: the current
blockhash and slot from the geyser feed, the Jito landed-tip floor percentiles,
the priority-fee distribution, and the derived congestion read
(`../../run/references/tip-pricing.md`). This is the ground truth the agent will
reason over, captured *before* the failure so it reflects real conditions at the
moment.

## Step 1 — inject the fault

Copilot takes a real, recent blockhash and **deliberately ages it past the
~150-slot window**: it rotates the hash bytes and applies a fixed transform
(XOR), producing a well-formed but stale-looking blockhash with an apparent age
beyond `MAX_PROCESSING_AGE`. A bundle built on this blockhash **cannot land** —
the cluster rejects it as effectively expired. The failure is manufactured, but
the cause is genuine: this is exactly what an expired blockhash does in
production.

## Step 2 — submit the doomed attempt

The bundle is built and submitted normally, but with a **short landing deadline**
(failure is expected, so there's no point waiting a full blockhash lifetime). When
the deadline passes with no landing, the attempt is recorded as a **failed**
`logs/lifecycle-run-NN.json` — a real failure log, not a simulation
(`../../run/references/lifecycle-tracking.md`).

## Step 3 — classify

The `fault` crate reads the signals — did it land? blockhash age vs. the window?
did a Jito leader produce? Block Engine verdict? tip vs. landed median? any
on-chain error? — and produces a `FailureEvent`: a **kind**, a **confidence**, a
**rationale**, and the **alternatives** it couldn't rule out. For the injected
fault this lands on `expired_blockhash` at high confidence, because the measured
blockhash age is decisively past the window
(`../../diagnose/references/failure-taxonomy.md`).

## Step 4 — decide (the model)

Copilot assembles the `FailureEvent` **plus** the Step-0 chain snapshot into a
single JSON context and sends it to Claude with a system prompt that grounds the
Solana mechanics. Claude returns a **structured JSON decision**: an `action`
(`retry` / `abort` / `wait`), an optional `new_tip_lamports`, a `reasoning`
string, and a `confidence`. **There is no local decision logic** — Copilot parses
and obeys the model's output. The context/decision schemas, the system prompt, the
client mechanics, and worked examples are all in `agent-reasoning.md`.

## Step 5 — execute

If the decision is `retry`, Copilot rebuilds the bundle with a **fresh blockhash**
from the live feed and the agent's chosen tip, resubmits with a **normal** landing
deadline, and tracks it to finality — writing a second `lifecycle-run-NN.json`.
For an expired-blockhash failure this retry should **land**, because the only
thing wrong was the stale blockhash. `abort` (stop) and `wait` (pause, then
reconsider) are honored too.

---

## What the loop proves

Each stage is observable and committed:

- **the failure is real** — a committed failed log with `landed_slot: null`,
- **the decision is real** — a logged model call over live data
  (`logs/agent-reasoning.jsonl`),
- **the recovery is real** — a committed landed log with an explorer-checkable
  slot.

Nothing in the loop is a hardcoded `if expired { refresh }`. That branch lives in
the **model's reasoning**, which is the entire point: live chain data in,
structured decision out, executed against the chain — the bar for "the agent
makes a real decision."

---

## Narrating it for a user

Set expectations first so the AI step is unmistakable, then narrate the three
printed lines as they appear:

1. the injection (an expired blockhash is being introduced on purpose),
2. `classified <kind> (confidence) — <rationale>`,
3. `agent decision: <action> (confidence) — <reasoning>`,

then open **both** artifacts: the failed + landed lifecycle logs (point at the
retry's real `landed_slot`) and the `agent-reasoning.jsonl` line (point at the
live context the model saw and what it returned). Finally tie it to mechanics: a
good decision keeps the tip near the prior value because the fault was expiry, not
under-pricing — and if it raised the tip, check whether congestion justified it
(`agent-reasoning.md`, `../../diagnose/references/tip-strategy.md`).

**Honesty rules:** the decision is the model's — never call it rule-based. If the
agent can't be reached (no session), say so; never hand-author a decision to make
the demo "work." And never claim a landing the log doesn't show — the injected
attempt fails by design; only the retry should land.