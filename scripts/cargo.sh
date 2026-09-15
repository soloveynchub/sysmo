#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ -x "$ROOT/.toolchain/cargo/bin/cargo" ]; then
  export CARGO_HOME="$ROOT/.toolchain/cargo"
  export RUSTUP_HOME="$ROOT/.toolchain/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
elif ! command -v cargo >/dev/null 2>&1; then
  echo 'Rust не найден. Выполните ./scripts/setup.sh' >&2
  exit 1
fi
cd "$ROOT/agent"
exec cargo "$@"
