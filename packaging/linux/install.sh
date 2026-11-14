#!/usr/bin/env bash
# Install Utsuri and Oumu for this user (no root): binaries in ~/.local/bin, icons, app
# entries, and Utsuri's right-click entry (Dolphin) / "Open with" entry. Re-run to update.
#
#   packaging/linux/install.sh [--profile release|fast] [--utsuri-autostart] [--no-build]
#
# Oumu: a desktop that runs `oumu` at login (Ryoku's Hub does) starts this one, since
# ~/.local/bin comes first on PATH. Utsuri does NOT start at login unless asked: its Shift
# trigger would fight another Shift+drag menu (pick Ctrl or Alt in its Settings first).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROFILE=release
BUILD=1
UTSURI_AUTOSTART=0
while [ $# -gt 0 ]; do
  case "$1" in
    --profile) PROFILE="$2"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    --utsuri-autostart) UTSURI_AUTOSTART=1; shift ;;
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
need pdftoppm "sudo pacman -S poppler"
need heif-dec "sudo pacman -S libheif"
need ffmpeg "sudo pacman -S ffmpeg (video/audio)"
need tesseract "sudo pacman -S tesseract tesseract-data-eng (OCR)"
need wl-paste "sudo pacman -S wl-clipboard (Oumu's highlight-anywhere)"
[ "$MISSING" = 0 ] || echo "(the apps run without the optional tools; their features say what is missing)"

if [ "$BUILD" = 1 ]; then
  echo "== building ($PROFILE)"
  (cd "$ROOT" && cargo build --profile "$PROFILE" -p utsuri -p oumu)
fi
OUT="$ROOT/target/$PROFILE"
[ -x "$OUT/utsuri" ] && [ -x "$OUT/oumu" ] || { echo "build first: $OUT/{utsuri,oumu} missing" >&2; exit 1; }

echo "== installing"
mkdir -p "$BIN" "$ICONS" "$APPS"
# a running copy keeps its old file open; install by rename so it is never half-written
# the build keeps line tables for crash logs (~440 MB on Linux, where they sit inside the
# binary); the installed copy drops them and keeps the symbol names (~75 MB)
for app in utsuri oumu; do
  if command -v strip >/dev/null; then
    strip --strip-debug -o "$BIN/.$app.new" "$OUT/$app"
    chmod 755 "$BIN/.$app.new"
  else
    install -m 755 "$OUT/$app" "$BIN/.$app.new"
  fi
  mv -f "$BIN/.$app.new" "$BIN/$app"
done
install -m 644 "$ROOT/crates/utsuri/assets/utsuri.png" "$ICONS/dev.cratusgod.utsuri.png"
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
# Utsuri's own app entry, Dolphin service menu and "Open with" (written by the app itself)
"$BIN/utsuri" --register-shell
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$DATA/icons/hicolor" 2>/dev/null || true

if [ "$UTSURI_AUTOSTART" = 1 ]; then
  mkdir -p "$CONFIG/autostart"
  cat > "$CONFIG/autostart/dev.cratusgod.utsuri.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Utsuri
Exec=$BIN/utsuri --quiet
Icon=dev.cratusgod.utsuri
Terminal=false
X-GNOME-Autostart-enabled=true
DESK
fi

echo "== done"
echo "  $BIN/utsuri  $BIN/oumu"
id -nG | tr ' ' '\n' | grep -qx input || echo "  Utsuri's trigger needs the input group: sudo usermod -aG input \$USER, then sign out and in."
case ":$PATH:" in *":$BIN:"*) ;; *) echo "  add $BIN to PATH" ;; esac
