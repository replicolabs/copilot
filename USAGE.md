#  Usage and Operations

This is the usage and operations manual for Copilot. It covers every step from a fresh machine to a confirmed onchain landing: prerequisites, installation, configuration, keypair generation, the complete command interface, a first-run walkthrough, and a troubleshooting reference. All commands and flags documented here are derived directly from the CLI source in `crates/cli/src/cli.rs` and `crates/cli/src/commands.rs`.

## Prerequisites

| Requirement | Notes |
|---|---|
| Rust toolchain 1.96.0 | Pinned in `rust-toolchain.toml`. `rustup` installs the correct version automatically when you first run `cargo` inside the repo. If `rustup` is not installed, obtain it from rustup.rs. |
| git | Required by the install script to clone and update the repo. Any recent version is sufficient. |
| curl or wget | Required by the install script for downloading. One of the two must be present. |
| Solana mainnet RPC endpoint | Used for the tip oracle (`getRecentPrioritizationFees`) and leader schedule lookups. Any public or private mainnet endpoint works. |
| Yellowstone gRPC endpoint | The live chain feed. Must be a mainnet Geyser node. The endpoint format is `host:port`; port 443 is auto-promoted to TLS by the binary. |
| A funded Solana keypair | Tip-only bundles are cheap: a 56-run session including 13 landings and five fault injection sessions consumed less than 0.1 SOL total. Fund the keypair address before running any submission command. |
| COPILOT_JITO_UUID | Required for bundles to reach finality. Without it, every bundle receives HTTP 200 from the Block Engine but is silently marked Invalid. Obtain one via a Jito Discord support ticket. |
| ANTHROPIC_API_KEY | Required only for `copilot inject`. The autonomous retry agent calls the Anthropic API directly; the Claude Code CLI session is not a substitute. |

## Installation

### The Install Script

The one-liner installs the full stack without any prior setup:

```bash
curl -fsSL https://copilot.asklemma.xyz/install.sh | bash
```

The script performs the following steps in order:

**1. Prerequisite check.** Verifies that `git` and at least one of `curl` or `wget` are available. Exits with an error message if either is missing.

**2. Claude Code check.** Looks for the `claude` command. If missing and `npm` is available, it runs `npm install -g @anthropic-ai/claude-code`. If `npm` is unavailable, it prints a warning and continues -- Claude Code is used by the skill interface but is not required for the core binary commands.

**3. Rust/cargo check.** Verifies that `cargo` is on PATH. If missing, it warns that the build step will be skipped and provides the rustup.rs URL. The script continues; you can build later with `cargo install --path ~/.copilot/src/crates/cli` once Rust is installed.

**4. Repository clone or update.** Clones the repo to `~/.copilot/src` (controlled by `COPILOT_HOME` if set). If `~/.copilot/src/.git` already exists, it runs `git pull --ff-only` instead.

**5. Skill installation.** Copies the contents of `skills/` to `~/.claude/skills/`. If the `codex` command is found, skills are also installed to `~/.codex/skills/`. This gives Claude Code access to the `watch`, `run`, `diagnose`, and `inject` skill prompts.

**6. Claude Code permissions scaffold.** If `~/.claude/settings.json` does not exist, writes a minimal permissions file granting `Bash`, `Read`, `Glob`, and `Grep` access.

**7. Binary build and install.** Runs `cargo install --path ~/.copilot/src/crates/cli`, which builds a release binary and installs it to `~/.cargo/bin/copilot`.

**8. Interactive configuration.** Launches a guided `.env` setup wizard. The wizard asks for:

- **COPILOT_RPC_URL** -- your Solana mainnet JSON-RPC endpoint.
- **COPILOT_GRPC_URL** -- your Yellowstone gRPC endpoint (`host:port` format, e.g. `fra.grpc.solinfra.dev:443`).
- **COPILOT_GRPC_X_TOKEN** -- the gRPC provider's auth token, if required. Press Enter to skip.
- **Keypair** -- choose `[1]` to generate a new keypair at `~/.copilot/keypair.json`, or `[2]` to provide the path to an existing one. If you choose `[1]`, the wizard prints the new public key and reminds you to fund it.
- **COPILOT_JITO_UUID** -- printed with an explanation of why it is required and how to obtain it. Press Enter to skip and set it later.
- **ANTHROPIC_API_KEY** -- printed with a note that the agent will not run without it. Press Enter to skip.

