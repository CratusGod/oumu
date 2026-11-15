#!/usr/bin/env bash
# Install Oumu for this user (no root): the binary in ~/.local/bin, its icon and app entry.
# Re-run to update.
#
#   packaging/linux/install.sh [--profile release|fast] [--autostart] [--no-build]
#
# A desktop that runs `oumu` at login (Ryoku's Hub does) starts this one, since ~/.local/bin
# comes first on PATH; --autostart adds an XDG autostart entry for other desktops.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROFILE=release
BUILD=1
AUTOSTART=0
while [ $# -gt 0 ]; do
  case "$1" in
    --profile) PROFILE="$2"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    --autostart) AUTOSTART=1; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

BIN="$HOME/.local/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}"
ICONS="$DATA/icons/hicolor/128x128/apps"
APPS="$DATA/applications"

need() { command -v "$1" >/dev/null || { echo "missing: $1 ($2)" >&2; MISSING=1; }; }
MISSING=0
need cargo "sudo pacman -S rust"
need wl-paste "sudo pacman -S wl-clipboard (highlight-anywhere)"
[ "$MISSING" = 0 ] || echo "(the app runs without the optional tools; its features say what is missing)"

if [ "$BUILD" = 1 ]; then
  echo "== building ($PROFILE)"
  (cd "$ROOT" && cargo build --profile "$PROFILE" -p oumu)
fi
OUT="$ROOT/target/$PROFILE"
[ -x "$OUT/oumu" ] || { echo "build first: $OUT/oumu missing" >&2; exit 1; }

# a running copy keeps its old file open; install by rename so it is never half-written.
# The build keeps line tables for crash logs (hundreds of MB on Linux, inside the binary);
# the installed copy drops them and keeps the symbol names.
install_bin() {
  if command -v strip >/dev/null; then
    strip --strip-debug -o "$BIN/.$1.new" "$OUT/$1"
    chmod 755 "$BIN/.$1.new"
  else
    install -m 755 "$OUT/$1" "$BIN/.$1.new"
  fi
  mv -f "$BIN/.$1.new" "$BIN/$1"
}
refresh() {
  command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$DATA/icons/hicolor" 2>/dev/null || true
}

echo "== installing"
mkdir -p "$BIN" "$ICONS" "$APPS"
install_bin oumu
install -m 644 "$ROOT/crates/oumu/icons/128x128.png" "$ICONS/dev.cratusgod.oumu.png"
cat > "$APPS/dev.cratusgod.oumu.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Oumu
GenericName=Writing Assistant
Comment=Grammar, spelling and AI writing tools in any text field
Exec=$BIN/oumu
Icon=dev.cratusgod.oumu
Terminal=false
Categories=Utility;TextTools;
Keywords=grammar;spelling;writing;proofread;
StartupNotify=false
DESK
refresh

if [ "$AUTOSTART" = 1 ]; then
  mkdir -p "$CONFIG/autostart"
  cat > "$CONFIG/autostart/dev.cratusgod.oumu.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Oumu
Exec=$BIN/oumu
Icon=dev.cratusgod.oumu
Terminal=false
X-GNOME-Autostart-enabled=true
DESK
fi

echo "== done: $BIN/oumu"
case ":$PATH:" in *":$BIN:"*) ;; *) echo "  add $BIN to PATH" ;; esac
