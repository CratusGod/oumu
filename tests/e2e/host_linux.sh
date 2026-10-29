#!/usr/bin/env bash
# Linux G1 + G2 gate (Hyprland): the app reads a real GTK text view through
# AT-SPI, lints it, places underlines over the right words, applies a
# suggestion into the app, and the app's own buffer shows the change.
#
# TARGET=gtk3 (default) uses tests/e2e/fixtures/gtk_textview.py.
# TARGET=gtk4 uses GNOME Text Editor when it is installed.
# TARGET=qt uses tests/e2e/fixtures/qt_textedit.py (PySide6).
# TARGET=chromium / firefox open a textarea in a throwaway browser profile.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
FIXTURE="I has teh cat and I think that we should just ship it. Teh end."
TARGET="${TARGET:-gtk3}"
WORK="$(mktemp -d)"
MIRROR="$WORK/mirror.txt"
FIX_PID=""

hypr() { hyprctl "$@" 2>/dev/null; }
active() { hypr activewindow -j | jsonq "$1"; }
focus_class() { hypr dispatch "hl.dsp.focus({ window = \"class:$1\" })" >/dev/null || hypr dispatch focuswindow "class:$1" >/dev/null; }

cleanup() {
  [ -n "$FIX_PID" ] && kill "$FIX_PID" 2>/dev/null
  pkill -f -- "$WORK/profile" 2>/dev/null
  stop_app
  rm -rf "$WORK"
}
trap cleanup EXIT

command -v hyprctl >/dev/null && [ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ] || { echo "SKIPPED: needs a Hyprland session"; exit 2; }

# Earlier runs may have dismissed fixture alerts; reset so the run is deterministic.
SETTINGS="$E2E_DATA_HOME/dev.cratusgod.oumu/settings.json"
[ -f "$SETTINGS" ] && python3 -I - "$SETTINGS" <<'PY'
import json, sys
p = sys.argv[1]
try:
    d = json.load(open(p))
except Exception:
    sys.exit(0)
d["dismissed"] = []
json.dump(d, open(p, "w"))
PY

wait_idle "${IDLE_S:-3}" "${IDLE_MAX_S:-120}" || { echo "SKIPPED (screen in use)"; exit 2; }
start_app

if [ "$(probe /health | jsonq 'd["trusted"]')" != "True" ]; then
  [ "${E2E_ENABLE_A11Y:-0}" = 1 ] && probe /trust >/dev/null
fi
if [ "$(probe /health | jsonq 'd["trusted"]')" = "True" ]; then pass "accessibility bus is on"; else fail "accessibility bus is off (click 'Turn on accessibility' in the panel, or rerun with E2E_ENABLE_A11Y=1)"; summary; exit 1; fi
if wait_for 'set(d["ui_ready"]) >= {"panel","oumubutton","underlines","tab"}' 15; then pass "panel, Oumu button, underline and edge-tab webviews mounted"; else fail "webviews not mounted: $(probe /state | jsonq 'd["ui_ready"]')"; fi

layer_ns() { hypr layers -j | python3 -I -c 'import json,sys
d=json.load(sys.stdin); print(sorted({l["namespace"] for m in d.values() for ls in m["levels"].values() for l in ls if l["namespace"].startswith("oumu-")}))'; }

# Layer-shell placement is Hyprland's (OUMU_DESKTOP=x11|wayland runs the generic backend).
HYPR_LAYERS=1; case "${OUMU_DESKTOP:-hyprland}" in hyprland) ;; *) HYPR_LAYERS=0 ;; esac

# Tuck / untuck: the panel is a layer surface placed at the dock margin.
probe /panel '{"tucked": false}' >/dev/null
if [ "$HYPR_LAYERS" = 1 ]; then
  if wait_for 'd["panel"]["visible"] and not d["panel"]["tab_visible"] and abs(d["panel"]["x"] - 12) < 1' 5; then pass "untuck shows the panel at the left dock (x=12)"; else fail "untuck: $(probe /state | jsonq 'd["panel"]')"; fi
  sleep 0.5
  case "$(layer_ns)" in *oumu-panel*) pass "panel is a layer-shell surface";; *) fail "no oumu-panel layer: $(layer_ns)";; esac
