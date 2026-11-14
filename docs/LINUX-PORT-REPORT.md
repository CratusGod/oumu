# Linux port: morning report (2026-11-15)

Both apps build, run and pass their tests on your Hyprland desktop. Work is on branch
`linux-port` in `~/AI/cratus-rs`: 12 local commits, privacy-scanned, **not pushed**.

## Do this when you're up (about 10 minutes)

1. **Sign out and back in once.** Utsuri's trigger needs the `input` group, and your
   current session started before you joined it.
2. **Remove the old Oumu package:** `sudo pacman -R oumu-git`. The new Oumu is already
   running from `~/.local/bin/oumu`, and your Hub's login autostart (`oumu`) now finds it
   first. The old package only adds a second "Oumu" launcher entry.
3. **Pick a trigger.** The Ryoku Utsuri plugin (`user.lua`, UTSURI-TRIGGER) also opens a
   menu on Shift+drag, so the new Utsuri would open a second one. Either set the new
   Utsuri to Ctrl or Alt in its Settings, or turn the plugin off. Then start it with
   `utsuri`. Add login autostart with `packaging/linux/install.sh --no-build --utsuri-autostart`.
4. **Before merging, build on Windows.** The Windows code now sits behind `cfg(windows)`,
   and this box can't compile for Windows, so that side is unchecked.

## Results

| Suite | Linux | Windows baseline |
|---|---|---|
| `cargo test --workspace` | 207 pass | 191 |
| Utsuri headless (`tests/linux/headless.py`) | 67/67 (debug and release) | 67 |
| Oumu headless | 88/88 (debug and release) | 88 |
| Utsuri E2E, real drags (`cargo run -p utsuri-e2e`) | **65/65** (debug and release) | 65/65 |
| Oumu E2E, real hover/click + AI (`tests/linux/oumu_e2e.py`) | **48/48** (release) | 45/45 |

The Oumu E2E covers:
- a real hover opening the card, and a real click on Accept fixing the word in the app
- 9 AI tools on your Gemini key (copied into the test folder, never printed)
- all 8 panel tools pressed through AT-SPI, with every button named
- the panel floated and pinned at the dock
- a web-page highlight read through PRIMARY and translated

## Measured

- Release build: 18 min. Installed binaries: Utsuri 74 MB, Oumu 59 MB (debug info stripped
  on install; the built ones are 440 and 260 MB).
- Utsuri drop to result: ~0.25 s for a JPG to PNG conversion.
- Overlay drawing during a drag: ~180 draws/s, which is your DP-3's refresh rate. Wayland
  paces overlays itself, so the Windows-only pacing fix isn't needed here (Windows hit ~680/s).
- Oumu hovering: 0 ms worst stall over repeated hovers.

## Bugs found and fixed tonight

1. **Overlays sat 39 px too low.** Rune's layer surfaces respected the bar's reserved zone.
   I patched `vendor/rune` (`layer_shell.rs`, exclusive zone -1 for placed overlays).
2. **About 1 drop in 12 was lost.** There were two causes:
   - The button-up (evdev) arrived before Wayland's drop data.
   - The menu's surface sometimes mapped on top of the drop surface.

   Both are fixed in `crates/utsuri/src/linux/dnd.rs`.
3. **Hyprland 0.57 here only accepts Lua dispatches.** The legacy
   `dispatch movewindowpixel …` form is refused, so all placement now uses `hl.dsp.*`.

## How it works on Linux (short)

**Utsuri**
- Trigger: evdev reads the keys and buttons; Hyprland IPC gives the pointer.
- Drops: an invisible Wayland surface over the menu receives them, because Rune's overlays
  can't take drops.
- PDF pages use poppler; HEIC/AVIF use libheif.
- Right-click: a Dolphin service menu, plus an "Open with" entry.

**Oumu**
- The AT-SPI backend from the original Oumu is reused.
- The panel is a normal window that Hyprland floats, pins and moves.
- The tab, button, card and underlines are layer-shell overlays.

Details are in `docs/NOTES.md` → "Linux port".

## Not done or not verified

- **The Windows build** (see step 4 above).
- **Multiple monitors:** this box has one, so placement off the first monitor is untested.
- **Desktops other than Hyprland:** the pointer and the panel placement use Hyprland IPC.
- **Utsuri's resize grip** does nothing on Linux. Use Super + right-drag.
- **Utsuri tool windows** open as normal Hyprland windows, tiled unless a window rule
  floats them. I didn't judge how that looks.
- **The browser extension's native-messaging host** came with `oumu-git`; `install.sh`
  doesn't install it. Say if you use the extension.
- **Oumu memory over a full day** is still unmeasured (also open on Windows).

## What changed on your machine

- Installed `~/.local/bin/{utsuri,oumu}`, icons, app entries, and a Dolphin service menu
  (`~/.local/share/kio/servicemenus/utsuri.desktop`).
- Stopped the old Oumu and started the new one. Your settings at
  `~/.local/share/dev.cratusgod.oumu` are shared by both versions.
- Started the `ydotool` user service for the tests (stopped again at the end). No config
  files were edited.
- The tests ran on workspace 9 and switched back afterwards.
