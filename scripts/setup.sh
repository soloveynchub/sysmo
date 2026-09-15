#!/bin/sh
# Build from locked dependencies. No sudo, no shell-profile changes.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
[ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ] || { echo 'Нужен Mac с Apple Silicon, терминал без Rosetta.' >&2; exit 1; }
xcode-select -p >/dev/null 2>&1 || { echo 'Установите инструменты Apple: xcode-select --install. Затем повторите запуск.' >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo 'Установите Python 3, затем повторите запуск.' >&2; exit 1; }
command -v npm >/dev/null 2>&1 && command -v node >/dev/null 2>&1 || { echo 'Установите Node.js 22.12+ (https://nodejs.org), затем повторите запуск.' >&2; exit 1; }
node -e 'const [a,b]=process.versions.node.split(".").map(Number);if(!((a===20&&b>=19)||(a===22&&b>=12)||a>=24)){console.error("Нужен Node.js 20.19+, 22.12+ или 24+");process.exit(1)}'
if ! command -v cargo >/dev/null 2>&1 && [ ! -x "$ROOT/.toolchain/cargo/bin/cargo" ]; then
  echo 'Установка Rust stable в .toolchain из https://sh.rustup.rs (без изменения профиля shell)…'
  INSTALLER=$(mktemp -t sysmo-rustup)
  trap 'rm -f "$INSTALLER"' EXIT HUP INT TERM
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$INSTALLER"
  CARGO_HOME="$ROOT/.toolchain/cargo" RUSTUP_HOME="$ROOT/.toolchain/rustup" sh "$INSTALLER" -y --profile minimal --default-toolchain stable --no-modify-path
fi
(cd "$ROOT/frontend" && npm ci && npm run build)
"$ROOT/scripts/cargo.sh" test --locked
"$ROOT/scripts/cargo.sh" build --release --locked
printf '\nСборка готова. Запуск: python3 scripts/manage.py install\n'
