# Leader scheduling

Who produces which slot, how Copilot knows it ahead of time, and why it matters
for whether a bundle can land. This is the mechanism behind the `leader_skipped`
failure kind and the "next Jito window" line in `run`.

---

## The leader schedule

Solana time is divided into ~400ms **slots**, grouped into **epochs** (~2 days).
For each epoch, a **leader schedule** is computed up front, assigning every slot
to exactly one validator (leaders are typically assigned in short consecutive
runs of slots). Because the schedule is known in advance, you can answer "who
leads slot N?" and "when does validator X lead next?" *before* those slots happen
— there's no waiting to find out.

Copilot's `leader` crate:

- fetches the schedule for the current epoch (`getLeaderSchedule`) and the epoch
  bounds (`getEpochInfo`),
- maps slot → leader pubkey (`leader_at(slot)`),
- and finds the next slot led by any validator in a given set within a lookahead
  window (`next_leader_slot(from, &set, max_lookahead)`),
- tracking the current leader as the chain tip advances.

This is the same `getLeaderSchedule` lookup the TPU-client fast path uses to know
where to fanout (`../../solana-internals/references/transaction-pipeline.md`).

---

## Why it matters: Jito-leader-only landing

A Jito bundle can **only** be included by a leader running Jito-enabled software.
So for landing, the schedule question that matters isn't "who's the next leader?"
but "**when is the next Jito-connected leader?**"

- If the upcoming slots are led by non-Jito validators, a bundle submitted now has
  no Jito leader to land on until one comes up.
- If the targeted Jito leader **skips its slot** (produces no block — validators
  skip slots for many reasons: offline, late, delinquent), the bundle is
  **silently dropped**, and the next leader may be non-Jito
  (`../../solana-internals/references/jito-bundles.md`).

Copilot fetches the set of Jito-connected leaders best-effort via a raw
`getConnectedLeaders` call and logs the next Jito window before each submission.
It's observability — it explains *why* a submission landed promptly or had to wait
— not a hard gate.

---

## How this shows up in failures

`leader_skipped` is its own failure kind precisely because it's **not** a fee or
validity problem (`../../diagnose/references/failure-taxonomy.md`):

- signal: the targeted Jito leader produced no block for the window,
- correct response: **wait for the next producing Jito leader and retry; hold the
  tip.** Raising the tip buys nothing — there was no auction to lose, just no
  block to land in.

Confusing a leader skip with under-pricing (and tipping more) is a classic
mismatch, and one the agent should avoid by reasoning from the leader signal
rather than reflexively escalating (`../../diagnose/references/tip-strategy.md`,
example E).

---

## Reading it in `watch`

In `watch`, the leader pubkey rotating each slot is the live view of the schedule.
If you watch for a bit during a run that's struggling to land, and you see long
stretches of non-Jito leaders or visible skips, that's corroboration that the
problem is *leader availability*, not your tip — useful when explaining a run
where landings came in bursts rather than steadily
(`../../logs/references/interpreting-logs.md`).