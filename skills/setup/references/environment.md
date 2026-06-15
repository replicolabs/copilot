# Environment and configuration reference

Every variable Copilot reads, the accepted formats, and the network caveats. Use
this when filling `.env`, debugging a config error, or explaining why a value
matters.

---

## All variables

Copilot reads configuration from the environment (the installer scaffolds `.env`
from `.env.example`, and your shell loads it). Nothing is prompted interactively.

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `COPILOT_RPC_URL` | yes | — | JSON-RPC endpoint: `getRecentPrioritizationFees`, `getLeaderSchedule`, `getEpochInfo` |
| `COPILOT_GRPC_URL` | yes | — | Yellowstone gRPC endpoint: slots/blocks/blockhash feed + per-signature lifecycle tracking |
| `COPILOT_GRPC_X_TOKEN` | no | empty | auth token for gRPC endpoints that require one; leave blank if not needed |
| `COPILOT_KEYPAIR` | for `run`/`inject` | — | payer/signer (file path or inline base58) |
| `COPILOT_BLOCK_ENGINE` | no | Jito mainnet engine | Block Engine base URL (regional mirrors below) |
| `COPILOT_MODEL` | no | `claude-sonnet-4-6` | model the retry agent uses |
| `ANTHROPIC_API_KEY` | no | inherited | the agent's credential — normally inherited from your Claude Code session; only set if running outside an authenticated session |
| `COPILOT_LOG` | no | `info` | tracing verbosity: `error` \| `warn` \| `info` \| `debug` \| `trace` |

`watch` needs only the two endpoints (and RPC for the tip floor). `run` and
`inject` additionally need `COPILOT_KEYPAIR` and, for real landings, mainnet.

---

## Keypair formats

`COPILOT_KEYPAIR` accepts either:

1. **A path to a Solana CLI keypair file** — the JSON array of 64 bytes that
   `solana-keygen` writes (the secret key + public key). Example value:
   `/home/you/.config/solana/id.json` or a dedicated `payer-keypair.json`.
2. **An inline base58 secret key** — the base58-encoded 64-byte secret, set
   directly as the variable's value.

Guidance:
- Prefer a **file kept outside the repo**. `.gitignore` excludes common keypair
  filenames and `.env`, but the safest posture is a keypair path that never sits
  in the project tree.
- Fund it **lightly** (~0.1 SOL). Copilot's default bundles are tip-only: each
  costs a tip (≥1,000 lamports, typically the landed p75) plus the 5,000-lamport
  base fee. 0.1 SOL covers many runs.
- An invalid keypair surfaces as a load error on `run`/`inject` — see
  `connectivity.md`.

Never commit a keypair or paste a secret into a log, a chat, or a screenshot.

---

## Networks: mainnet vs. devnet

- **Mainnet** is required for **real Jito landings**. Bundles only land on
  Jito-connected leader slots, and that infrastructure is a mainnet reality
  (`../../solana-internals/references/jito-bundles.md`).
- **Devnet** is fine for wiring up and exercising the pipeline (feed connects,
  blockhashes flow, the tracker runs), but don't expect the bundle auction to
  behave like mainnet. If a user reports "nothing lands" on devnet, that's
  expected — confirm the network before diagnosing further.

Set the network implicitly via your endpoints (point `COPILOT_RPC_URL` /
`COPILOT_GRPC_URL` / `COPILOT_BLOCK_ENGINE` at mainnet or devnet infrastructure
consistently — don't mix).

---

## Block Engine regional endpoints

`COPILOT_BLOCK_ENGINE` defaults to the mainnet engine. Jito runs regional
mirrors; picking the one nearest your sender reduces latency to the auction:

```
https://mainnet.block-engine.jito.wtf/api/v1            (default)
https://amsterdam.mainnet.block-engine.jito.wtf/api/v1
https://frankfurt.mainnet.block-engine.jito.wtf/api/v1
https://ny.mainnet.block-engine.jito.wtf/api/v1
https://tokyo.mainnet.block-engine.jito.wtf/api/v1
```

(Use whichever region Jito currently documents nearest you; the path suffix
`/api/v1` is what Copilot expects.)

---

## The agent credential (no key prompt)

`ANTHROPIC_API_KEY` is deliberately **not** something the installer asks for. The
agent inherits the credential from your authenticated **Claude Code** session, so
in normal use you leave it unset. Set it explicitly only when running Copilot
outside such a session (e.g. a bare server) and you need to supply a key. This is
the "no manual API key" design point — the agent is wired to the session you
already have.

`COPILOT_MODEL` overrides the model the agent uses; it defaults to a current
Sonnet. The agent boundary and how the model is called are in
`../../inject/references/agent-reasoning.md`.

---

## Logging

`COPILOT_LOG` sets tracing verbosity. `info` is a good default; use `debug` or
`trace` when diagnosing why the feed or tracker behaves unexpectedly (it surfaces
subscription and submission detail). Set it before launching `watch`/`run`/`inject`.