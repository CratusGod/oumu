# Oumu on Linux

Oumu runs on any Arch desktop. Hyprland gets everything: the panel, the edge tab, the Oumu button,
underlines in place, hover cards, Accept into the host app, and every AI feature. X11 sessions get
underlines too. Other Wayland desktops (GNOME, KDE, Sway) show suggestions in the panel instead of
underlines. See the support table in the README.

## Install

```bash
git clone https://github.com/CratusGod/oumu.git && cd oumu/packaging/arch
makepkg -si
```

Runtime dependencies (pulled in by the package): `webkit2gtk-4.1 gtk3 gtk-layer-shell at-spi2-core
libayatana-appindicator wl-clipboard hyprland xdg-utils`.

## First run

1. Launch **Oumu** from the app launcher.
2. Click **Turn on accessibility** in the panel.
3. Restart browsers, Electron apps and Qt apps that were already open. Only apps started after the
   switch export their text.

Oumu remembers the choice and turns accessibility back on at every launch. The accessibility bus
turns itself off when the process that enabled it quits.

Start Oumu at login so it is on before your other apps:

```bash
mkdir -p ~/.config/autostart && cp /usr/share/applications/oumu.desktop ~/.config/autostart/
```

To turn accessibility off for good, quit Oumu, then run:

```bash
gsettings set org.gnome.desktop.interface toolkit-accessibility false
```

## Using it

- **Edge tab.** While idle Oumu is a thin strip at the screen edge (left by default; Settings → Dock
  side). Point at it and the tab slides out; click it for the full panel. Esc tucks the panel away
  while the pointer is on it. Move the pointer to your window and the keyboard goes back there.
- **Translate anything you highlight.** Highlight text in any app: a web page, a PDF, a chat.
  Then open Translate and click **Translate highlighted text**. To do it from anywhere, bind
  `oumu --translate` to a key in Ryoku Hub → Keybinds. The panel slides out with the translation.
  It reads the primary selection, the text a middle click would paste.

### Right-click "Translate with Oumu" in Zen or Firefox

The package installs the extension in `/usr/share/oumu/browser-extension` and its native-messaging
bridge in `/usr/lib/mozilla/native-messaging-hosts/`. Release builds of Firefox and Zen only keep
extensions that Mozilla has signed. Signing is free and automatic for a private ("unlisted") add-on:

1. Create API credentials at https://addons.mozilla.org/developers/addon/api/key/ (Mozilla account).
2. Sign it, which takes about a minute. The `.xpi` lands in `~/oumu-extension`:
   ```bash
   npx --yes web-ext@8.3.0 sign --channel=unlisted --source-dir /usr/share/oumu/browser-extension \
     --artifacts-dir ~/oumu-extension --api-key "$AMO_JWT_ISSUER" --api-secret "$AMO_JWT_SECRET"
   ```
3. In Zen open `about:addons`, then the gear icon, then **Install Add-on From File**, and pick that `.xpi`.

To try it without signing, open `about:debugging#/runtime/this-firefox`, click **Load Temporary
Add-on**, and pick `/usr/share/oumu/browser-extension/manifest.json`. It stays until the browser
closes. Then highlight text on a page, right-click, and choose **Translate with Oumu**. If Oumu
isn't running, the bridge starts it.

### Plagiarism search

