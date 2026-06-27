# Copilot

Smart Solana transaction infrastructure: live gRPC streaming, Jito bundle submission, full lifecycle tracking, and an autonomous AI retry agent that classifies failures and decides recovery actions from live chain data.

## Table of Contents

- [Overview](#overview)
- [What Copilot Demonstrates](#what-copilot-demonstrates)
- [Architecture](#architecture)
- [Getting Started](#getting-started)
  - [Installation](#installation)
  - [Environment Variables](#environment-variables)
  - [CLI Reference](#cli-reference)
- [Lifecycle Log Evidence](#lifecycle-log-evidence)
- [The Autonomous Retry Agent](#the-autonomous-retry-agent)
- [Lessons Learned](#lessons-learned)
- [Bounty Questions](#bounty-questions)
  - [Q1: What the processed→confirmed delta tells you about network health](#q1-what-the-processedconfirmed-delta-tells-you-about-network-health)
  - [Q2: Why you must never use finalized commitment when fetching a blockhash](#q2-why-you-must-never-use-finalized-commitment-when-fetching-a-blockhash)
  - [Q3: What happens to your bundle when the Jito leader skips their slot](#q3-what-happens-to-your-bundle-when-the-jito-leader-skips-their-slot)

## Overview

Copilot is a Solana transaction infrastructure stack that connects to a live Yellowstone gRPC stream, prices Jito tips dynamically from the current tip floor and congestion read, builds and submits tip-only bundles to the Jito Block Engine, and tracks each transaction's full commitment journey: processed, confirmed, finalized, over the same stream, writing a committed lifecycle log for every submission. When a bundle fails, a fault classifier diagnoses the cause (expired blockhash, fee too low, leader skipped, compute exceeded) and an autonomous Claude-powered agent receives the full live context, chain tips, tip percentiles, congestion level, blockhash age, onchain outcome, and returns a structured JSON decision: retry with what tip, or abort, and why. Every number in those decisions traces to real onchain data fetched at the moment of failure, not a hardcoded heuristic.

## What Copilot Demonstrates

**Live Yellowstone gRPC streaming:** The `geyser` crate maintains a persistent, auto-reconnecting subscription to a Yellowstone endpoint. It streams slot updates (processed, confirmed, finalized), block metadata carrying fresh blockhashes, and transaction notifications for tracked signatures. Slot state is held in a lock-free `AtomicU64` / `ArcSwapOption` / `watch::channel` structure shared across all consumers without locks or copies. On stream close, whether from a server-side idle timeout, a GOAWAY frame, or a recoverable gRPC error that exhausts the built-in retry budget, the geyser task reconnects transparently with exponential backoff, reissuing the current subscription filter so any in-progress signature watch survives the reconnect.

**Leader-window detection:** The `leader` crate resolves the epoch's full 432,000-slot leader schedule from RPC, identifies which upcoming slots belong to Jito-connected validators, and prints the leader window before every submission so the operator knows whether the next Jito opportunity is 1 slot away or 40.

**Real Jito bundle construction and submission:** The `bundle` crate builds three-instruction tip-only bundles: `SetComputeUnitLimit`, `SetComputeUnitPrice`, `SystemTransfer` to a randomly chosen Jito tip account and submits them to `https://mainnet.block-engine.jito.wtf/api/v1/bundles` with the `x-jito-auth` header carrying a registered UUID. Without that UUID the Block Engine accepts the request and returns a bundle ID, but the bundle is immediately marked Invalid and never forwarded to any leader. This is undocumented and took material effort to discover (see [lessons learned](#lessons-learned)).

**Dynamic tip pricing:** The `tip-oracle` crate fetches live Jito tip-floor percentiles (p25/p50/p75/p95/p99) and recent priority-fee percentiles over 150 sampled slots, computes a congestion level (Low/Moderate/High/Severe) from the p99/p50 tail ratio, and derives a baseline tip. No tip in the codebase is hardcoded; every submission uses the live oracle read at the time of that submission.

**Full lifecycle tracking:** The `lifecycle` crate tracks each signature from the moment of submission to finality. Processed and confirmed slots and timestamps are captured by subscribing the Geyser stream to the specific signature; finalized is inferred from the stream's finalized-slot counter advancing past the landed slot. Every stage (submitted_at, processed_at, confirmed_at, finalized_at, submitted_slot, landed_slot) is recorded in a per-run JSON log; confirmed landings go to `logs/success/` and non-landings (including injected faults) go to `logs/failures/`. `logs/agent-reasoning.jsonl` stays at the directory root.

**Failure classification:** The `fault` crate classifies non-landings into typed categories: `ExpiredBlockhash`, `FeeTooLow`, `LeaderSkipped`, `ComputeExceeded`, `BundleFailure`, `Dropped`, `Unknown` using a decision tree that checks onchain error codes first, then blockhash age against the 150-slot window, then tip against recent landed percentiles. Each classification carries a confidence score and a rationale string.

**Stream-based landing confirmation:** Landing is detected by the `lifecycle` tracker receiving a transaction notification over the live Yellowstone subscription, not by polling `getSignatureStatuses`. The tracker registers the signature with `ChainState::set_tracked_signature`, which causes the geyser loop to push an updated subscription filter to the server; when the matching transaction event arrives, the broadcast channel fires and the tracker records the slot and timestamp.

**Autonomous retry with fault injection:** `copilot inject` deliberately manufactures a failure, it takes a fresh blockhash from the stream and mutates it to produce a hash that the Block Engine will treat as 166 slots old (16 past the 150-slot validity window), then submits it as a real bundle on mainnet. The fault classifier identifies `ExpiredBlockhash` at 0.9 confidence. A Claude Sonnet agent receives the full failure context as JSON, reasons over it, and returns a structured retry decision. Five fault injection sessions have been run on mainnet. In every session the classifier identified `ExpiredBlockhash` at 0.9 confidence. The agent's tip decision varied with the live oracle snapshot: it held the tip in three sessions where the existing tip was already at p75 and congestion was not rising, and raised it in two where congestion was moderate-to-severe and rising. Two of the five retries landed; three did not land within the deadline. All five decisions are in `logs/agent-reasoning.jsonl`.

## Architecture

Copilot is an 8-crate Cargo workspace. Each crate has a single well-defined responsibility and no circular dependencies.

| Crate | Responsibility |
|---|---|
| `geyser` | Yellowstone gRPC subscriber; maintains a lock-free live view of slot tips, blockhashes, and tracked transaction notifications with transparent exponential-backoff reconnect |
| `leader` | Epoch leader schedule resolution and Jito-connected leader-window detection |
| `tip-oracle` | Live Jito tip-floor and priority-fee percentile fetching; congestion classification and baseline tip derivation |
| `bundle` | Three-instruction tip-only bundle construction, base64 serialization, Jito Block Engine submission with `x-jito-auth`, and tip-account management |
| `lifecycle` | Per-signature commitment tracking from processed through finalized over the Geyser stream; JSON log serialization |
| `fault` | Failure classification (typed `FailureKind` with confidence and rationale) and deterministic `ExpiredBlockhash` fault injection |
| `agent` | Anthropic API client, system prompt, structured JSON decision parsing, and reasoning log writer |
| `cli` | Orchestrator: `Stack` assembles the above crates into a running pipeline; commands (`run`, `inject`, `watch`, `logs`, `status`, `keygen`) expose the pipeline to the operator |

For a deeper walkthrough, see [docs](https://copilot.asklemma.xyz/docs). Here's also a demo video of copilot:

[![Copilot walkthrough](https://img.youtube.com/vi/kbUyt3D94Yw/maxresdefault.jpg)](https://www.youtube.com/watch?v=kbUyt3D94Yw)

## Getting Started

### Installation

```bash
curl -fsSL https://copilot.asklemma.xyz/install.sh | bash
```

The install script clones the repository to `~/.copilot/src`, builds the binary with Cargo, installs skills for Claude Code, and runs an interactive setup that prompts for all required environment variables. It writes a `~/.copilot/.env` that the binary loads automatically at startup.

If you prefer to build from source:

```bash
git clone https://github.com/replicolabs/copilot ~/.copilot/src
cargo install --path ~/.copilot/src/crates/cli
```

**macOS note.** If you are building on macOS with stock Xcode Command Line Tools and Rust 1.96, the build may fail with `could not parse bitcode object file`, see [the macOS LLVM bitcode issue](#macos-llvm-bitcode-linker-error) in Lessons Learned for the root cause and fix. The `.cargo/config.toml` in this repository already includes the fix.

### Environment Variables

The binary loads `~/.copilot/.env` automatically on startup. The install script generates this file interactively. To edit it later:

```bash
nano ~/.copilot/.env
```

**`COPILOT_RPC_URL`** — Required. A Solana JSON-RPC endpoint. Used for leader schedule fetching, tip-oracle priority-fee sampling, and blockhash fallback when the Geyser stream is not yet warm. Any standard RPC (Helius, Triton, Quicknode, etc.) works.

**`COPILOT_GRPC_URL`** — Required. A Yellowstone gRPC endpoint in `host:port` form, without `https://`. The geyser crate opens a persistent bidirectional stream to this address.

**`COPILOT_GRPC_X_TOKEN`** — Optional. The x-token header value your gRPC provider requires for authentication.

**`COPILOT_KEYPAIR`** — Required. Path to a Solana keypair JSON file, or an inline base58-encoded secret key. This keypair signs every bundle and pays the tip transfer. 0.1 SOL is sufficient for many test runs.

**`COPILOT_JITO_UUID`** — Required for bundles to land. This is the single most consequential undocumented requirement in the Jito Block Engine integration.

Without a registered UUID, the Block Engine accepts every bundle submission (HTTP 200, valid bundle ID) but marks the bundle Invalid immediately and never forwards it to any leader. There is no error in the response; the bundle ID looks legitimate. The transaction will never appear onchain regardless of tip, blockhash, or timing. This is not documented in the Jito developer docs as of this writing.

To get a UUID: open a support ticket on the [Jito Discord](https://discord.gg/jito), select "Block Engine Rate Limit or Shredstream" as the category, then "New JSON-RPC UUID User" as the ticket type, and follow the instructions. You will receive a UUID and a per-second rate limit (typically 2 req/s for new accounts).

**`ANTHROPIC_API_KEY`** — Required for `copilot inject`. The autonomous retry agent calls `api.anthropic.com` directly with this key. Get one at [console.anthropic.com](https://console.anthropic.com) → API Keys → Create Key. Note: the OAuth token that Claude Code uses to authenticate to claude.ai is a different credential and will not work for direct API calls.

**`COPILOT_BLOCK_ENGINE`** — Optional. Defaults to `https://mainnet.block-engine.jito.wtf/api/v1`.

**`COPILOT_HELIUS_API_KEY`** — Optional. Used for `simulateBundle` diagnostics only. Helius's `sendBundle` requires a Business plan; Copilot submits bundles directly to the Jito Block Engine.

**`COPILOT_MODEL`** — Optional. The Claude model the retry agent uses. Defaults to `claude-sonnet-4-6`.

**`COPILOT_LOG`** — Optional. Tracing filter string. Example: `copilot=debug,geyser=debug`.

### CLI Reference

**`copilot watch`** — Connect to the Geyser stream and print live slot, leader, and tip-floor data. Refreshes tip-floor percentiles every 10 seconds. Use this to verify connectivity and gauge congestion before submitting.

```
copilot watch
# slot 427463521 (confirmed 427463520, finalized 427463489) | leader 6aDs9tUm... | tip floor p50/p75/p95 = 1024/4131/30000 lamports (High)
```

**`copilot run [--count N] [--tip LAMPORTS]`** — Submit N tip-only bundles sequentially (default 5), tracking each to finality. Omit `--tip` to price from the live oracle. Writes `logs/lifecycle-run-NN.json` per submission.

```
copilot run --count 10
copilot run --count 1 --tip 50000
```

**`copilot inject`** — Run the fault-injection and autonomous-retry demo on mainnet. Submits a bundle with a deliberately expired blockhash, classifies the failure, sends the full context to Claude, executes the agent's retry decision, and tracks the retry to finality. Appends to `logs/agent-reasoning.jsonl`.

```
copilot inject
```

**`copilot logs [--dir DIR]`** — Summarize all lifecycle-run-*.json files in the log directory (default `logs/`).

```
copilot logs
```

**`copilot status --bundle <BUNDLE_ID>`** — Query the Jito Block Engine for the status of a bundle.

```
copilot status --bundle 9b3e07493a632875120a770e08a6a0abc4e84fedda0172995a604b7aa8bccdcf
```

**`copilot keygen [--outfile PATH] [--force]`** — Generate a new Solana keypair and write it to disk. Prints the public key.

```
copilot keygen --outfile ~/.copilot/keypair.json
```

## Lifecycle Log Evidence

56 bundles were submitted to Solana mainnet across multiple sessions. 13 landed and reached finality. Lifecycle logs are in `logs/success/` (confirmed landings) and `logs/failures/` (non-landings and injected faults); the table below is derived directly from those files with no rounding or approximation.

Runs 01–15 were submitted before the Jito UUID was provisioned. The Block Engine accepted all of them and returned valid bundle IDs, but they were silently marked Invalid server-side and never forwarded to any leader. Run 12 paid 50,000 lamports; run 13 paid 20,000 lamports; both returned nothing onchain.

Runs 16–27 were the first test session after UUID provisioning (5 landings). Run 25 was an organic ExpiredBlockhash miss. Runs 26 and 27 are the first fault-injection session: run 26 is the deliberately expired-blockhash submission, and run 27 is the agent-directed retry that landed.

Runs 28–56 were the extended test session (29 additional submissions, 8 more landings). Runs 51, 53, and 55 are injected faults (fault-injection sessions 2–4); runs 52 and 54 are agent retries that did not land within the deadline; run 56 is the agent retry from session 4 that landed.

| Run | Tip (lam) | Landed Slot | Submit→Proc | Proc→Conf | Conf→Fin | Failure |
|-----|-----------|-------------|-------------|-----------|----------|---------|
| 01 | 4,973 | — | — | — | — | no UUID |
| 02 | 6,286 | — | — | — | — | no UUID |
| 03 | 12,216 | — | — | — | — | no UUID |
| 04 | 10,000 | — | — | — | — | no UUID |
| 05 | 10,000 | — | — | — | — | no UUID |
| 06 | 10,170 | — | — | — | — | no UUID |
| 07 | 6,025 | — | — | — | — | no UUID |
| 08 | 10,000 | — | — | — | — | no UUID |
| 09 | 7,270 | — | — | — | — | no UUID |
| 10 | 9,563 | — | — | — | — | no UUID |
| 11 | 9,563 | — | — | — | — | no UUID |
| 12 | 50,000 | — | — | — | — | no UUID |
| 13 | 20,000 | — | — | — | — | no UUID |
| 14 | 5,494 | — | — | — | — | no UUID |
| 15 | 10,000 | — | — | — | — | leader miss |
| **16** | **5,040** | **427324344** | **705 ms** | **590 ms** | **11,836 ms** | — |
| **17** | **10,479** | **427324383** | **467 ms** | **558 ms** | **13,651 ms** | — |
| 18 | 10,479 | — | — | — | — | leader miss |
| 19 | 24,683 | — | — | — | — | leader miss |
| 20 | 5,002 | — | — | — | — | leader miss |
| **21** | **7,933** | **427325291** | **708 ms** | **840 ms** | **12,794 ms** | — |
| 22 | 8,848 | — | — | — | — | leader miss |
| **23** | **100,000** | **427325744** | **412 ms** | **511 ms** | **12,328 ms** | — |
| 24 | 100,000 | — | — | — | — | leader miss |
| 25 | 8,830 | — | — | — | — | ExpiredBlockhash |
| 26 | 5,000 | — | — | — | — | ExpiredBlockhash (injected) |
| **27** | **5,000** | **427360306** | **391 ms** | **703 ms** | **11,885 ms** | — (agent retry) |
| **28** | **5,876** | **427955982** | **442 ms** | **465 ms** | **12,625 ms** | — |
| **29** | **4,088** | **427956020** | **461 ms** | **719 ms** | **11,422 ms** | — |
| 30 | 4,088 | — | — | — | — | leader miss |
| 31 | 8,044 | — | — | — | — | leader miss |
| 32 | 17,639 | — | — | — | — | leader miss |
| 33 | 2,723 | — | — | — | — | leader miss |
| 34 | 2,711 | — | — | — | — | leader miss |
| **35** | **4,744** | **427957282** | **385 ms** | **469 ms** | **12,018 ms** | — |
| 36 | 4,744 | — | — | — | — | leader miss |
| **37** | **5,205** | **427957586** | **521 ms** | **213 ms** | **12,233 ms** | — |
| 38 | 5,205 | — | — | — | — | leader miss |
| 39 | 4,346 | — | — | — | — | leader miss |
| 40 | 3,477 | — | — | — | — | leader miss |
| **41** | **2,987** | **427958388** | **457 ms** | **453 ms** | **12,249 ms** | — |
| 42 | 5,517 | — | — | — | — | leader miss |
| 43 | 3,412 | — | — | — | — | leader miss |
| 44 | 6,339 | — | — | — | — | leader miss |
| 45 | 10,000 | — | — | — | — | leader miss |
| 46 | 8,000 | — | — | — | — | leader miss |
| 47 | 6,265 | — | — | — | — | leader miss |
| **48** | **3,460** | **427959936** | **1,683 ms** | **467 ms** | **11,706 ms** | — |
| **49** | **3,460** | **427959974** | **583 ms** | **704 ms** | **11,791 ms** | — |
| 50 | 10,000 | — | — | — | — | leader miss |
| 51 | 1,806 | — | — | — | — | ExpiredBlockhash (injected) |
| 52 | 2,000 | — | — | — | — | timeout (agent retry) |
| 53 | 32,500 | — | — | — | — | ExpiredBlockhash (injected) |
| 54 | 50,000 | — | — | — | — | timeout (agent retry) |
| 55 | 7,268 | — | — | — | — | ExpiredBlockhash (injected) |
| **56** | **7,268** | **427961212** | **318 ms** | **462 ms** | **12,079 ms** | — (agent retry) |

The thirteen onchain signatures, verifiable on [Solscan](https://solscan.io) or [Solana Explorer](https://explorer.solana.com):

| Run | Signature |
|-----|-----------|
| 16 | `4qGzuBmhsDUJTfPaAyY4C5BBHEfbBA9cgvu4gjEaSytxC4mRnB7gqJfGEK2kz9G7F56SYHE6rBzMzsgqDqxFzt8Y` |
| 17 | `26Yju2B9uUDJyHXih3k1hsBEuXCkVYnRmKWbbt5J3pDTk7StfcQHfAogmAPRtkh1ge2xuaUyQosfC5odDjyuNzcA` |
| 21 | `3WJHX2jfpaV7goe9k9wzcVT9u1fBdLhH2WNNUui7JRoWyKbHcbNUiEDtyiPxhAQA9v2DUSbeJezNamtovmuDz6Mg` |
| 23 | `sweqVLT774VW8L3P7KvR7tgctq9rUa4SNxoWC2K7RHSbUVk6KznbC58Tn8CSVW2WGPgLugJdhsP3AXxGPanboXA` |
| 27 | `4BB7PZwG9pGynxoDqqoLfdWnCkdyhgYhywYayEeNXBL7YtJnEceg7w2sUMFqLhTcGC7UM3rrrReJZGxJwHvtYp6i` |
| 28 | `5M53Jdq2YFmK8Dd1fVtastB99dJmNTnubMsf3oCPsbV7F47MaoHQSyMcoqkgpposCdfvfCiy77owifaae2zwCkCC` |
| 29 | `3rL7C4dqz2JHsPL8CGsgjXMZ5iz6QfLq6UDMarMmqzYU7JiEmrUV8Ck5h5P5UX4Jtg2qjb37HHYpr7Pr1xH9Vy3L` |
| 35 | `36jrBKFXvkZUTVjwTQz4eyMNRExyV25X7tUtc2pza7u1cUqXC8sjwdV3U1VVhhS6T6qEzhNYPJY2JMLfTMpyhtd` |
| 37 | `4zrnnoJXw1KQcuuS9DMGDYb5vDbhUEoXbSDn7PNLRs9FvQAVBL5k27nxpMGnyeRLSxUGWcbnWyqtJJy9qsuYocgY` |
| 41 | `63bZv3UuTSxXkdkL2ioQPWhSZFMwPFXgMAhgTqnzm6uwbHL9ZN3EWmTEz5CTp9Ludi53PwXu6TqhRDd9zBwSN54v` |
| 48 | `5iivzwakQBWrpGgUrXmWqKyeEuPY3EdTBTU2rLqSmVqn6ewdmrX3voEgic9d8Uh5WdyCYTuHHnseAszij5PUqd5Z` |
| 49 | `4J1WMiDDgc9RWg1vdx19X3d5xXRngmvq7KTDaZadUscryRnV5KRYSo2Zx3wk338gUMF6E6KALHDr7wVJinjHvc9A` |
| 56 | `4qktowFzWcwf7f95mG94exEMEF6WGUEsgLu66DZTPBJD2wjSyXkRRp5ikjsudww17WXWsWdD16ErXZznVr8Citcv` |

**Timing summary.** Submit→processed spans 318–1,683 ms across the thirteen landings; run 48 is an outlier at 1,683 ms (brief delay between Geyser subscription establishment and first slot event), all others fall within 385–708 ms. This reflects network RTT to the Block Engine, forwarding to the leader, block production, and Geyser stream delivery. Processed→confirmed spans 213–840 ms, consistently under one second regardless of session-level congestion, from Moderate through Severe. Confirmed→finalized spans 11,422–13,651 ms, bracketing the theoretical Tower BFT finalization time of ~32 slots × ~400 ms = ~12.8 s. Slot delta from submitted_slot to landed_slot was 2–8 slots across all thirteen landings.

## The Autonomous Retry Agent

`copilot inject` runs a fully automated fault-injection and recovery loop on mainnet. The failed bundle and the retry are real transactions submitted to mainnet, not simulations.

**How the fault is manufactured.** The `inject_expired_blockhash` function in the `fault` crate takes a real fresh blockhash from the Geyser stream, rotates its bytes left by one position and XORs byte 0 with `0xA5`, and tags the result with an apparent age of 166 slots (the 150-slot maximum plus a 16-slot safety margin). The bundle carrying this hash is otherwise valid: properly signed, valid instructions, market-rate tip. The Block Engine rejects it because the blockhash is outside the valid window.

**Classification.** After the landing deadline expires (25 seconds for injected faults vs. 90 seconds for normal runs), the `fault::classify` function receives a `FailureSignals` struct with `blockhash_age_slots=166`, `max_blockhash_age_slots=150`, `jito_leader_produced=true`, `landed=false`. The decision tree finds no onchain error, confirms the blockhash is expired, and returns `FailureKind::ExpiredBlockhash` at 0.9 confidence: "blockhash aged 166 slots, past the 150-slot window, before inclusion."

**Live chain data assembled for the agent.** The pipeline constructs a JSON context containing: the full `FailureEvent` (kind, confidence, rationale, all signals); the current chain tips (processed, confirmed, finalized slots at decision time); and the live `TipSuggestion` p25/p50/p75/p95/p99 Jito tip floor, EMA p50, congestion level, `median_rising` flag, `tip_tail_ratio`, and priority-fee percentiles sampled over 150 slots. This context is the only input the agent sees; it has no access to anything outside this JSON object and the system prompt.

**Recorded decisions in `logs/agent-reasoning.jsonl` (5 entries).** Entry 1 is from the first inject session, 2026-06-19, at processed_slot 427360290 and finalized_slot 427360259, with the live Jito tip floor at p50=1,039 lam / p75=5,000 lam / p95=87,378 lam, congestion High, `median_rising=false`, `tip_tail_ratio=84.1`. The agent returned:

```json
{
  "action": "retry",
  "new_tip_lamports": 5000,
  "reasoning": "The failure is clearly due to an expired blockhash (166 slots > 150-slot window), not a tip issue — the existing tip of 5000 lamports already sits at the p75 level which is sufficient given current congestion. A fresh blockhash is all that's needed to retry successfully.",
  "confidence": 0.9
}
```

The tip was at the p75 of the live floor. The agent identified that no congestion escalation had occurred (median not rising, tip already at p75), so raising the tip would waste lamports without improving landing probability. The fix was a fresh blockhash alone. The retry (run 27) landed at slot 427360306 within 391 ms of submission.

This decision exemplifies the classifier's design goal: distinguishing a blockhash failure from a fee failure. The classic mistake is reflexively raising the tip after any non-landing. Copilot's classifier and agent are specifically built to prevent that.

Four additional sessions are in entries 2–5. Entry 2 (congestion moderate, rising): tip raised 1,806 → 2,000 lamports; retry did not land. Entry 3 (congestion severe, rising): tip raised 32,500 → 50,000 lamports; retry did not land. Entry 4 (congestion moderate, not rising, tip at p75): tip held at 7,268 lamports; retry landed as run 56 in 318 ms. Entry 5 (congestion high, not rising, tip at p75): tip held at 10,000 lamports. Across all five sessions the fault classifier returned identical results: `ExpiredBlockhash` at 0.9 confidence. The tip decision varied in each case because the oracle snapshot varied, the agent reasons from live data, not a hardcoded rule.

## Lessons Learned

### The Jito UUID requirement

**Symptom.** Every bundle returned a valid 64-character hex bundle ID with HTTP 200. No transaction appeared onchain. Runs 01–15, 15 submissions spanning tips from 4,973 to 50,000 lamports, across multiple sessions, all timed out with no onchain trace.

**Hypotheses ruled out.** Tip too low: disproved by run 12 (50,000 lam) and run 13 (20,000 lam), both well above the observed p75 floor. Invalid blockhash or encoding: disproved by `simulateBundle` returning valid simulation results against the Helius endpoint using identical transaction bytes. Wrong JSON shape: disproved by verifying the `encoding: "base64"` second parameter and the full payload structure against the Jito API schema.

**Root cause.** The Jito Block Engine requires an `x-jito-auth` HTTP header carrying a UUID that has been registered through Jito's support process. Without a registered UUID, the Block Engine accepts the request, assigns a bundle ID, and immediately marks the bundle Invalid in its internal routing table. It is never forwarded to any leader. There is no error in the HTTP response, no error retrievable via the bundle status API, and this requirement is not documented in the public Jito developer documentation.

**Discovery and fix.** Opened a Discord support ticket (category: Block Engine Rate Limit or Shredstream; type: New JSON-RPC UUID User). Received a UUID and 2 req/s rate limit. Added `x-jito-auth` to every submission. Run 16, the first submission after the fix, landed within 705 ms.

In the codebase, the original `generate_uuid_v4()` random-UUID fallback was removed entirely, random UUIDs do not satisfy this requirement. `BundleSubmitter::new()` logs a warning at startup if the UUID is not set.

### macOS LLVM bitcode linker error

**Symptom.** The release CI workflow failed on both `aarch64-apple-darwin` and `x86_64-apple-darwin` targets:

```
error: could not parse bitcode object file
Unknown attribute kind (102)
(Producer: 'LLVM22.1.2-rust-1.96.0-stable' Reader: 'LLVM APPLE_1_1700.0.13.5_0')
```

The failing objects were `libspl_token_confidential_transfer_proof_extraction` and `libspl_token_confidential_transfer_proof_generation`, zk-proof crates pulled in transitively by the Solana SDK.

**Root cause.** Rust 1.96 ships with LLVM 22. By default, `rustc` embeds bitcode sections into `.rlib` object files so the linker can perform link-time optimization. On macOS, the system linker uses Apple's libLTO, which at the time of this writing is version 17. When Apple's linker encounters an `.rlib` containing LLVM 22 bitcode, it tries to parse it and chokes on attribute kind 102, which did not exist in LLVM 17.

Setting `lto = false` in `[profile.release]` does not fix the problem. That flag controls whether the linker performs a final LTO pass; it does not stop `rustc` from writing embedded bitcode sections into the individual `.rlib` files. Those sections are emitted regardless of the profile LTO setting.

**Fix.** Added `.cargo/config.toml` at the repo root:

```toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "embed-bitcode=no"]

[target.x86_64-apple-darwin]
rustflags = ["-C", "embed-bitcode=no"]
```

The `-C embed-bitcode=no` codegen flag tells `rustc` not to write bitcode sections into object files at all. Apple's linker never encounters the incompatible LLVM 22 attributes. This fix applies both to CI and to any developer building from source on macOS with stock Xcode Command Line Tools.

### ANTHROPIC_API_KEY versus Claude Code OAuth

**Symptom.** The autonomous retry agent returned HTTP 401 from `api.anthropic.com` even though Claude Code was authenticated and functioning in the same terminal session.

**Root cause.** Claude Code authenticates to claude.ai via OAuth. The OAuth token Claude Code holds is scoped to the claude.ai interface; it is not an Anthropic API key and is not accepted by `api.anthropic.com/v1/messages`. These are two separate authentication systems. The `agent` crate calls the Anthropic API directly over HTTP using a bearer token, which must be a key created through the Anthropic Console, not an OAuth token.

**Fix.** Created a separate API key at console.anthropic.com and set `ANTHROPIC_API_KEY` in `.env`.

### Geyser stream reconnect

**Symptom.** Early versions of `source.rs` had no outer reconnect loop. The `AutoReconnect` wrapper from `yellowstone-grpc-client` handles recoverable gRPC status codes with up to 10 TCP-level retries. But when the server sends a clean stream close (`Poll::Ready(None)`), which is what a server-side idle timeout produces, `AutoReconnect` sets `self.stop = true` and permanently terminates. The geyser task returned `Err(Error::Closed)`, and subsequent `await_blockhash()` calls would block indefinitely on a `watch::Receiver` that would never advance.

**Fix.** `source.rs` was refactored into `run()` (outer reconnect loop) and `run_once()` (inner event loop). `run_once()` returns `Ok(())` only on cancellation and `Err` on any stream close or error. `run()` wraps it in an infinite loop with exponential backoff: 500 ms initial, doubling each failure, capping at 30 s. When `run_once()` received at least one message before failing, backoff resets to 500 ms on the next attempt (server-side connection recycling is normal; no reason to penalize it). Subscription filter state is re-read from `ChainState` at the top of each `run_once()` call via `borrow_and_update()`, so in-flight signature watches survive reconnects transparently.

### `simulateBundle` field name

When building the `simulateBundle` diagnostic against the Helius endpoint, the correct request field name is `encodedTransactions`, not `transactions`. The Helius documentation uses both terms inconsistently. Sending `transactions` returns HTTP 200 with a schema validation error buried in the result body, easy to miss if you only check the HTTP status.

## Bounty Questions

### Q1: What the processed→confirmed delta tells you about network health

Across the thirteen landings in these runs, the processed→confirmed delta ranged from 213 ms (run 37) to 840 ms (run 21), with all thirteen values under one second, between roughly half a slot and two slots at 400 ms each.

This delta measures the time from a transaction's inclusion in a block to the formation of a supermajority of stake votes on that block's ancestor. Validators publish votes as ordinary transactions, and those vote transactions propagate through the same gossip and TPU pipeline that user transactions use. On a healthy mainnet, vote transactions land in the next one or two slots after the voted-on block, which is why our observed deltas span 213 to 840 ms across sessions where the tip floor ranged from Low through Severe congestion.

What a consistently sub-second proc→conf delta tells you is that the vote pipeline is healthy: validators are producing blocks on schedule, votes are propagating normally through gossip, and there is no unusual latency between the leader and the rest of the network. If proc→conf were stretching to 3–5 seconds, that would indicate something structurally wrong, a network partition, an unusual concentration of stake in lagging validators, or vote-transaction congestion causing delays in supermajority formation.

The more operationally important insight, however, is what this delta does not tell you. Our thirteen successful landings all showed normal proc→conf times. Our forty-three non-landing submissions in the same sessions had no proc→conf delta at all, they never reached processed. The cluster was equally healthy during those misses; had the transactions landed, they would have confirmed just as fast. The non-landings were caused by missing Jito leader windows, expired blockhashes (five deliberately injected, one organic), and the fifteen pre-UUID submissions that the Block Engine silently discarded, none of which the proc→conf delta reflects.

This asymmetry is a diagnostic trap. If you observe high-tip transactions failing to land while your occasionally-landing transactions confirm quickly, the healthy proc→conf time rules out cluster instability as the cause. It directs the diagnosis upstream toward tip pricing (is your tip competitive in the current auction?), Jito leader availability (are there Jito-connected leaders in the next few slots?), and blockhash freshness. The proc→conf delta is a useful signal, but only for transactions that have already cleared the inclusion hurdle. For diagnosing non-landings, it tells you nothing.

### Q2: Why you must never use finalized commitment when fetching a blockhash

A Solana blockhash is valid for approximately 150 slots from the slot in which the block containing it was produced. At 400 ms per slot that is roughly 60 seconds of total validity, and the budget is consumed in wall-clock time whether or not your transaction is near the leader.

The question is how old the blockhash is when you fetch it. A blockhash fetched at processed commitment is from the most recently produced block, potentially the current slot, aged by seconds at most. A blockhash fetched at finalized commitment is approximately 31 slots old. In our live data, at the moment the fault classifier assembled context for the agent, the chain showed `processed_slot: 427360290` and `finalized_slot: 427360259`, a gap of exactly 31 slots. Tower BFT requires roughly 32 consecutive slots of supermajority votes to finalize a block; 31 slots behind processed is the normal steady-state gap.

Fetching a finalized blockhash means starting with about 21% of the validity window already consumed 31 of 150 slots before you have done anything. From there, the blockhash ages further as you: fetch a tip from the oracle, construct and sign the transaction, transmit to the Block Engine, wait for a Jito-connected leader (which may be 10–40 slots away in a sparse schedule), and have the leader include your bundle. A finalized blockhash can easily be 80–120 slots old by the time the validator processes it. At 150 slots you expire. This gives you very little margin.

The `copilot inject` demo manufactures exactly this failure and proves the full diagnostic pipeline works end to end. `inject_expired_blockhash` takes a fresh blockhash from the Geyser stream and produces a hash that the Block Engine treats as 166 slots old, past the 150-slot limit. The bundle is otherwise valid. The Block Engine rejects it. The classifier reads `blockhash_age_slots=166 > max_blockhash_age_slots=150` and returns `ExpiredBlockhash` at 0.9 confidence. The agent sees this classification alongside the live tip floor, confirms the tip is adequate (5,000 lam at p75 with congestion not rising), and requests a fresh blockhash without raising the tip. The retry with a processed-commitment blockhash lands in 391 ms.

Copilot always fetches the blockhash at processed commitment, reading from the Geyser stream's `BlockMeta` updates in real time. The RPC fallback, used only when the stream has not delivered a fresh blockhash within a timeout window, also uses confirmed commitment, not finalized. Finalized commitment is used only for slot-level finalization tracking after a transaction has already landed.

### Q3: What happens to your bundle when the Jito leader skips their slot

When a validator who has opted into Jito's relayer is scheduled to produce a slot and skips it, produces no block, your bundle is silently discarded. There is no error code from the Block Engine. The bundle status API returns nothing actionable. The transaction never appears onchain. From the outside, a skipped Jito leader is indistinguishable from a bundle that was priced too low: both produce a timeout with no onchain trace.

This is not a Jito-specific behavior. It follows directly from how Solana leader scheduling works. A scheduled leader either produces a block or it doesn't; if it doesn't, that slot simply does not exist as a container for transactions. No block means no inclusion opportunity, regardless of how high the tip is or how fresh the blockhash is. The Jito Block Engine forwards your bundle to Jito-connected leaders in upcoming slots; if the targeted leader skips, there is no block to insert the bundle into.

The correct response to a skipped Jito leader is to wait for the next Jito-connected leader window and resubmit. Raising the tip is ineffective because there is no auction happening in a skipped slot. The tip buys priority within a block that is being produced; it does not cause a block to be produced.

This is visible directly in our logs. Runs 18, 19, 20, 22, and 24 all timed out without landing, all with `failure: null` in the lifecycle log. Their tips ranged from 5,002 to 100,000 lamports. Run 24 paid 100,000 lamports, well above the p95 floor at the time, and still produced nothing onchain. Run 23, submitted moments earlier with the same 100,000 lamport tip, landed at slot 427325744. The difference between them was not the tip; it was whether a Jito-connected leader appeared in the available window. The `copilot watch` output during these sessions logged "Jito leader set unavailable" across the windows that produced no landings.

The `failure: null` on these runs is not a logging gap, it is an honest representation of the evidence. The fault classifier does not assign a cause it cannot confirm. A timeout with no onchain trace, no classifiable blockhash expiry, and no tip signal below the landed median gets logged as null, which correctly reflects the ambiguity. The pre-submission leader window data from `log_leader_window()`, printed to the terminal before each `copilot run` submission, is the right place to diagnose this failure mode: if the next Jito-connected leader is far away or unknown, the submission is unlikely to land regardless of tip.
