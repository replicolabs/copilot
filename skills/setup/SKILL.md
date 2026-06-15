---
name: setup
description: Configure Copilot's chain endpoints, payer keypair, and environment, and verify the live feed is actually connected before submitting anything. Use when the user is setting up Copilot for the first time, says the stack "isn't connected" or "isn't streaming", wants to fill in the .env file, needs help choosing or wiring RPC / Yellowstone gRPC / Block Engine endpoints, or asks how to point Copilot at devnet vs mainnet.
---

# setup

Get Copilot wired to a chain and confirm the whole stack is live before anyone
submits a bundle. A stack that isn't streaming can't track lifecycles, so this
verification step is not optional.

> **Routing:** if this isn't the right skill for the request, consult `../SKILL_ROUTER.md` and switch.

## Prerequisites

- The `copilot` binary is installed: `copilot --version` should print. If not,
  build it from the repo with `cargo install --path crates/cli`, or re-run the
  installer (`curl -fsSL https://copilot.asklemma.xyz/install.sh | bash`).
- A funded payer keypair. ~0.1 SOL is plenty — Copilot's default bundles are
  tip-only and cost a tip plus a base fee each.

## Workflow

1. **Interview first — never assume.** Find out what the user actually has:
   - Their RPC URL and their Yellowstone gRPC URL (these are often different
     providers; gRPC is the one that matters most).
   - Whether their gRPC endpoint needs an `x-token`.
   - Where the payer keypair lives — a file path or an inline base58 secret.
   - **Mainnet or devnet.** Real Jito landings only happen on mainnet; devnet is
     for wiring/testing the pipeline. If they expect bundles to land, confirm
     mainnet.
2. **Fill `.env`** from `.env.example` (the installer scaffolds it). Set:
   - `COPILOT_RPC_URL`, `COPILOT_GRPC_URL`, `COPILOT_GRPC_X_TOKEN` (if any),
   - `COPILOT_KEYPAIR`, and leave `COPILOT_BLOCK_ENGINE` at the mainnet default.
   - Leave `ANTHROPIC_API_KEY` alone — the agent inherits the Claude Code session.
3. **Verify the feed is live.** Run a short `copilot watch`. Within a few seconds
   you should see the processed slot advancing and a current leader. Read the
   output with the user; details in `references/connectivity.md`.
4. **Verify finality is moving.** Confirmed and finalized slots should trail the
   processed slot by a small, steady gap and keep climbing. A stalled finalized
   slot means the endpoint or cluster has a problem — flag it, don't submit on it.
5. **Hand off.** Once `watch` shows healthy, advancing slots, the user is ready
   for the `run` skill. If `watch` hangs or errors, work through
   `references/connectivity.md` — the gRPC URL/token is the usual culprit.

## Non-negotiables

- A keypair or x-token must never land anywhere that gets committed. `.env` is
  gitignored; keep the keypair file outside the repo when you can.
- Do not proceed to submitting until `watch` shows live, advancing slots. This is
  the single most common setup failure and the cheapest to catch here.
- Configure against processed/confirmed, never finalized — see
  `../solana-internals/references/blockhash-lifetime.md` for why.

## References

- `references/connectivity.md` — what each endpoint is for, why the stack is
  stream-based, what healthy output looks like, how to read connection failures,
  and the pre-submit checklist.
- `references/environment.md` — every `COPILOT_*` variable, keypair formats,
  mainnet vs. devnet, Block Engine regions, and the inherited agent credential.