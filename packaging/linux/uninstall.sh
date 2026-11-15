#!/usr/bin/env bash
# Remove what install.sh put in place. Settings and history stay (delete them by hand:
# ~/.local/share/dev.cratusgod.oumu, ~/.local/state/oumu).
set -euo pipefail
BIN="$HOME/.local/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"
rm -f "$BIN/oumu" "$DATA/icons/hicolor/128x128/apps/dev.cratusgod.oumu.png" \
  "$DATA/applications/dev.cratusgod.oumu.desktop" "$CONFIG/autostart/dev.cratusgod.oumu.desktop"
echo "removed Oumu from ~/.local"
