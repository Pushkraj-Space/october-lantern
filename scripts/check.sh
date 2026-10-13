#!/usr/bin/env bash
# The gate every change must pass: engine format, lints and tests; the app's build and tests; and
# shell syntax. release.sh runs it before building, and CI runs it on every push.
#   scripts/check.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Rust from Homebrew's rustup or a plain rustup install, when cargo isn't already on PATH.
if ! command -v cargo >/dev/null; then
  for dir in /opt/homebrew/opt/rustup/bin "$HOME/.cargo/bin" "$HOME"/.rustup/toolchains/stable-*/bin; do
    if [[ -x "$dir/cargo" ]]; then export PATH="$dir:$PATH"; break; fi
  done
fi

if [[ "$(uname -s)" == Linux ]]; then
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/build/target}"
fi

echo "==> version"
# One version for every platform: the engine's, which the Mac app and the releases use too.
ENGINE_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/engine/Cargo.toml" | head -1)"
DESKTOP_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/desktop/Cargo.toml" | head -1)"
[[ "$ENGINE_VERSION" == "$DESKTOP_VERSION" ]] || {
  echo "desktop/Cargo.toml is $DESKTOP_VERSION but engine/Cargo.toml is $ENGINE_VERSION; give both the same version" >&2
  exit 1
}

echo "==> engine"
(cd "$ROOT/engine" && cargo fmt --all -- --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked)

case "$(uname -s)" in
  Darwin)
    echo "==> macOS app"
    (cd "$ROOT/macos" && swift build)
    if grep -q testTarget "$ROOT/macos/Package.swift"; then
      (cd "$ROOT/macos" && swift test)
    fi
    ;;
  Linux)
    echo "==> Rust desktop"
    (cd "$ROOT/desktop" && cargo fmt --all -- --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked)
    ;;
  *) echo "Unsupported host OS" >&2; exit 1 ;;
esac

echo "==> scripts"
for s in "$ROOT"/scripts/*.sh; do bash -n "$s"; done
if [[ "$(uname -s)" == Linux ]] && command -v node >/dev/null; then
  node --check "$ROOT/linux/gnome-extension/extension.js"
fi
echo "==> checks passed"