The written `.env` is placed at `~/.copilot/.env` with permissions `600` (owner read/write only). Any values left blank are listed as a reminder at the end of the wizard. You can edit the file at any time with `nano ~/.copilot/.env` or your preferred editor.

If the script is run non-interactively (no `/dev/tty`), the wizard is skipped and the `.env` template is written with empty values.

### Building from Source

To build without the install script:

```bash
git clone https://github.com/replicolabs/copilot ~/.copilot/src
cd ~/.copilot/src

# Rust 1.96.0 is declared in rust-toolchain.toml.
# rustup installs the correct version automatically on first cargo invocation.
cargo build                          # debug build
cargo build --release                # optimized build
cargo install --path crates/cli      # installs to ~/.cargo/bin/copilot
```

**Why the toolchain is pinned to 1.96.0.** The workspace uses Rust 2024 edition features, including `if let` chains in `&&` expressions and async closures. These features were stabilized in specific minor releases and their syntax is edition-gated. Pinning to 1.96.0 avoids silent breakage from future edition boundary changes and ensures that contributors and CI agree on exactly one compiler.

**macOS note: `.cargo/config.toml` and the embed-bitcode flag.** The repo includes a `.cargo/config.toml` with the following entries:

```toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "embed-bitcode=no"]

[target.x86_64-apple-darwin]
rustflags = ["-C", "embed-bitcode=no"]
```

Rust 1.96 uses LLVM 22, which embeds bitcode in `.rlib` files using attribute kind 102. The Apple system linker is tied to an older LLVM version that cannot parse this attribute and produces a link error at the final binary step. Passing `-C embed-bitcode=no` suppresses bitcode embedding for macOS targets, resolving the error without any runtime impact. This setting is active automatically when building inside the cloned repo.

## Configuration

The binary loads environment variables from two locations at startup, in order:

1. A `.env` file in the current working directory (standard `dotenvy` behavior).
2. `$COPILOT_HOME/.env`, falling back to `~/.copilot/.env` if `COPILOT_HOME` is unset.

Variables from the working-directory `.env` take precedence. You can run `copilot` from any directory and the `~/.copilot/.env` file will always be found.

| Variable | Required | Default | Description |
|---|---|---|---|
| `COPILOT_RPC_URL` | Yes | -- | Solana JSON-RPC endpoint. Used by the tip oracle (`getRecentPrioritizationFees`) and leader schedule fetching. Any mainnet RPC endpoint works. |
| `COPILOT_GRPC_URL` | Yes | -- | Yellowstone gRPC endpoint. Accepted formats: `host:port` or `https://host:port`. Port 443 is automatically prefixed with `https://`; all other bare `host:port` strings are prefixed with `http://`. If the URL already contains `://`, it is used as-is. |
| `COPILOT_GRPC_X_TOKEN` | Yes | -- | Auth token for the gRPC provider. Sent as the `x-token` HTTP/2 header. Separate from the URL -- do not embed the token in `COPILOT_GRPC_URL`. |
| `COPILOT_KEYPAIR` | Yes | -- | Payer keypair. Either a filesystem path to a JSON file containing a 64-byte ed25519 keypair, or an inline base58-encoded secret key. Required for `run`, `inject`, and `keygen --force`. |
| `COPILOT_BLOCK_ENGINE` | Yes | `https://mainnet.block-engine.jito.wtf/api/v1` | Jito Block Engine base URL. The public mainnet endpoint is the default. Change only to target a specific regional endpoint. |
| `COPILOT_JITO_UUID` | Yes | -- | Jito auth UUID. Sent as the `x-jito-auth` HTTP header on every bundle submission. Without it, the Block Engine accepts the submission and returns a bundle ID, but silently marks the bundle Invalid. Every bundle will fail to land. To obtain a UUID: open a support ticket in the Jito Discord, select the "Block Engine Rate Limit or Shredstream" category, then "New JSON-RPC UUID User", and follow the instructions. |
| `ANTHROPIC_API_KEY` | Yes | -- | Anthropic API key for the autonomous retry agent. Required for `copilot inject`. Obtain at console.anthropic.com under API Keys. The Claude Code OAuth session is not shared with the binary; a dedicated key is required. |
| `COPILOT_MODEL` | No | `claude-sonnet-4-6` | Claude model ID used by the retry agent. Override to target a different model. |
| `COPILOT_HELIUS_API_KEY` | No | -- | Helius API key. Used for `simulateBundle` diagnostics. The `sendBundle` path requires a Helius business plan. |
| `COPILOT_LOG` | No | `info` | Tracing filter string, passed to `tracing_subscriber`'s `EnvFilter`. Accepts standard filter syntax (`info`, `debug`, `geyser=trace`, etc.). |

