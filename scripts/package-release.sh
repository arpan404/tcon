#!/usr/bin/env sh
set -eu

if [ "$#" -lt 1 ]; then
  echo "Usage: $0 <version> [target ...]"
  echo "Example: $0 1.0.0 x86_64-apple-darwin aarch64-apple-darwin"
  exit 2
fi

VERSION="$1"
shift

if [ "$#" -eq 0 ]; then
  HOST_TARGET="$(rustc -vV | awk '/host:/ {print $2}')"
  TARGETS="$HOST_TARGET"
else
  TARGETS="$*"
fi

mkdir -p dist
CHECKSUMS="dist/checksums-${VERSION}.txt"
: > "$CHECKSUMS"

for TARGET in $TARGETS; do
  echo "Building target: $TARGET"
  rustup target add "$TARGET" >/dev/null 2>&1 || true
  cargo build --release --target "$TARGET"

  BIN="target/$TARGET/release/tcon"
  EXT=""
  case "$TARGET" in
    *windows*) EXT=".exe" ;;
  esac
  BIN="${BIN}${EXT}"
  if [ ! -f "$BIN" ]; then
    echo "Missing binary: $BIN"
    exit 1
  fi

  NAME="tcon-${VERSION}-${TARGET}"
  if [ -n "$EXT" ]; then
    ZIP_PATH="dist/${NAME}.zip"
    python3 - <<PY
import zipfile
z = zipfile.ZipFile("$ZIP_PATH", "w", compression=zipfile.ZIP_DEFLATED)
z.write("$BIN", arcname="tcon.exe")
z.close()
PY
    shasum -a 256 "$ZIP_PATH" >> "$CHECKSUMS"
  else
    TAR_PATH="dist/${NAME}.tar.gz"
    tar -czf "$TAR_PATH" -C "$(dirname "$BIN")" "tcon"
    shasum -a 256 "$TAR_PATH" >> "$CHECKSUMS"
  fi
done

echo "Artifacts written to dist/"
