#!/usr/bin/env bash
# Bundle the native Linux Bus without changing the macOS download manifest.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMMIT="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["commit"])' "$ROOT/engine/bus.json")"
SOURCE="${OCTOBER_BUS_SRC:-$ROOT/build/october-bus-src}"
command -v go >/dev/null || { echo "Go 1.27+ is required to bundle October Bus." >&2; exit 1; }
[[ -d "$SOURCE/.git" ]] || git clone --quiet https://github.com/october-dev/october-bus.git "$SOURCE"
git -C "$SOURCE" cat-file -e "$COMMIT^{commit}" 2>/dev/null || git -C "$SOURCE" fetch --quiet origin
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
git -C "$SOURCE" archive "$COMMIT" | tar -x -C "$WORK"
mkdir -p "$ROOT/build/linux"
(cd "$WORK" && CGO_ENABLED=0 GOOS=linux go build -trimpath -ldflags '-s -w' -o october-bus ./cmd/october-bus)
install -m 755 "$WORK/october-bus" "$ROOT/build/linux/.october-bus.new"
mv -f "$ROOT/build/linux/.october-bus.new" "$ROOT/build/linux/october-bus"
cp "$WORK/LICENSE" "$ROOT/build/linux/OCTOBER-BUS-LICENSE"
printf '%s\n' "$COMMIT" > "$ROOT/build/linux/OCTOBER-BUS-COMMIT"
"$ROOT/build/linux/october-bus" --help >/dev/null
echo "Bundled native October Bus ($COMMIT)."