## Generating a Keypair

```bash
copilot keygen
```

By default this writes a new keypair to `copilot-keypair.json` in the current directory. To specify a path:

```bash
copilot keygen --outfile ~/.copilot/keypair.json
```

Flags:

| Flag | Short | Default | Description |
|---|---|---|---|
| `--outfile` | `-o` | `copilot-keypair.json` | Destination file path. Parent directories are created if they do not exist. |
| `--force` | `-f` | false | Overwrite an existing file. Without this flag, the command exits with an error if the file already exists. |

The command generates a fresh ed25519 keypair, writes it as a JSON array of 64 bytes, and sets file permissions to `600`. It prints the public key to stdout and a funding reminder to stderr:

```
fund this address with ~0.1 SOL before running `copilot run`
```

After generating, set `COPILOT_KEYPAIR` in `~/.copilot/.env` to the output path before running any submission command.

## Command Reference

| Command | Description | Flags |
|---|---|---|
| `copilot watch` | Live chain feed: slots, leader, tip floor, congestion | none |
| `copilot run` | Submit tip-only bundles and track each to finality | `--count`, `--tip` |
| `copilot inject` | Fault injection and autonomous retry demo | none |
| `copilot logs` | Summarize all lifecycle log files in a directory | `--dir` |
| `copilot status` | Query Block Engine status for a bundle ID | `--bundle` |
| `copilot keygen` | Generate a new ed25519 keypair | `--outfile`, `--force` |

### copilot watch

```bash
copilot watch
```

No flags. Connects to the Geyser stream and prints one line per processed slot as it arrives:

```
slot 427360290 (confirmed 427360289, finalized 427360259) | leader AbCd...xYz | tip floor p50/p75/p95 = 1000/2000/5000 lamports (Low)
```

Field meanings:

- **slot**: the most recently processed slot number, incremented roughly twice per second.
- **confirmed**: the most recent slot that has received a supermajority vote. Trails processed by 1-2 slots under normal conditions.
- **finalized**: the most recent slot confirmed by Tower BFT. Trails processed by approximately 31 slots (~12.4 seconds at 400ms/slot).
- **leader**: the validator pubkey assigned to the current slot by the epoch leader schedule.
- **tip floor p50/p75/p95**: the 50th, 75th, and 95th percentile Jito tip amounts from recently landed bundles, in lamports. Refreshed every 10 seconds.
- **congestion level**: `Low`, `Moderate`, `High`, or `Severe`, derived from the ratio of p95 to p50 in the tip distribution (the tail ratio). A rising tail ratio indicates the auction is heating up and that bids near p50 are being undercut.

Press Ctrl-C to stop.

### copilot run

```bash
copilot run [--count <N>] [--tip <lamports>]
```

| Flag | Short | Default | Description |
|---|---|---|---|
| `--count` | `-c` | `5` | Number of bundles to submit, in sequence. |
| `--tip` | `-t` | oracle-priced | Tip amount in lamports. If omitted, the oracle computes a baseline tip from the live Jito tip floor and congestion level. The tip is always floored at the protocol minimum. |

For each submission in the sequence, `run`:

1. Logs the current leader and the next approaching Jito-connected leader window.
2. Queries the tip oracle and computes a baseline tip (or uses the pinned `--tip` value).
3. Fetches the latest blockhash at processed commitment.
4. Constructs a tip-only bundle: `[SetComputeUnitLimit, SetComputeUnitPrice, SystemTransfer to Jito tip account]`, signed by the payer.
5. Submits the bundle to the Block Engine at `COPILOT_BLOCK_ENGINE` with the `x-jito-auth` UUID header.
6. Waits for the signature to appear in the Geyser transaction stream at processed, confirmed, and finalized commitment.
7. Writes a `logs/lifecycle-run-NN.json` file with the full timing record.

Example output for a landed submission:

```
submission 3/5
  payer: 7xKX...qRst
Finalized — slot 427360290, submit→processed 1823ms, processed→confirmed 456ms, confirmed→finalized 13241ms
  logged → logs/lifecycle-run-27.json
```

Example output for a failed submission:

