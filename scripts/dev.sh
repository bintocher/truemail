#!/usr/bin/env sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"

if [ -f .env ]; then
  set -a
  # .env is developer-owned and expected to contain simple KEY=VALUE lines.
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

command -v cargo >/dev/null
command -v perl >/dev/null

if [ "${1:-}" = "--check" ]; then
  echo "truemail dev environment: OK"
  exit 0
fi

if ! command -v cargo-sweep >/dev/null; then
  cargo install cargo-sweep --version 0.8.0 --locked
fi

cleanup() {
  cd "$ROOT"
  cargo sweep --time 30 .
}
trap cleanup EXIT INT TERM

cd apps/desktop/src-tauri
cargo tauri dev