Plagiarism checks work without any setup: Oumu searches Brave's, then Bing's, then DuckDuckGo's
public results pages. Many checks in a row can get those pages to block you for a while. For
steady results, run your own [SearXNG](https://docs.searxng.org), which is free with no key or limit:

1. Write its settings (JSON output on, rate limiter off, since only you use it):
   ```bash
   mkdir -p ~/.config/searxng && printf 'use_default_settings: true\nserver:\n  secret_key: "%s"\n  limiter: false\nsearch:\n  formats: [html, json]\n' "$(openssl rand -hex 32)" > ~/.config/searxng/settings.yml
   ```
2. Start it. It listens on this computer only and restarts with Docker:
   ```bash
   docker run -d --name searxng --restart unless-stopped -p 127.0.0.1:8888:8080 -v ~/.config/searxng:/etc/searxng searxng/searxng
   ```
3. In Oumu, open Settings, then Plagiarism search. Enter `http://localhost:8888` as the SearXNG
   address and click **Test**.

A Brave Search API key also works (Settings, same section). Oumu tries the Brave key first, then
SearXNG, then the free pages. Any search that fails or finds nothing falls back to the free pages,
and the result says why. Google's free search API closed to new users and shuts down on
2027-01-01, so Oumu does not use it.

### DragShift

With the DragShift plugin (0.5.0 or later), drag highlighted text and tap Shift. The Text ring offers
**Translate** while Oumu runs.

## What works where

Tested 2026-10-27 on Hyprland 0.56.2, at-spi2-core 2.62.1, a single 1920x1080 output at scale 1.
Each row passed all 15 checks of `tests/e2e/host_linux.sh`: read, lint, underline placement,
hover, hover card, Accept, and the app's own buffer showing the change.

| App / toolkit | Version | Underlines | Accept method |
|---|---|---|---|
| GTK 3 (`TextView`) | 3.24.52 | on the word | in place |
| GTK 4 (GNOME Text Editor) | 4.24.1 / 51.0 | on the word | in place |
| Qt 6 (`QPlainTextEdit`) | 6.12.0 | on the word | in place |
| Chromium (`<textarea>`) | 156 | on the word, wrapped lines too | select + paste |
| Firefox (`<textarea>`) | 158 | on the word | select + paste |

Browser and Electron notes:
- Chromium and Electron export text only with accessibility on at their start. If they don't, start
  them with `--force-renderer-accessibility`.
- Firefox, Zen and Thunderbird (Gecko) are always pasted into. Their in-place edit puts the new
  text at the caret instead of over the word.

Not supported:
- Terminals and password fields. They are skipped on purpose.
- Apps with no accessibility tree: games, SDL and raw X11 apps. The panel's own tools still work
  there; underlines do not.
- Rich editors that embed objects in the text (Google Docs canvas, some web editors). Fields whose
  text contains embedded objects are skipped, so an Accept cannot land in the wrong place.

## How it works

| Need | macOS | Linux |
|---|---|---|
| Focused field, text, selection | AX API | AT-SPI over D-Bus (`linux/ax.rs`) |
| Focus changes | AX polling | `object:state-changed:focused` events, plus a tree search of the front app when an event was missed |
| Frontmost app, window box, pointer | AppKit / CoreGraphics | Hyprland IPC socket (`linux/hyprland.rs`) |
| Underline position | screen coordinates | window-relative text boxes + the Hyprland window position |
| Accept | AX set value, then paste | EditableText delete + insert, then select + `wl-copy` + Hyprland `send_shortcut` Ctrl+V (`linux/apply.rs`) |
| Overlay windows | NSWindow levels | gtk-layer-shell surfaces (`linux/layer.rs`) |

Every Accept is read back. If the field does not show the expected text, Oumu reports a failure
and does not retry on top of a half-applied edit. The clipboard is restored after the paste lands,
and only if you have not copied something else meanwhile. Oumu's paste is marked sensitive, so
clipboard history skips it.

Toolkit quirks the code handles:
- GTK 4 reports a range's box as its first character only. Oumu joins the first and last
  character boxes.
- Chromium counts text boxes from the web page, below its toolbar, in window coordinates. Oumu
  picks, per field, the coordinate mode whose caret box lies inside the field.
- Chromium marks the page focused as well as the field. The tree search prefers the editable one.
- Chromium applies selections asynchronously, so Oumu waits for the selection before it pastes.
- Gecko mishandles `AddSelection`, so Oumu uses `SetSelection`.

## Troubleshooting

- **No underlines in one app.** Restart that app with Oumu running. It must start after
  accessibility is on.
- **Underlines in the wrong place.** Report the app, its toolkit and the output scale. Fractional
  scaling and multi-monitor setups are untested.
- **The panel shows on every workspace.** That is intended: it is a layer surface, like a bar.
  Tuck it with the edge tab.
- **Blank webviews on NVIDIA.** Oumu sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` for itself, unless you
  set that variable yourself.

## Testing

```bash
cd src-tauri && cargo build && cd ..
for t in gtk3 gtk4 qt chromium firefox; do TARGET=$t E2E_ENABLE_A11Y=1 tests/e2e/host_linux.sh; done
```

The gate focuses windows and moves the pointer, and waits while a fullscreen window is in front.
Browsers run in throwaway profiles. The Qt target needs `pyside6`.
