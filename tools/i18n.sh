#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MODE="${1:-all}"
AUTH_MODE="${AUTH_MODE:-auto}"
LOCALE="${LOCALE:-en}"
EN_PATH="${EN_PATH:-i18n/en.json}"
BATCH_SIZE="${BATCH_SIZE:-500}"
TRANSLATOR_BIN="${TRANSLATOR_BIN:-greentic-i18n-translator}"
BINSTALL_CMD="${BINSTALL_CMD:-cargo-binstall}"

usage() {
  cat <<'EOF'
Usage: tools/i18n.sh [translate|validate|status|all]

Environment overrides:
  EN_PATH=...                     English source file path (default: i18n/en.json)
  AUTH_MODE=...                   Translator auth mode for translate (default: auto)
  LOCALE=...                      CLI locale used for translator output (default: en)
  BATCH_SIZE=...                  Keys per translation request (default: 500)
  TRANSLATOR_BIN=...              Translator binary name or path (default: greentic-i18n-translator)
  BINSTALL_CMD=...                cargo-binstall command name or path (default: cargo-binstall)

Examples:
  tools/i18n.sh all
  AUTH_MODE=api-key tools/i18n.sh translate
  EN_PATH=i18n/en.json tools/i18n.sh validate
EOF
}

ensure_translator() {
  if command -v "$TRANSLATOR_BIN" >/dev/null 2>&1; then
    return 0
  fi

  if ! command -v "$BINSTALL_CMD" >/dev/null 2>&1; then
    echo "error: $TRANSLATOR_BIN is not installed and $BINSTALL_CMD was not found on PATH" >&2
    echo "install cargo-binstall or set TRANSLATOR_BIN to an existing greentic-i18n-translator binary" >&2
    exit 1
  fi

  echo "$TRANSLATOR_BIN not found; installing via $BINSTALL_CMD" >&2
  "$BINSTALL_CMD" -y greentic-i18n-translator

  if ! command -v "$TRANSLATOR_BIN" >/dev/null 2>&1; then
    echo "error: $TRANSLATOR_BIN is still not available after installation" >&2
    exit 1
  fi
}

run_translate() {
  ensure_translator
  "$TRANSLATOR_BIN" \
    --locale "$LOCALE" \
    translate --langs all --en "$EN_PATH" --auth-mode "$AUTH_MODE" --batch-size "$BATCH_SIZE"
}

run_validate() {
  python3 ci/i18n_check.py validate
}

run_status() {
  python3 ci/i18n_check.py status
}

if [[ "${MODE}" == "-h" || "${MODE}" == "--help" ]]; then
  usage
  exit 0
fi

case "$MODE" in
  translate) run_translate ;;
  validate) run_validate ;;
  status) run_status ;;
  all)
    run_translate
    run_validate
    run_status
    ;;
  *)
    echo "Unknown mode: $MODE" >&2
    usage
    exit 2
    ;;
esac
