#!/usr/bin/env bash

set -euo pipefail

PRODUCT_NAME="copilot"
GITHUB_REPO="replicolabs/copilot"
GITHUB_URL="https://github.com/${GITHUB_REPO}.git"
REPO_DIR="${COPILOT_HOME:-$HOME/.copilot}/src"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; DIM='\033[2m'; RESET='\033[0m'

log()  { printf "\n  ${GREEN}▸${RESET} %s\n" "$1"; }
warn() { printf "  ${YELLOW}!${RESET} %s\n" "$1"; }
fail() { printf "\n  ${RED}✗${RESET} %s\n\n" "$1" >&2; exit 1; }
ok()   { printf "  ${GREEN}✓${RESET} %s\n" "$1"; }
has_cmd() { command -v "$1" >/dev/null 2>&1; }

printf "\n"
printf "  ${CYAN}${BOLD}%s${RESET}\n" '   ___   ___   ___  ___ _      ___ _____'
printf "  ${CYAN}${BOLD}%s${RESET}\n" '  / __| / _ \ | _ \|_ _| |    / _ \_   _|'
printf "  ${CYAN}${BOLD}%s${RESET}\n" ' | (__ | (_) ||  _/ | || |__ | (_) || |'
printf "  ${CYAN}${BOLD}%s${RESET}\n" '  \___| \___/ |_|  |___|____| \___/ |_|'
printf "  ${DIM}%s${RESET}\n\n" 'Smart transaction infrastructure for Solana'

log "Checking prerequisites..."

has_cmd git  || fail "git is required."
has_cmd curl || has_cmd wget || fail "curl or wget is required."
ok "git $(git --version | awk '{print $3}')"

if has_cmd claude; then
  ok "Claude Code found"
else
  if has_cmd npm; then
    log "Installing Claude Code..."
    if npm install -g @anthropic-ai/claude-code >/dev/null 2>&1; then
      ok "Claude Code installed"
    else
      warn "Could not install Claude Code. Install later: npm i -g @anthropic-ai/claude-code"
    fi
  else
    warn "Claude Code not found and npm unavailable. Install: npm i -g @anthropic-ai/claude-code"
  fi
fi

HAVE_CARGO=false
if has_cmd cargo; then HAVE_CARGO=true; ok "cargo $(cargo --version | awk '{print $2}')"; else
  warn "Rust/cargo not found — skills will install, but the binary won't build."
  warn "Install Rust (https://rustup.rs), then re-run, or: cargo install --path crates/cli"
fi

log "Fetching ${PRODUCT_NAME}..."
mkdir -p "$(dirname "$REPO_DIR")"
if [ -d "$REPO_DIR/.git" ]; then
  git -C "$REPO_DIR" pull --ff-only --quiet && ok "Updated $REPO_DIR"
else
  git clone --depth 1 --quiet "$GITHUB_URL" "$REPO_DIR" && ok "Cloned to $REPO_DIR"
fi

log "Installing skills..."
SKILL_TARGETS="$HOME/.claude/skills"
has_cmd codex && SKILL_TARGETS="$SKILL_TARGETS $HOME/.codex/skills"

