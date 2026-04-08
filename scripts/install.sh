#!/usr/bin/env sh
set -eu

PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"

echo "Building tcon (release)..."
cargo build --release

mkdir -p "$BIN_DIR"
cp "target/release/tcon" "$BIN_DIR/tcon"
chmod +x "$BIN_DIR/tcon"

echo "Installed: $BIN_DIR/tcon"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "Note: add '$BIN_DIR' to PATH to run 'tcon' directly." ;;
esac
