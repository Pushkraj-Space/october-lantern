#!/usr/bin/env bash
# Type-checks the Windows code from Linux or macOS, without a Windows C toolchain:
# `cargo clippy --target x86_64-pc-windows-gnu -D warnings` for the engine and the desktop app.
#
# Nothing is linked, so the C that a few dependencies compile in their build scripts isn't
# needed: a stand-in compiler pretends to compile it and a stand-in archiver writes empty
# archives. This proves the Windows Rust code compiles cleanly; it doesn't run it. CI's Windows
# job (and a real Windows machine) still have the last word.
#
# Needs: rustup target add x86_64-pc-windows-gnu
set -euo pipefail
cd "$(dirname "$0")/.."
target=x86_64-pc-windows-gnu
rustup target list --installed | grep -qx "$target" || { echo "rustup target add $target" >&2; exit 1; }

stubs=$(mktemp -d)
trap 'rm -rf "$stubs"' EXIT
printf '#!/bin/sh\nexit 0\n' > "$stubs/cc"
printf '#!/bin/sh\nfor a in "$@"; do case "$a" in *.a) : > "$a";; esac; done\nexit 0\n' > "$stubs/ar"
chmod +x "$stubs/cc" "$stubs/ar"
export CC_x86_64_pc_windows_gnu="$stubs/cc" AR_x86_64_pc_windows_gnu="$stubs/ar"

for crate in engine desktop; do
  echo "== $crate ($target)"
  (cd "$crate" && CARGO_TARGET_DIR=target/windows-types cargo clippy --quiet --target "$target" --all-targets -- -D warnings)
done
echo "Windows types: ok"
