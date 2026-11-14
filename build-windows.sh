#!/usr/bin/env sh
# Cross-compile Colonia Galactica for 64-bit Windows from Linux and package
# it as a zip in dist/.
#
# One-time setup:
#   sudo apt install mingw-w64
#   rustup target add x86_64-pc-windows-gnu
#
# Usage: ./build-windows.sh

set -eu

TARGET=x86_64-pc-windows-gnu
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
NAME="colonia-galactica-$VERSION-windows-x64"
OUT="dist/$NAME"

if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    echo "error: mingw-w64 not found. Run: sudo apt install mingw-w64" >&2
    exit 1
fi
if ! rustup target list --installed 2>/dev/null | grep -qx "$TARGET"; then
    echo "error: Rust target missing. Run: rustup target add $TARGET" >&2
    exit 1
fi

echo "==> Building release binaries for $TARGET..."
cargo build --release --bins --target "$TARGET"

rm -rf "$OUT" "dist/$NAME.zip"
mkdir -p "$OUT"
for b in colonia-galactica colonia-galactica-sim; do
    cp "target/$TARGET/release/$b.exe" "$OUT/"
done
cp README.md CHANGELOG.md "$OUT/"

(cd dist && zip -qr "$NAME.zip" "$NAME")
echo "==> Done: dist/$NAME.zip"
