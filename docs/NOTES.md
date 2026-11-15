# Rust rebuild notes (Windows only)

## Decisions
- **UI (2026-11-13):** both apps run on **Rune** (github.com/anomalyco/rune, pinned rev
  `c369cc0`, feature `tooling`). The egui version is kept on the local branch
  `egui-snapshot`. Rune renders every window headlessly (`--snap`, `--script`, `--tree`),
  so UI work and tests never take over the screen or mouse.
- The workspace repeats Rune's four `[patch.crates-io]` entries (winit, parley_engine,
  wgpu-hal, accesskit_windows); keep their rev equal to rune's.
- **Platform:** Windows only. **Layout:** one workspace: `shared`, `utsuri-engine` (all
  file work, no UI), `utsuri` (app), `utsuri-e2e` (real-drag test harness), `oumu-rt`,
  `oumu`, `install` + `setup` (per-user installers).

## Rune notes
- `App::overlay` surfaces (menu, toasts, Oumu's tab/button/card) never take focus. They
  aren't rendered headless, so each app's `headless.rs` re-hosts them as windows.
- No glass since 2026-11-14 (the user: the see-through effect flickered, the corners were
  off, and the screen capture behind it cost CPU). Tool windows are opaque with the theme
  surface and Windows 11 rounded corners (DWMWA_WINDOW_CORNER_PREFERENCE); the menu's petals
  and hub and the toast cards are solid theme colours. Everything is screen-capturable. The
  egui-era glass shader (`glass.wgsl` + compositor) is kept on the `egui-snapshot` branch.
- `.wrap()` is text wrap; `.flex_wrap()` wraps children. No percentage widths: progress
  bars are two `grow` children (`style::meter`). `.shrink()` opts an element into shrinking.
- Fields made on a press don't autofocus; use `tx.focus(key)`. `.autofocus()` re-takes
  focus on every rebuild: give a field `TextEdit.focused = true` at start instead.
- Keys with nothing focused reach `AppSpec.on_key` only (an El's `on_key` needs focus).
- Real-screen lessons (2026-11-14, all fixed):
  - Overlays carry winit's default OLE drop target, whose events Rune ignores:
    `utsuri/src/droptarget.rs` revokes it and registers ours.
  - Text sent as `KEYEVENTF_UNICODE` (VK_PACKET) was dropped. Our `vendor/winit` copy maps
    it to a character (grep "cratus-rs patch").
  - A window opened from a drop isn't focused: `tools::bring_to_front` for 1 s.
  - Resizing an overlay makes Rune rebuild its caches (`live.scale = 0; rescale`), slower
    each time when it shows images. Utsuri's notifications are a fixed-size window with
    click-through around the cards (`OverlayInput::Regions` + `.hit()`). Oumu's card has
    no images: resizing measured 0 ms, kept.
  - Regions click-through follows a low-level mouse hook: `SetCursorPos` alone is never
    seen. Tests must send real mouse events.
  - A closed window keeps its GPU device unless `App::release_gpu_when_closed()`.
  - First-use costs moved to startup: ffmpeg/Tesseract detection (a thread) and the menu's
    first show (it draws once at 1% opacity at startup).
  - The trigger is sampled on the tick thread and queued, so a busy UI thread can't
    lose a Shift tap. "ui thread was busy for N ms" in the log marks stalls.
- **Rune is vendored (2026-11-14)** at `vendor/rune` (rev c369cc0) for our patches; grep
  "cratus-rs". (1) Tried and reverted the same day: a window of another `App` reusing the GPU
  device a live surface draws with. It made tool windows no faster, and Oumu grew from
  245 MB to 1,045 MB in 1.7 h (24 write-combined 32 MB GPU upload blocks + one 352 MB heap
  block; Utsuri had 4 such blocks). The patch was not the cause: measured after the revert
  (`tests/tools/memmap.ps1` every 2 s during oumu_e2e), Oumu starts at ~290 MB private with
  5 blocks, the first underline + card show adds ~175 MB (3 more blocks), the panel's first
  open adds ~147 MB (3 more), and 10 more hovers add nothing. So every Rune surface costs
  ~150 MB at its first show (about 96 MB of it upload blocks) and it is never given back;
  the morning's 24 blocks are not explained yet (surfaces re-made after their GPU was
  parked?). RUNE_GPU_LOG=1 through oumu_e2e (OUMU_E2E_STDERR=file) shows one device for the whole run, so
  it is per-surface upload-heap memory, not a device each; rune-staging keeps at most 4 MB.
  Solved by (3) below: with the overlays paced, the same test stays at 285 -> 329 MB with no
  new blocks. `OUMU_GPU_REPORT=1` logs `rune::gpu_allocation_report()` (wgpu's own
  allocations: under 70 MB reserved), so the blocks were the driver's, growing with frames in
  flight. Tool: a VirtualQueryEx region map, `tests/tools/memmap.ps1 -Name Oumu`.
  (2) `RUNE_OPEN_TRACE=1` appends every Rune mark, with ms since `open_window`, to
  `%TEMP%\rune-open.txt`.
- **(3) FIXED 2026-11-14 20:05: overlays are paced on Windows** (`overlay/draw.rs`
  `pace_until`, cratus-rs patch). Rune paces windows (`window/slot.rs`) but drew overlays on
  every update: up to 678 GPU submits a second during a drag, a queue 100+ frames behind,
  and every new window waited in wgpu's `configure_surface` for it to drain. Now an overlay
  draws at most once per `TIMER_FRAME` (60 fps); a held frame draws at its deadline.
  Results: submits 2,882 -> 438 for the same cases; tool windows open in 5-25 ms (were
  1.8-6.5 s); Oumu memory stays flat (see (1)). Also: unkeyed siblings morph into each other
  in Rune (Oumu's tab lip turned into the icon's box: a cyan block fading out); key them.
  `[profile.fast]` + `CRATUS_PROFILE=fast` for test builds (~12 min, not ~27).
- **(4) FIXED 2026-11-15: overlays come back after a GPU driver reset** (`overlay/surface.rs`
  `regain`, cratus-rs patch). A driver update removed the GPU device under a running Oumu;
  Rune hid each overlay whose surface then refused to configure and waited for the app to
  show it again, which Oumu never does for its tab and button (and the old build crashed:
  "Oumu suggestion: Application Error", no log). Now a Windows overlay that lost its device
  while shown shows itself again on a new one: first try after 300 ms, then once a second
  for 2 min. Test it with `RUNE_TEST_REMOVE_DEVICE_MS=<ms>` (`render/gpu_loss.rs`), which
  removes every device made before that time (`ID3D12Device5::RemoveDevice`), as a driver
  update does; both E2E suites pass with it. Oumu now has the crash logger too
  (`win/crash.rs`, `%TEMP%oumu-crash-<pid>.txt`; `OUMU_CRASH_TEST=1` crashes on purpose).
- History: **Slow tool windows, cause found (2026-11-14 16:35)**, with `RUNE_OPEN_TRACE=1` plus
  `cratus_trace` points in `vendor/rune/vendor/wgpu-hal/src/dx12/mod.rs`: the DX12 part of
  `surface.configure` (queue idle, CreateSwapChainForComposition, DComp) takes 3 ms. The
  1.8-2 s (4-6 s after dozens of menu uses) is before it, in wgpu-core
  `Device::configure_surface`: `self.maintain(PollType::wait_indefinitely())`, "wait for all
  work to finish before configuring the surface", i.e. the fence of the device's last
  submission. The tool window shares the overlays' device (renderer.ready at +6 ms), so it
  waits for whatever the menu/toast overlays still have queued (frames presented to a
  swapchain that was just hidden?). `queue.submit` already runs `maintain(Poll)`, so it is
  not a cleanup backlog. To try next, one at a time, timing with the trace: (a) after the
  menu overlay hides, `queue.submit(empty)` + `device.poll(wait)` on a worker thread, so the
  wait is paid before the drop's window opens; (b) give tool windows their own warmed
  device (`rune::warm_gpu` when the menu shows; a new device's queue is empty);
  (c) find which submission is slow (log the fence wait per submission index). (c) found it:
  see (3).
- Headless test crash (rare, ~1 in 500 runs): a null call during Rune's headless frame loop,
  after the tree is written. Headless runs now install `utsuri/src/crash.rs` (no crash box,
  a stack scan to stderr and `%TEMP%\utsuri-crash-<pid>.txt`), and the test runners kill a
  run after 90 s. The real app logs a crash there too (Windows' box still shows).
  `UTSURI_CRASH_TEST=1` crashes on purpose to check it. Each headless run writes its own
  fixtures folder (a shared one lost windows to locked PDFs).
- The utsuri-e2e harness leaves one `explorer.exe /factory` process per run. To fix.

## Utsuri: status (2026-11-13)
Everything the Python app did is ported: all conversions (incl. HEIC via WIC, AVIF, lossy
WebP, PDF), image tools (Compress/Resize/Crop/Edit/Annotate/Redact/Add BG/Metadata), PDF
tools (Compress/Split/Rotate/Merge/Protect/Unlock/Extract/Metadata/Organize/Stamp/Numbers/
Gray/Flatten/Halves/Chapters/OCR), Word/text, video/audio (ffmpeg), archives, folders,
Pack, Copy, OCR (Tesseract), Translate->Oumu, toasts with Open/Show/Undo, settings, single instance + pipe, right-click/Send To, start at login.

PDF rendering uses Windows.Data.Pdf (no pdfium); it can't open AES-256 files, so protected
files are decrypted to a temp copy first.

## Oumu: status (2026-11-13)
- The original Tauri logic (~11,700 lines: Harper engine, UI Automation reading/fixing,
  host loop, AI features, plagiarism, spot check) runs **unchanged** on `crates/oumu-rt`, a
  small Tauri-shaped runtime (`AppHandle`, `emit`/`emit_to`, managed `State`, window
  handles, `async_runtime`). Oumu depends on it under the name `tauri`.
- The React UI is ported to Rune (`crates/oumu/src/ui`): `Front` (windowless app, model,
  placement tick every 16 ms, tray), the docked panel window with every view (Review,
  Chat, Plagiarism, Tone, Performance, Paraphraser, Humanizer, Reactions, Translate,
  Explain, AI Rewriter, AI Detector, History, Settings, welcome), and the edge tab, Oumu
  button and suggestion card as overlays. Icons are the React SVGs rasterized with resvg.
  Oumu's 11 themes + Matugen feed `style::Tok` (the CSS tokens, derived as color-mix).
- Underlines are a plain Win32 layered window (`ui/strips.rs`, GDI, always click-through).
- Fixes found while testing: WPF text ranges were one character too long after the first
  word; a settings.json with a BOM silently reset every setting; Gemini with an empty base
  URL failed as "busy"; a stream Google cut off mid-answer now gets one retry; a used-up
  Gemini quota says so instead of "busy".
- Matched to the React UI (2026-11-14): AI Rewriter scans by itself when opened and as the
  text changes (offline); Tone runs once typing pauses (12+ words); the score follows every
  edit; "Why?" on a suggestion; "Applied via paste" when the paste fallback was used;
  creating a voice or audience confirms or shows the error; placeholders; multi-line
  writing sample.

## Utsuri: matched to Python (2026-11-14)
Rings match slice for slice. Organize pages got Python's "Select pages: 1-3, 7" box (Enter
selects, never saves) and "Extract selection"; video Compress got its hint.

## Tests
- `cargo test --workspace`: engine (44), Utsuri (10), shared (9), Oumu (138 + 9 shared).
- Headless, no screen use: `tests/utsuri_headless.ps1` (73) and `tests/oumu_headless.ps1`
  (98): every window renders, scripted taps/typing checked in the element tree, and no
  text sticks out of its container anywhere (`tests/overflow.ps1`).
  `oumu --list-windows`, `oumu --window review --snap review.png --scale 2`.
- Real screen (only when the user says go): `cargo run -p utsuri-e2e` (65 cases, real
  drags from a test Explorer window; quit other Utsuris first; report
  `target/e2e/latest.md`), `tests/oumu_e2e.ps1` (real hover and click on Accept, every
  tool; AI cases need a key via OUMU_E2E_KEY), `tests/install_test.ps1` (sandbox install).

## Env switches (tests/screenshots)
UTSURI_SETTINGS, UTSURI_STATE_DIR, UTSURI_FAKE_TRASH, UTSURI_PIPE, UTSURI_LOG,
UTSURI_NO_LAUNCH, UTSURI_NO_WELCOME, UTSURI_E2E (logs toast button rects),
UTSURI_DEMO*,
UTSURI_THEME (headless), `utsuri.exe --cli rings|run|fixtures|selftest`.
RUNE_TEST_REMOVE_DEVICE_MS (a GPU driver reset, on demand), UTSURI_E2E_STDERR / OUMU_E2E_STDERR
(keep the app's stderr in E2E runs),
OUMU_DATA_DIR (keeps tests off the user's settings; headless runs default to a temp
folder), OUMU_E2E (logs the card's Accept point), OUMU_THEME (headless), OUMU_E2E_KEY.

## Disk
`[profile.dev]` has no debug info and no incremental cache: the first build was 27 GB.

## Linux port (2026-11-15, branch `linux-port`)
Target: Arch (CachyOS kernel) + Hyprland 0.57 (Wayland), Ryoku desktop, NVIDIA (Vulkan).
The Windows code is unchanged in behaviour but now sits behind `cfg(windows)`; **the
Windows build was not compiled from Linux** (no Windows target here): build it on Windows
before merging.

- **Build:** Arch's `rust` 1.99.1 matches `rust-toolchain.toml`. `windows`/`winreg`/
  `embed-resource` are `cfg(windows)` deps; `install`, `setup` and the Windows half of
  `utsuri-e2e` compile to stubs elsewhere. Build scripts skip resources off Windows.
- **Engine:** `winrt.rs` → `linux_codec.rs` (same API): PDF pages via poppler `pdftoppm`
  (it opens AES-256 files with `-upw`), HEIC/AVIF decode/encode via libheif's `heif-dec` /
  `heif-enc` (x265, dav1d/aom on this box). `fonts.rs`: Windows font names map to
  fontconfig (Noto Sans / DejaVu, Vietnamese OK). `proc.rs`: `Quiet::quiet()`/`detached()`
  replace `creation_flags`. Free space via `statvfs`. XDG paths: settings
  `~/.config/Utsuri-rs`, log/state `~/.local/state/Utsuri-rs` (`shared::paths`).
- **Utsuri trigger:** evdev (`linux/keys.rs`, one reader thread per keyboard/mouse,
  hot-plug rescan every 2 s, ydotool's virtual device included) + pointer from Hyprland IPC
  (`cursorpos`, asked only while the button is down). Needs the `input` group; without it
  Utsuri shows a toast saying so. **The Ryoku Utsuri plugin also answers Shift+drag**
  (user.lua UTSURI-TRIGGER): both menus open. Tests use Ctrl.
- **Drops on the menu (Wayland):** Rune's layer-shell overlays don't do DnD, so
  `linux/dnd.rs` maps an invisible layer surface (own Wayland connection, `wl_data_device`)
  over the menu while it shows in drag mode. Files arrive on enter (rings follow) and on
  drop. Race found in E2E: the button-up (evdev) beat the drop's data, and the 120 ms grace
  hid the catcher first; now events are handled before hide commands, reads run on their
  own thread, and `Drag::Dropping` holds the menu up to 3 s for the files.
- **Rune patch:** `OverlayPlacement::Rect` layer surfaces set exclusive zone -1. With 0,
  Hyprland measured the margins from inside the bar's reserved zone: every placed overlay
  (Oumu underlines, Utsuri menu) sat 39 px low. `RUNE_OVERLAY_FPS=1` prints overlay
  draws/s (measuring aid for the Windows-only pacing).
- **Hyprland 0.57 here only takes Lua dispatches** (`hl.dsp.window.move({...})`); the
  legacy `dispatch movewindowpixel ...` form is refused.
- **Oumu:** the original Tauri Oumu's `src-tauri/src/linux/` (AT-SPI, Hyprland, PRIMARY
  selection, socket) copied in minus `layer.rs` (GTK). Windows go through `wm::plain`; the
  panel is a normal Rune window floated, pinned and moved by address through Hyprland
  (`ui::place_panel_hyprland`, user drags reported back); tab/button/card/underlines are
  Rune layer-shell overlays (`ui/strips_rune.rs` replaces the GDI strips). Log:
  `~/.local/state/oumu/oumu.log`; autostart: XDG `~/.config/autostart`. `OUMU_SOCK` moves
  the socket for tests. At launch the a11y bus is switched back on when consent was saved.
- **Tests (Linux):** `cargo test --workspace`; `python3 tests/linux/headless.py`
  (Utsuri 67, Oumu 88; checks are case-insensitive like PowerShell's `-like`);
  `cargo run -p utsuri-e2e` (Linux driver: GTK drag source + ydotool, workspace 9, Ctrl);
  `python3 tests/linux/oumu_e2e.py [--no-ai]` (GTK3 editor, ydotool hover/click, AT-SPI
  panel buttons, Chromium PRIMARY highlight via a real triple-click: script-made
  selections don't set PRIMARY on Wayland).
- **Install:** `packaging/linux/install.sh` (~/.local/bin, icons, app entries, Dolphin
  service menu); `uninstall.sh`.

## In-app updates (2026-11-15)
- Both apps check github.com/CratusGod/<app> `releases/latest` (shared/src/update.rs, through
  the system curl: no TLS crate added). A release offers an update when its tag `v<x.y.z>`
  is newer than the running version and it carries `<App>-Setup.exe`. The download must match
  the asset's SHA-256 (GitHub's `digest`, or `<App>-Setup.exe.sha256`), else it is refused.
- Installing: `install::start_update` finds this copy's Apps entry (its folder, a test
  suffix, whether it starts at sign-in) and runs the setup with `--silent --launch --dir ..
  --wait-pid <app> [--no-autostart]`; the app quits, the setup replaces the exe and starts it.
  A copy run from a build folder (no Apps entry) never checks by itself and can't update.
- When: 30 s after start, then every 12 h, if Settings → "Check for updates automatically"
  (default on); "Check for updates" in Settings and the tray any time. Utsuri shows a card
  (Update / Later); Oumu a banner at the top of the panel, plus a status row in Settings.
- Releasing: bump `crates/<app>/Cargo.toml` and `crates/install` (`AppInfo.version`; a unit
  test checks they match), then `packaging\release.ps1 -Publish`. It builds with
  `--remap-path-prefix` (dependency paths in panic strings carried `C:\Users\<name>\.cargo`),
  refuses exes that still contain the home folder or user name, and makes the release.
  The same RUSTFLAGS must be set for every cargo run on target\release, or cargo rebuilds all.
- Tests: `shared` unit tests (versions, picking the asset, checksum refusal, file:// feeds);
  headless update card / banner / Settings; `tests/install_test.ps1` runs a real update in the
  sandbox install (a file:// release list offering v99.0.0 and this setup).
- Env: <APP>_UPDATE_FEED (release list URL; file:// works), <APP>_FAKE_VERSION,
  <APP>_UPDATE_AFTER_MS (first automatic look), <APP>_UPDATE_YES=1 (install without asking).
  The setup strips these from the app it restarts, so a test can't loop.
