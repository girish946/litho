#!/usr/bin/env bash
# Record or rebuild litho-tui demo assets (P11).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BINARY="${BINARY:-./target/release/litho-tui}"

usage() {
    cat <<'EOF'
Usage: scripts/record-demo.sh [record|gif]

  record  Run asciinema rec and save demo.cast (interactive)
  gif     Build demo.gif from existing demo.cast (requires agg)

Environment:
  BINARY  Path to litho-tui (default: ./target/release/litho-tui)

Examples:
  cargo build --release --bin litho-tui
  scripts/record-demo.sh record
  scripts/record-demo.sh gif
EOF
}

cmd="${1:-}"

case "$cmd" in
  record)
    if ! command -v asciinema >/dev/null; then
      echo "asciinema is required: https://asciinema.org/" >&2
      exit 1
    fi
    if [[ ! -x "$BINARY" ]]; then
      echo "Build litho-tui first: cargo build --release --bin litho-tui" >&2
      exit 1
    fi
    echo "Recording demo to demo.cast — quit litho-tui with q when done."
    asciinema rec -c "$BINARY" demo.cast
    ;;
  gif)
    if ! command -v agg >/dev/null; then
      echo "agg is required: https://github.com/asciinema/agg" >&2
      exit 1
    fi
    if [[ ! -f demo.cast ]]; then
      echo "demo.cast not found — run: scripts/record-demo.sh record" >&2
      exit 1
    fi
    agg demo.cast demo.gif
    echo "Wrote demo.gif"
    ;;
  *)
    usage
    exit 1
    ;;
esac