```
submission 2/5
  payer: 7xKX...qRst
WaitTimeout — never landed
  logged → logs/lifecycle-run-22.json
```

Example invocations:

```bash
copilot run                          # 5 submissions, oracle-priced tips
copilot run --count 1                # single submission
copilot run --count 10               # batch of 10
copilot run --count 3 --tip 100000   # 3 submissions at a pinned 100,000 lamport tip
```

### copilot inject

```bash
copilot inject
```

No flags. Requires `ANTHROPIC_API_KEY` and `COPILOT_KEYPAIR`. Runs the deterministic fault injection and autonomous retry demonstration:

1. Constructs a bundle with an intentionally corrupted blockhash, designed to appear approximately 166 slots old (16 slots past the 150-slot expiry window).
2. Submits the bundle and waits for the timeout.
3. Runs the fault classifier against the failure signals to produce a typed `FailureKind` with confidence and rationale.
4. Assembles a JSON context object containing the failure classification, live chain state, and current tip oracle data.
5. Calls the Claude model (configurable via `COPILOT_MODEL`) with the context.
6. Parses the model's structured JSON response: `action`, `new_tip_lamports`, `reasoning`, and `confidence`.
7. If the agent recommends `retry`, submits a fresh bundle with a valid blockhash and the agent's tip.
8. Tracks the retry bundle to finality.
9. Appends the full reasoning record to `logs/agent-reasoning.jsonl`.

### copilot logs

```bash
copilot logs [--dir <path>]
```

| Flag | Short | Default | Description |
|---|---|---|---|
| `--dir` | `-d` | `logs` | Directory to scan for lifecycle log files. |

Reads every file matching `lifecycle-run-*.json` in the directory, sorted by name, and prints a one-line summary for each, followed by a landing rate total:

```
lifecycle-run-16.json  Finalized — slot 427183295, submit→processed 1819ms, processed→confirmed 534ms, confirmed→finalized 13187ms
lifecycle-run-17.json  Finalized — slot 427185085, submit→processed 2064ms, processed→confirmed 423ms, confirmed→finalized 13502ms
lifecycle-run-18.json  WaitTimeout — never landed
...
56 runs -- 13 landed, 43 failed
```

### copilot status

```bash
copilot status --bundle <bundle-id>
```

| Flag | Short | Required | Description |
|---|---|---|---|
| `--bundle` | `-b` | Yes | The bundle ID returned by `sendBundle`, also present in the `bundle_id` field of a lifecycle log entry. |

Queries the Block Engine's `getBundleStatuses` endpoint and prints the raw status for the given bundle ID. Useful for post-hoc diagnosis of a specific submission. Note that bundle status is ephemeral -- the Block Engine retains records for a limited window after submission.

### copilot keygen

