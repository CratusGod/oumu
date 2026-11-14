#!/usr/bin/env bash
# Remove what install.sh put in place. Settings and history stay (delete them by hand:
# ~/.config/Utsuri-rs, ~/.local/state/Utsuri-rs, ~/.local/share/dev.cratusgod.oumu, ~/.local/state/oumu).
set -euo pipefail
BIN="$HOME/.local/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"
[ -x "$BIN/utsuri" ] && "$BIN/utsuri" --unregister-shell || true
rm -f "$BIN/utsuri" "$BIN/oumu" \
  "$DATA/icons/hicolor/128x128/apps/dev.cratusgod.utsuri.png" "$DATA/icons/hicolor/128x128/apps/dev.cratusgod.oumu.png" \
  "$DATA/applications/dev.cratusgod.oumu.desktop" \
  "$CONFIG/autostart/dev.cratusgod.utsuri.desktop" "$CONFIG/autostart/dev.cratusgod.oumu.desktop"
echo "removed Utsuri and Oumu from ~/.local"
