# Skill router

> **For AI agents:** if the user's request doesn't match the skill you're in,
> find the right one below and switch. Tell the user briefly, e.g. *"That's a
> diagnosis task — switching to the `diagnose` skill."*

Copilot's skills form an **operational pipeline**, not a set of independent
tools. Most requests map cleanly to one skill, but several intents look similar
and are easy to misroute — this table is the tie-breaker. Each `SKILL.md`
references this file so you can self-correct mid-task.

## The pipeline order

```
setup → watch → run → inject        (operate)
                 └→ logs → diagnose  (review & recover)
solana-internals                     (shared knowledge, referenced by all)
```

`setup` must succeed before any submission. `watch` reads conditions before a
run. `run` submits and tracks. `inject` is the autonomous-retry demo. `logs`
reads what happened; `diagnose` explains and recovers a failure. If asked to
`run` but `setup` hasn't been verified, do `setup` first.

## Intent → skill

| The user wants to… | Skill |
|---|---|
| connect to RPC/gRPC, set a keypair, verify the chain view is live, fix a connection error | `setup` |
| see the live feed — slots, current leader, tip floor, congestion — or judge whether it's a good time to submit | `watch` |
| submit one or more bundles, price a tip from the live market, track a submission to finality | `run` |
| run the fault-injection / autonomous-retry demonstration | `inject` |
| read the committed lifecycle logs or the agent-reasoning log, summarize a run | `logs` |
| understand **why** a submission failed or didn't land, and what to do about it | `diagnose` |
| understand Solana mechanics (pipeline, QUIC/QoS, banking stage, commitment, blockhash, fees, Jito) | `solana-internals` |

## Common misroutes (read these — they're the whole point)

- **"Why didn't my bundle land?"** → `diagnose`, **not** `logs` or `inject`.
  `logs` reports *what* happened; `diagnose` reasons about *why* and chooses the
  fix. (If they then want to *see* the recorded entry, hand off to `logs`.)
- **"Show me the tip floor / is it congested?"** → `watch`, **not** `run`. `run`
  *uses* the tip floor to price a submission; `watch` is where you read it.
- **"Is it connected / why is nothing showing?"** → `setup` for wiring and
  connection errors; `watch` only once slots are confirmed advancing.
- **"Explain commitment levels / blockhash expiry / how Jito works"** →
  `solana-internals`, **not** the operational skill that happens to use it. Route
  to the specific reference (e.g. `blockhash-lifetime.md`).
- **"Retry / recover this failure"** → `diagnose` decides the response; `inject`
  is only for the *demonstration* of the retry loop, not for handling an
  arbitrary real failure.
- **"Tip strategy / how much should I pay?"** → `diagnose`'s `tip-strategy.md`
  for a *retry* decision; `run`'s `tip-pricing.md` for a *first* submission.

## How to use this router

1. Read the user's request.
2. Match it against **Intent → skill** above; check **Common misroutes** if two
   skills feel plausible.
3. If you're in the wrong skill, say so briefly and switch.
4. For a pure mechanics question, route into `solana-internals` and load the
   specific reference rather than answering from memory.
5. Respect the pipeline order — don't submit before `setup` is verified.

For a human-readable overview of the whole skill set, see `README.md`.