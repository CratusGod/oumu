# Oumu

**Oumu** (オウム, "parrot") is a local-first writing assistant for Arch Linux. It checks spelling and
grammar in the app you're typing in, underlines mistakes in place, and adds a panel of AI tools:
rewrite, paraphrase, tone, translate, explain, chat, plagiarism and AI-text checks. It runs on any
Arch desktop, and on Hyprland it gets in-place overlays everywhere.

* **Grammar and spelling** come from [Harper](https://writewithharper.com), which runs offline on
  your computer. No key or account is needed.
* **AI tools** use your own key: Google Gemini (free keys from
  [AI Studio](https://aistudio.google.com/apikey)), OpenAI, Anthropic, OpenRouter or Groq. Paste it
  and the provider and model are picked for you. A busy or retired Gemini model hands over to the
  next one automatically.
* **Local model (optional):** run the AI tools on your own computer with
  [Ollama](https://ollama.com) instead: no key and no daily limit, but slower, and the model needs
  a few GB of disk and GPU memory. Settings → AI → Local model finds it and lists your models.
* **Translate anything:** text field contents, or text highlighted in any app (a web page, a PDF, a
  chat). Trigger it from a keybind, a DragShift slice, or a right-click item in Zen/Firefox.
* **Plagiarism check** needs no key. Optionally point it at your own SearXNG or a Brave Search key
  for steadier results (see "Plagiarism search" in [`docs/LINUX.md`](docs/LINUX.md)).
* **Themes:** 11 built-in, plus Matugen, which follows your wallpaper. See [`docs/THEMES.md`](docs/THEMES.md).

## Desktop support

| | Hyprland | X11 (any) | Other Wayland (GNOME, KDE, Sway…) |
|---|---|---|---|
| Panel, AI tools, translate, review list | ✓ | ✓ | ✓ |
| Reading and fixing text (GTK, Qt, Chromium, Firefox) | ✓ | ✓ | ✓ |
| Underlines in place, hover cards | ✓ | ✓ | suggestions show in the panel |
| Paste-based Accept (Chromium, Firefox) | ✓ | with `xdotool` | with `wtype` (wlroots) |
| Edge tab, docked panel above windows | ✓ layer-shell | normal windows | layer-shell where supported |

Tested apps on Hyprland: GTK 3, GTK 4 (GNOME Text Editor), Qt 6, Chromium and Firefox. Each passes
all 15 live checks of `tests/e2e/host_linux.sh`. Details, limits and troubleshooting are in
[`docs/LINUX.md`](docs/LINUX.md).

## Install (Arch)

```bash
git clone https://github.com/CratusGod/oumu.git && cd oumu/packaging/arch
makepkg -si
```

Then launch **Oumu** from your app launcher and click **Turn on accessibility** in its panel. Restart
browsers and Electron apps that were already open. Oumu remembers the choice.

Optional extras: `hyprland` (full overlays), `wl-clipboard` (highlight translation), `wtype` or
`xdotool` (paste on other desktops), `matugen` (wallpaper theme outside Ryoku).

### On Ryoku

Start Oumu at login: Ryoku Hub → Session → Autostart command → `oumu`. **Recommended:** install
**DragShift** from RyoStore too. Then highlight text anywhere, drag it, tap Shift and pick
**Translate**.

### Translate from anywhere

Bind `oumu --translate` to a key (on Ryoku: Hub → Keybinds). It translates whatever text is
highlighted and opens the panel with the result. For the right-click item in Zen or Firefox, see
"Translate with Oumu" in [`docs/LINUX.md`](docs/LINUX.md).

## Using it

* Oumu starts as a thin strip at the screen edge (left by default; Settings → Dock side). Point at
  the strip to slide the tab out, then click it to open the panel. **Esc** tucks the panel away.
* Click into any text field: suggestions appear in the panel and as underlines. Hover an underline
  for its card, and click **Accept** to fix it in place.
* Settings → AI: paste a key. Settings → Appearance: pick a theme.

## Build from source

```bash
pnpm install                # also installs ui/
cd src-tauri && cargo build # the debug build loads the Vite dev server (pnpm --dir ui dev)
```

Tests:

```bash
cd src-tauri && cargo test             # unit tests, plus a matugen integration test when matugen is installed
cd ui && pnpm test && pnpm exec tsc --noEmit
TARGET=gtk3 tests/e2e/host_linux.sh    # live gate: gtk3, gtk4, qt, chromium, firefox
```

The live gate focuses windows and moves the pointer, so leave the desktop alone while it runs.
`OUMU_DESKTOP=x11|wayland` forces the generic desktop backend for testing.

## Credits

Grammar checking is [Harper](https://github.com/Automattic/harper). Licensed under the GNU
AGPL-3.0 (see `LICENSE` and `NOTICE`).
