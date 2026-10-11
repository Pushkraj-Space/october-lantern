#!/usr/bin/env bash
# Install the local CPU speech runtime and multilingual base model, with no root access.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMMIT=d1be6fde11ac6e0407606b4e42fe72d34add8037
SOURCE="$ROOT/build/whisper-src"
SPEECH="${XDG_DATA_HOME:-$HOME/.local/share}/october-lantern/speech"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/october-lantern/settings.json"
SUM=60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe
command -v c++ >/dev/null || { echo "Install a C++ compiler first." >&2; exit 1; }
if command -v cmake >/dev/null; then
  CMAKE="$(command -v cmake)"
else
  command -v uv >/dev/null || { echo "Install CMake, or uv for a local CMake installation." >&2; exit 1; }
  uv pip install --target "$ROOT/build/speech-tools" 'cmake==4.4.4'
  CMAKE="$ROOT/build/speech-tools/cmake/data/bin/cmake"
fi
[[ -d "$SOURCE/.git" ]] || git clone --quiet --depth 1 --branch v1.9.5 https://github.com/ggml-org/whisper.cpp.git "$SOURCE"
[[ "$(git -C "$SOURCE" rev-parse HEAD)" == "$COMMIT" ]] || { echo "Unexpected whisper.cpp source revision." >&2; exit 1; }
"$CMAKE" -S "$SOURCE" -B "$ROOT/build/whisper-build" -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DWHISPER_BUILD_TESTS=OFF -DWHISPER_CURL=OFF -DGGML_NATIVE=OFF
"$CMAKE" --build "$ROOT/build/whisper-build" --target whisper-cli -j "${BUILD_JOBS:-2}"
mkdir -p "$SPEECH"
chmod 700 "$SPEECH"
install -m 755 "$ROOT/build/whisper-build/bin/whisper-cli" "$SPEECH/.whisper-cli.new"
mv -f "$SPEECH/.whisper-cli.new" "$SPEECH/whisper-cli"
MODEL="$SPEECH/ggml-base.bin"
if [[ ! -f "$MODEL" ]] || ! printf '%s  %s\n' "$SUM" "$MODEL" | sha256sum --check --status; then
  PART="$(mktemp "$SPEECH/.model.XXXXXX")"
  trap 'rm -f "$PART"' EXIT
  curl --fail --location --retry 3 --output "$PART" https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
  printf '%s  %s\n' "$SUM" "$PART" | sha256sum --check
  mv -f "$PART" "$MODEL"
  trap - EXIT
fi
cp "$SOURCE/LICENSE" "$SPEECH/WHISPER-LICENSE"
python3 - "$CONFIG" "$SPEECH/whisper-cli" "$MODEL" <<'PY'
import json, os, pathlib, sys, tempfile
path = pathlib.Path(sys.argv[1]); path.parent.mkdir(parents=True, exist_ok=True)
settings = json.loads(path.read_text()) if path.exists() else {}
settings.update(whisper=sys.argv[2], whisper_model=sys.argv[3], speech_language='auto')
fd, temporary = tempfile.mkstemp(prefix='.dictation-', dir=path.parent)
try:
    with os.fdopen(fd, 'w') as stream:
        json.dump(settings, stream, indent=2); stream.flush(); os.fsync(stream.fileno())
    os.replace(temporary, path)
finally:
    if os.path.exists(temporary): os.unlink(temporary)
PY
"$SPEECH/whisper-cli" -m "$MODEL" -f "$SOURCE/samples/jfk.wav" -l en -nt -np
printf 'Local dictation installed. Reopen Lantern to load the speech settings.\n'
