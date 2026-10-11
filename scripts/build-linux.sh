#!/usr/bin/env bash
# Build the portable Rust desktop alongside its Rust engine sidecar.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
[[ "$(uname -s)" == Linux ]] || { echo "This script builds the Linux app." >&2; exit 1; }
PROFILE=release
FLAGS=(--release)
case "${1:-}" in
  --debug) PROFILE=debug; FLAGS=() ;;
  '') ;;
  *) echo "Usage: scripts/build-linux.sh [--debug]" >&2; exit 1 ;;
esac
if ! command -v cargo >/dev/null; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
command -v cargo >/dev/null || { echo "Install stable Rust from https://rustup.rs" >&2; exit 1; }
# Shared target directory avoids rebuilding dependencies used by both Rust executables.
export CARGO_TARGET_DIR="$(realpath -m "${CARGO_TARGET_DIR:-$ROOT/build/target}")"
(cd "$ROOT/engine" && cargo build --locked "${FLAGS[@]}")
(cd "$ROOT/desktop" && cargo build --locked "${FLAGS[@]}")
DEST="$ROOT/build/linux"
mkdir -p "$DEST"
# Replace by rename: Linux refuses writes to an executable that a running app uses.
for pair in lantern-engine:lantern-engine lantern-desktop:october-lantern; do
  source_bin="${pair%%:*}"
  dest_bin="${pair##*:}"
  temporary_bin="$(mktemp "$DEST/.$dest_bin.XXXXXX")"
  install -m 755 "$CARGO_TARGET_DIR/$PROFILE/$source_bin" "$temporary_bin"
  mv -f "$temporary_bin" "$DEST/$dest_bin"
done
cp "$ROOT/logo.png" "$DEST/october-lantern.png"
cp "$ROOT/LICENSE" "$DEST/LICENSE"
python3 - "$ROOT/linux/gnome-extension" "$DEST/lantern-gnome-extension.zip" <<'PY'
import pathlib, sys, zipfile
with zipfile.ZipFile(sys.argv[2], 'w', compression=zipfile.ZIP_DEFLATED) as archive:
    for path in sorted(pathlib.Path(sys.argv[1]).iterdir()):
        if path.is_file(): archive.write(path, path.name)
PY
cat > "$DEST/dev.october.lantern.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=October Lantern
Comment=Monitor and reply to your AI coding agents
Exec="$DEST/october-lantern"
Icon=$DEST/october-lantern.png
Terminal=false
Categories=Development;Utility;
StartupWMClass=dev.october.lantern
DESKTOP
"$DEST/october-lantern" --check-sidecar
echo "Run: \"$DEST/october-lantern\""