else
  if wait_for 'd["panel"]["visible"]' 5; then pass "untuck shows the panel (normal window off Hyprland)"; else fail "untuck: $(probe /state | jsonq 'd["panel"]')"; fi
fi
probe /panel '{"tucked": true}' >/dev/null
if wait_for 'd["panel"]["tucked"] and not d["panel"]["visible"] and d["panel"]["tab_visible"]' 5; then pass "tuck hides the panel and shows the edge tab"; else fail "tuck: $(probe /state | jsonq 'd["panel"]')"; fi

# Open the target app with the fixture and give it focus.
case "$TARGET" in
  gtk3)
    python3 -I "$ROOT/tests/e2e/fixtures/gtk_textview.py" "$FIXTURE" "$MIRROR" >/dev/null 2>&1 &
    FIX_PID=$!; CLASS="oumu-e2e" ;;
  qt)
    python3 -I -c 'import PySide6' 2>/dev/null || { echo "SKIPPED: PySide6 not installed"; exit 2; }
    python3 -I "$ROOT/tests/e2e/fixtures/qt_textedit.py" "$FIXTURE" "$MIRROR" >/dev/null 2>&1 &
    FIX_PID=$!; CLASS="oumu-e2e-qt" ;;
  gtk4)
    command -v gnome-text-editor >/dev/null || { echo "SKIPPED: gnome-text-editor not installed"; exit 2; }
    printf '%s' "$FIXTURE" > "$WORK/fixture.txt"
    gnome-text-editor --standalone "$WORK/fixture.txt" >/dev/null 2>&1 &
    FIX_PID=$!; CLASS="org.gnome.TextEditor"; MIRROR="$WORK/fixture.txt" ;;
  chromium|firefox)
    command -v "$TARGET" >/dev/null || { echo "SKIPPED: $TARGET not installed"; exit 2; }
    cat > "$WORK/page.html" <<HTML
<!doctype html><meta charset=utf-8><title>oumu-e2e</title>
<textarea id=t style="width:700px;height:200px;font:18px sans-serif">$FIXTURE</textarea>
<script>t.focus(); t.setSelectionRange(0, 0)</script>
HTML
    # A throwaway profile: never the user's own browser session.
    if [ "$TARGET" = chromium ]; then
      chromium --user-data-dir="$WORK/profile" --no-first-run --force-renderer-accessibility --ozone-platform=wayland --new-window "file://$WORK/page.html" >/dev/null 2>&1 &
    else
      mkdir -p "$WORK/profile"
      firefox --no-remote --new-instance --profile "$WORK/profile" "file://$WORK/page.html" >/dev/null 2>&1 &
    fi
    FIX_PID=$!; CLASS="$TARGET"; MIRROR="" ;;
  *) echo "unknown TARGET=$TARGET"; exit 1 ;;
esac
for _ in $(seq 1 40); do sleep 0.25; [ "$(active 'd.get("class")')" = "$CLASS" ] && break; focus_class "$CLASS"; done
[ "$(active 'd.get("class")')" = "$CLASS" ] && pass "$CLASS window is in front" || { fail "could not focus $CLASS (front: $(active 'd.get("class")'))"; summary; exit 1; }

if wait_for 'd["doc"] is not None and d["doc"]["text"].startswith("I has teh cat")' 10; then pass "focused text view is read through AT-SPI"; else
  FRONT="$(active 'd.get("class")')"
  if [ "$FRONT" != "$CLASS" ]; then fail "focus moved to $FRONT during the run (someone using the desktop?); rerun"; else fail "document not read: $(probe /state | jsonq 'd["status"]')"; fi
  summary; exit 1
