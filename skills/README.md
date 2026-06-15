# Copilot skills

This directory is Copilot's knowledge layer: a set of **agent skills** that teach
an AI agent how to operate the stack and reason about Solana transaction
infrastructure. It follows the [agentskills.io](https://agentskills.io) format.

## How routing works

There is **no router file**. Each skill's `SKILL.md` carries a `description` in
its frontmatter stating *what the skill does and when to use it*. At startup the
agent loads only those descriptions (the cheap routing tier) and activates a
skill when a task matches its description. Activating a skill loads its full
`SKILL.md` (the workflow); the dense `references/` files load only when that
`SKILL.md` points the agent to them and it decides it needs the detail. So the
descriptions **are** the router — keep them accurate and this index in sync.

## The skills

| Skill | Use it when you need to… |
|---|---|
| **setup** | connect Copilot to an RPC + Yellowstone gRPC endpoint and a keypair, and verify the chain view is live before submitting anything |
| **watch** | read the live feed — slot tips, current leader, the Jito tip floor, and the congestion read — to judge conditions before a run |
| **run** | build, price (from the live tip floor, never hardcoded), submit, and track a Jito bundle across processed → confirmed → finalized |
| **inject** | run the flagship demo: inject a real expired-blockhash failure, have the agent reason over live chain data, and execute its structured retry decision |
| **logs** | read the committed lifecycle logs and the agent-reasoning log, and explain a run honestly from the recorded slots |
| **diagnose** | classify a failed or non-landing submission by cause and choose the right response — match the fix to the cause, not "retry harder" |
| **solana-internals** | the shared knowledge base (TPU pipeline, QUIC/SWQoS, Banking Stage/Sealevel, commitment, blockhash lifetime, fees/compute, Jito bundles) that every other skill cross-references |

`solana-internals` is the foundation: the operational skills link into its
references rather than restating the mechanics, so the "why" lives in one place.

## Layout

```
skills/
  <skill>/
    SKILL.md            frontmatter (name + description) + workflow + references list
    references/         dense, load-on-demand detail (no length cap)
```

19 reference documents in total. Every reference is listed in its parent
`SKILL.md`, and every cross-reference link between them resolves.

## The core idea

Copilot's distinguishing behavior — autonomous retry with fault injection — is a
closed loop of **live chain data → model reasoning → structured JSON decision →
execution**, with the failure, the decision, and the recovery all committed to
`logs/`. The `inject`, `diagnose`, and `solana-internals` skills together carry
the knowledge that makes that decision a real one rather than a hardcoded rule.