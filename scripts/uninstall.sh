#!/usr/bin/env sh
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin/tcon"

if [ -f "$BIN" ]; then
  rm -f "$BIN"
  echo "Removed: $BIN"
else
  echo "Not installed at: $BIN"
fi