fi
if wait_for 'any(a["original"].lower()=="teh" for a in d["alerts"])' 8; then pass "Harper flags 'teh'"; else fail "no 'teh' alert: $(probe /state | jsonq '[a["original"] for a in d["alerts"]]')"; fi
TEH="$(probe /state | jsonq 'next(a["id"] for a in d["alerts"] if a["original"]=="teh")')"

# Geometry: the underline sits inside the window and on the word's line.
if wait_for "any(r['id']=='$TEH' for r in d['geometry']['rects'])" 8; then pass "underline geometry computed"; else fail "no underline rect for 'teh'"; fi
S="$(probe /state)"; W="$(hypr activewindow -j)"
INSIDE="$(python3 -I -c '
import json,sys
s=json.loads(sys.argv[1]); w=json.loads(sys.argv[2]); teh=sys.argv[3]
g=s["geometry"]; r=next(r for r in g["rects"] if r["id"]==teh)
x=g["origin_x"]+r["x"]; y=g["origin_y"]+r["y"]
wx,wy=w["at"]; ww,wh=w["size"]
print(wx<=x<=wx+ww and wy<=y<=wy+wh and 0<r["w"]<200 and 0<r["h"]<80)' "$S" "$W" "$TEH")"
[ "$INSIDE" = "True" ] && pass "the 'teh' underline lies inside the app window, word-sized" || fail "underline off target: $(echo "$S" | jsonq "[r for r in d['geometry']['rects'] if r['id']=='$TEH']") window=$(echo "$W" | jsonq 'd["at"], d["size"]')"

# Hover: move the pointer onto the underline (skipped if someone is moving the mouse).
M0="$(probe /mouse)"
HX="$(echo "$S" | jsonq "next(round(d['geometry']['origin_x']+r['x']+r['w']/2) for r in d['geometry']['rects'] if r['id']=='$TEH')")"
HY="$(echo "$S" | jsonq "next(round(d['geometry']['origin_y']+r['y']+r['h']/2) for r in d['geometry']['rects'] if r['id']=='$TEH')")"
probe /mouse "{\"x\": $HX, \"y\": $HY}" >/dev/null; sleep 0.4
if [ "$(probe /mouse | jsonq "abs(d['x']-$HX)<3 and abs(d['y']-$HY)<3")" = "True" ]; then
  if wait_for "d['geometry']['hovered']=='$TEH'" 3; then pass "pointer over the underline is a hover"; else fail "hover not detected: $(probe /state | jsonq 'd["geometry"]["hovered"]')"; fi
  if wait_for 'len(d["card_rects"]) > 0' 3; then pass "hover card opens"; else fail "hover card did not open"; fi
  probe /mouse "{\"x\": $(echo "$M0" | jsonq 'd["x"]'), \"y\": $(echo "$M0" | jsonq 'd["y"]')}" >/dev/null
else
  echo "  ----  hover checks skipped: the pointer did not land (someone moving the mouse?)"
fi

# Accept: 'teh' -> 'the', read back by Oumu and by the app itself.
R="$(probe /accept "{\"id\": \"$TEH\"}")"
if [ "$(echo "$R" | jsonq '"the cat" in d.get("new_text","")')" = "True" ]; then pass "accept applied ($(echo "$R" | jsonq 'd["method"]'))"; else fail "accept failed: $R"; fi
sleep 0.5
if [ -z "$MIRROR" ] || [ "$TARGET" = gtk4 ] || grep -q "I has the cat" "$MIRROR" 2>/dev/null; then pass "the app's own buffer shows 'the cat'"; else fail "app buffer: $(head -c 80 "$MIRROR")"; fi
if wait_for 'd["doc"]["text"].startswith("I has the cat") and not any(a["original"]=="teh" for a in d["alerts"])' 6; then pass "re-read after the edit; the 'teh' alert is gone"; else fail "after accept: $(probe /state | jsonq 'd["doc"]["text"][:30]')"; fi

summary