SKILL_COUNT=0
for target in $SKILL_TARGETS; do
  mkdir -p "$target"
  for skill_dir in "$REPO_DIR"/skills/*/; do
    [ -f "$skill_dir/SKILL.md" ] || continue
    name=$(basename "$skill_dir")
    rm -rf "${target:?}/$name"
    cp -Rf "$skill_dir" "$target/$name"
    SKILL_COUNT=$((SKILL_COUNT + 1))
  done
  [ -f "$REPO_DIR/skills/SKILL_ROUTER.md" ] && cp -f "$REPO_DIR/skills/SKILL_ROUTER.md" "$target/SKILL_ROUTER.md"
  [ -f "$REPO_DIR/skills/README.md" ] && cp -f "$REPO_DIR/skills/README.md" "$target/README.md"
  ok "Installed skills to $target"
done

CLAUDE_SETTINGS="$HOME/.claude/settings.json"
if [ ! -f "$CLAUDE_SETTINGS" ]; then
  mkdir -p "$HOME/.claude"
  printf '%s\n' '{"permissions":{"allow":["Bash","Read","Glob","Grep"]}}' > "$CLAUDE_SETTINGS"
  ok "Configured Claude Code permissions"
fi

if [ "$HAVE_CARGO" = true ]; then
  log "Building the copilot binary (this can take a few minutes)..."
  if cargo install --path "$REPO_DIR/crates/cli" --quiet 2>/dev/null; then
    ok "Installed $(copilot --version 2>/dev/null || echo copilot) to ~/.cargo/bin"
  else
    warn "Build failed. Try manually: cd $REPO_DIR && cargo install --path crates/cli"
  fi
fi

export PATH="$HOME/.cargo/bin:$PATH"

CONFIG_DIR="${COPILOT_HOME:-$HOME/.copilot}"
ENV_FILE="$CONFIG_DIR/.env"
KEYPAIR_DEFAULT="$CONFIG_DIR/keypair.json"
EDIT_HINT="${EDITOR:-nano} $ENV_FILE"
mkdir -p "$CONFIG_DIR"

write_env() {
  cat > "$ENV_FILE" <<ENVEOF
# Copilot configuration — loaded automatically by the copilot binary.
# Edit anytime: ${EDITOR:-nano} $ENV_FILE

# Required: JSON-RPC endpoint (tip oracle + leader schedule).
COPILOT_RPC_URL=$1
# Required: Yellowstone gRPC endpoint (live feed + lifecycle tracking).
COPILOT_GRPC_URL=$2
# Optional: x-token, if your gRPC provider requires one.
COPILOT_GRPC_X_TOKEN=$3
# Payer keypair: a path to a keypair file, or an inline base58 secret.
COPILOT_KEYPAIR=$4

# Public defaults — usually no need to change these.
COPILOT_BLOCK_ENGINE=https://mainnet.block-engine.jito.wtf/api/v1
# COPILOT_MODEL=claude-sonnet-4-6
# COPILOT_LOG=info
ENVEOF
  chmod 600 "$ENV_FILE" 2>/dev/null || true
}

ask() {
  local _ans=""
  printf "    %s " "$1" > /dev/tty
  IFS= read -r _ans < /dev/tty || _ans=""
  printf '%s' "$_ans"
}

RECONFIGURE=true
if [ -f "$ENV_FILE" ] && [ -r /dev/tty ]; then
  printf "\n  ${YELLOW}!${RESET} %s already exists. Reconfigure? [y/N] " "$ENV_FILE" > /dev/tty
  _reconf=""
  IFS= read -r _reconf < /dev/tty || _reconf=""
  case "$_reconf" in [yY]*) RECONFIGURE=true ;; *) RECONFIGURE=false ;; esac
fi

if [ "$RECONFIGURE" = true ] && [ -r /dev/tty ]; then
  printf "\n  ${BOLD}Let's set up Copilot.${RESET} Press Enter to skip any value and set it later.\n\n" > /dev/tty
  MISSING=""

  RPC=$(ask "RPC URL           (COPILOT_RPC_URL):")
  if [ -z "$RPC" ]; then MISSING="$MISSING COPILOT_RPC_URL"; fi

  GRPC=$(ask "Yellowstone gRPC  (COPILOT_GRPC_URL):")
  if [ -z "$GRPC" ]; then MISSING="$MISSING COPILOT_GRPC_URL"; fi

  XTOKEN=$(ask "gRPC x-token      (optional, Enter to skip):")

  KEYPAIR=""
  printf "\n    Keypair:  [1] create a new one   [2] use an existing one\n" > /dev/tty
  KP_CHOICE=$(ask "Choose [1/2] (Enter to skip):")
  case "$KP_CHOICE" in
    1)
      if has_cmd copilot; then
        NEWPUB=$(copilot keygen --outfile "$KEYPAIR_DEFAULT" --force 2>/dev/null) || NEWPUB=""
        if [ -n "$NEWPUB" ]; then
          KEYPAIR="$KEYPAIR_DEFAULT"
          chmod 600 "$KEYPAIR_DEFAULT" 2>/dev/null || true
          printf "    ${GREEN}✓${RESET} created %s\n" "$KEYPAIR_DEFAULT" > /dev/tty
          printf "    ${YELLOW}!${RESET} Fund this address with ~0.1 SOL: ${BOLD}%s${RESET}\n" "$NEWPUB" > /dev/tty
        else
          warn "keygen failed — set COPILOT_KEYPAIR later"
        fi
      else
        warn "copilot binary not on PATH yet — create a keypair later, then set COPILOT_KEYPAIR"
      fi
      ;;
    2)
      KEYPAIR=$(ask "Path to keypair file (or paste a base58 secret):")
      ;;
    *)
      : 
      ;;
  esac
  if [ -z "$KEYPAIR" ]; then MISSING="$MISSING COPILOT_KEYPAIR"; fi

  write_env "$RPC" "$GRPC" "$XTOKEN" "$KEYPAIR"
  ok "Saved configuration to $ENV_FILE"
  if [ -n "$MISSING" ]; then
    printf "    ${YELLOW}!${RESET} Left blank:%s\n" "$MISSING" > /dev/tty
    printf "      Set them later with: ${BOLD}%s${RESET}\n" "$EDIT_HINT" > /dev/tty
  fi
elif [ "$RECONFIGURE" = true ]; then
  if [ ! -f "$ENV_FILE" ]; then
    write_env "" "" "" ""
    ok "Wrote a config template to $ENV_FILE"
  fi
  warn "No terminal detected — edit $ENV_FILE to set your endpoints and keypair"
fi

printf "\n  ${BOLD}Installed %s skills.${RESET} Next:\n\n" "$SKILL_COUNT"
printf "    ${CYAN}1.${RESET} Make sure your keypair holds SOL and endpoints are set: ${BOLD}%s${RESET}\n" "$EDIT_HINT"
printf "    ${CYAN}2.${RESET} ${BOLD}copilot watch${RESET}    ${DIM}# live slot / leader / tip feed${RESET}\n"
printf "    ${CYAN}3.${RESET} ${BOLD}copilot run${RESET}      ${DIM}# land tip-only bundles, track lifecycle${RESET}\n"