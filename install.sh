#!/usr/bin/env sh
# Build FTL Drift (spacegame) in release mode and install it for this user,
# with a desktop launcher and icon so it shows up in the KDE (or any
# freedesktop) application menu.
#
# Usage:
#   ./install.sh                  # binaries to ~/.local/bin
#   INSTALL_DIR=/path ./install.sh
#   ./install.sh --uninstall      # remove everything this script installed
#
# Linux only. (Windows: see "Building on Windows" in README.md.)

set -eu

BIN=spacegame
SIM=spacegame-sim
DEST_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_DIR="$DATA_HOME/applications"
ICON_DIR="$DATA_HOME/icons/hicolor/256x256/apps"

refresh_menus() {
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$APP_DIR" || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$DATA_HOME/icons/hicolor" || true
    # KDE Plasma: rebuild the service cache so the launcher appears at once.
    for k in kbuildsycoca6 kbuildsycoca5; do
        if command -v "$k" >/dev/null 2>&1; then "$k" >/dev/null 2>&1 || true; break; fi
    done
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$DEST_DIR/$BIN" "$DEST_DIR/$SIM" "$APP_DIR/$BIN.desktop" "$ICON_DIR/$BIN.png"
    refresh_menus
    echo "==> Removed. (Saves are kept in $DATA_HOME/spacegame.)"
    exit 0
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install Rust from https://rustup.rs first." >&2
    exit 1
fi

echo "==> Building release binaries..."
cargo build --release --bins

mkdir -p "$DEST_DIR" "$APP_DIR" "$ICON_DIR"
for b in "$BIN" "$SIM"; do
    src="$PWD/target/release/$b"
    if [ ! -x "$src" ]; then
        echo "error: $src not found after build." >&2
        exit 1
    fi
    echo "==> Installing $b -> $DEST_DIR/$b"
    cp -f "$src" "$DEST_DIR/$b"
    chmod +x "$DEST_DIR/$b"
done

echo "==> Installing launcher and icon"
cp -f assets/icon.png "$ICON_DIR/$BIN.png"
# Absolute Exec path: the launcher works even if DEST_DIR is not in the
# desktop session's PATH.
sed "s|@BIN@|$DEST_DIR/$BIN|" assets/spacegame.desktop.in > "$APP_DIR/$BIN.desktop"
chmod 644 "$APP_DIR/$BIN.desktop"
refresh_menus

case ":$PATH:" in
    *":$DEST_DIR:"*) ;;
    *)
        echo
        echo "note: $DEST_DIR is not in your PATH (the menu launcher still works)."
        echo "      To run it from a terminal, add to your shell rc:"
        echo "        export PATH=\"$DEST_DIR:\$PATH\""
        ;;
esac

echo "==> Done. Launch 'FTL Drift' from the application menu, or run: $BIN"