Covered in full in [Generating a Keypair](#generating-a-keypair).

## First-Run Walkthrough

This walkthrough takes a new user from zero to a confirmed onchain landing. Follow each step in order.

**Step 1: Install**

```bash
curl -fsSL https://copilot.asklemma.xyz/install.sh | bash
```

Walk through the interactive wizard. When prompted for the Jito UUID, paste your UUID if you have one; press Enter to skip and set it after. When prompted for the Anthropic API key, paste your key or press Enter to skip. Note the public key printed if you chose to generate a new keypair -- you will need it in the next step.

**Step 2: Fund the keypair**

Send at least 0.05 SOL to the address printed during keygen. A 5-bundle run with tips near p50 costs roughly 0.0005 SOL (500,000 lamports) in tips plus transaction fees. The full 27-run test session, including repeated unlanded attempts and the fault injection demo, consumed less than 0.1 SOL total.

Check your balance at any point with any Solana explorer or the Solana CLI:

```bash
solana balance <your-pubkey>
```

**Step 3: Verify the live feed**

Tell Claude Code: "Start the chain feed and let me know when you see slots advancing." Healthy output has slot numbers climbing at roughly two per second, confirmed trailing processed by 1–2 slots, and finalized trailing by about 31. If the numbers are stalled or not advancing, ask Claude Code to check `COPILOT_GRPC_URL` in `~/.copilot/.env` and confirm the gRPC endpoint is reachable.

**Step 4: Single submission**

Tell Claude Code: "Submit one bundle and track it to finality." Expected outcome within 15–20 seconds: a `Finalized` line with a slot number and latency deltas. If it says `WaitTimeout — never landed`, ask Claude Code to check that `COPILOT_JITO_UUID` is set in `~/.copilot/.env` and matches the UUID Jito issued you (see Section 10.7).

**Step 5: Batch run**

Tell Claude Code: "Submit five bundles sequentially and report the landing rate." Each submission is priced from the live oracle and tracked independently through processed, confirmed, and finalized. Landing rate is near 100% when a Jito-connected leader is active and the oracle-priced tip is competitive.

**Step 6: Fault injection demo**

Tell Claude Code: "Run the fault injection demo and walk me through what the agent decided." Requires `ANTHROPIC_API_KEY` to be set. Claude Code will submit a bundle with a deliberately expired blockhash (the failure is injected deterministically, not left to chance), classify the failure, send the full live context to the Claude agent, execute the agent's retry decision, and read back the structured reasoning from `logs/agent-reasoning.jsonl` once the session completes.

**Step 7: Read the log summary**

Tell Claude Code: "Summarize the run logs and tell me the overall landing rate." Claude Code will scan `logs/success/` and `logs/failures/`, report how many runs landed, and show per-run latency for the confirmed landings. To inspect a specific run, say: "Read the lifecycle log for run 27 from logs/success/ and show me the timing breakdown." To review the agent's reasoning history, say: "Read logs/agent-reasoning.jsonl and explain each decision."

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| Bundles consistently time out; Block Engine returns a bundle ID but nothing lands | `COPILOT_JITO_UUID` is missing or incorrect | Obtain a UUID via the Jito Discord support system: open a ticket in the Block Engine channel, select "Block Engine Rate Limit or Shredstream", then "New JSON-RPC UUID User", and follow the instructions. Set the UUID in `~/.copilot/.env`. The Block Engine silently marks bundles Invalid when the UUID is absent or wrong -- the HTTP response is 200 regardless. |
| `copilot inject` fails with a 401 error or "unauthorized" from the Anthropic API | `ANTHROPIC_API_KEY` is not set or is invalid | Set `ANTHROPIC_API_KEY` in `~/.copilot/.env`. The value must be a live API key from console.anthropic.com under API Keys. The Claude Code OAuth session used by the `claude` CLI is not shared with the binary and is not a substitute. |
| Log output shows "Geyser stream lost; reconnecting" | The gRPC provider closed the stream (normal server-side idle timeout) | No action needed. The reconnect loop handles clean closes and gRPC errors transparently with exponential backoff starting at 500ms and capping at 30s. The stream resumes automatically and all shared state (slot counters, blockhash, tracked signature) is preserved across reconnects. |
| "COPILOT_RPC_URL is not set" or "COPILOT_GRPC_URL is not set" at startup | The `.env` file is missing or the variable is blank | Confirm `~/.copilot/.env` exists and contains the variable: `cat ~/.copilot/.env`. If missing, re-run the install script or create the file manually using the template in the repo root (`.env.example`). |
| gRPC connection error or "transport error" on startup | The gRPC auth token is embedded in `COPILOT_GRPC_URL` instead of being separate | The URL and the auth token are separate variables. `COPILOT_GRPC_URL` should be `host:port` (e.g. `fra.grpc.solinfra.dev:443`). The provider auth token belongs in `COPILOT_GRPC_X_TOKEN`. The binary sends the token as the `x-token` HTTP/2 header. Embedding the token in the URL string will produce a malformed endpoint that the gRPC transport cannot resolve. |
| `copilot: command not found` after installation | `~/.cargo/bin` is not on `PATH` | Add `export PATH="$HOME/.cargo/bin:$PATH"` to `~/.bashrc` or `~/.zshrc` and open a new shell, or run `source ~/.bashrc`. The install script temporarily adds `.cargo/bin` to `PATH` for its own session, but this does not persist. |
| Build fails on macOS with an LLVM linker error about attribute kind 102 | Rust 1.96 (LLVM 22) bitcode incompatible with the Apple system linker | This is handled automatically by `.cargo/config.toml` in the repo (`-C embed-bitcode=no` for both macOS targets). If the error appears, confirm you are running `cargo build` from within the cloned repo directory so that `.cargo/config.toml` is on the search path. |
| `cargo clippy -p geyser` reports `E0204` or `E0277` errors | Feature unification: other workspace crates enable features on `solana_hash` that provide `Copy` and `FromStr`, but they are absent when compiling the crate in isolation | Run `cargo clippy --workspace -- -D warnings` instead of targeting a single crate. The workspace compilation enables the necessary features and all crates pass with zero warnings. |
