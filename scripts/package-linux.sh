#!/usr/bin/env bash
# Creates the exact native bundle consumed by the Linux updater. Does not publish it.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/build/linux"
ARCH="$(uname -m)"
[[ "$ARCH" != arm64 ]] || ARCH=aarch64
FILES=(october-lantern lantern-engine october-bus october-lantern.png LICENSE OCTOBER-BUS-LICENSE OCTOBER-BUS-COMMIT lantern-gnome-extension.zip)
for file in "${FILES[@]}"; do
  [[ -f "$DEST/$file" ]] || { echo "Missing $file; run build-app.sh and build-bus-linux.sh first." >&2; exit 1; }
done
OUT="$ROOT/build/October-Lantern-linux-$ARCH.tar.gz"
tar -czf "$OUT" -C "$DEST" "${FILES[@]}"
(cd "$ROOT/build" && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256")
echo "$OUT"
