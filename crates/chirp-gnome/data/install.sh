#!/bin/sh
# Install Chirp Wallpaper for the current user (no root needed).
set -e
cd "$(dirname "$0")"
BIN="$HOME/.local/bin"
SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm755 chirp-wallpaper "$BIN/chirp-wallpaper"
install -Dm755 chirp "$BIN/chirp"
install -Dm644 io.github.wi_wissen.ChirpWallpaper.desktop "$SHARE/applications/io.github.wi_wissen.ChirpWallpaper.desktop"
install -Dm644 io.github.wi_wissen.ChirpWallpaper.svg "$SHARE/icons/hicolor/scalable/apps/io.github.wi_wissen.ChirpWallpaper.svg"
command -v update-desktop-database >/dev/null && update-desktop-database "$SHARE/applications" || true
echo "Chirp Wallpaper installed. Open it from the app grid."
