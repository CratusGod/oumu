# cratus-rs

Two small Windows apps, rewritten in Rust on the [Rune](https://github.com/anomalyco/rune) UI toolkit:

- **Utsuri**: drag a file and tap **Shift** to get a radial menu of actions for it: convert
  images and documents, compress, resize, crop, redact, annotate, PDF tools (split, merge,
  protect, unlock, organize pages, OCR), video and audio conversions, archives, and more.
  Drop the file on a slice and it's done, with Open / Show in folder / Undo.
- **Oumu**: a writing assistant that checks spelling and grammar in the app you're typing in
  (Notepad, browsers, Discord, Office...), underlines mistakes and fixes them in place. Optional
  AI tools (paraphrase, tone, translate, humanize, AI detector, chat) use your own AI key.

| Utsuri menu | Utsuri tool window | Oumu |
|---|---|---|
| ![Utsuri radial menu](docs/screenshots/utsuri-menu.png) | ![Utsuri compress window](docs/screenshots/utsuri-compress.png) | ![Oumu review panel](docs/screenshots/oumu-review.png) |

Windows 10/11 (x64) only.

## Install

Build the installers from source (there are no prebuilt downloads yet):

1. Install [Rust](https://rustup.rs) and the
   [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the
   "Desktop development with C++" workload. The right Rust version is picked up from
   `rust-toolchain.toml`.
2. Build (takes a while the first time):
   ```bash
   cargo build --release -p utsuri -p oumu
   cargo build --release -p setup
   ```
3. Run `target\release\utsuri-setup.exe` and/or `target\release\oumu-setup.exe`. They install per
   user (no admin), add a Start menu entry, start with Windows, and can be removed from
   Settings > Apps.

Optional, for Utsuri's video/audio and OCR actions:

```bash
winget install Gyan.FFmpeg
```

```bash
winget install UB-Mannheim.TesseractOCR
```

## Privacy

- Neither app has accounts, telemetry or update checks.
- Utsuri works on your files locally; originals are never modified (results are written next to
  them, and Undo moves a result to the Recycle Bin).
- Oumu's spelling and grammar checks run on your PC ([Harper](https://writewithharper.com)).
  Text leaves your PC only when you use an AI tool (it goes to the AI provider you set up, with
  your key) or the plagiarism check (short quotes go to a web search). Your AI key is stored in
  `%APPDATA%\dev.cratusgod.oumu\settings.json`.

## Development

```bash
cargo test --workspace
cargo run -p utsuri
cargo run -p oumu
```

- `cargo build --profile fast` is an optimized build for testing in about half the time of
  `--release`; set `CRATUS_PROFILE=fast` to point the test scripts at it.
- Headless UI tests (no window, mouse or keyboard used): `tests\utsuri_headless.ps1`,
  `tests\oumu_headless.ps1`.
- Real-screen tests (they move the mouse and type, so don't use the PC meanwhile):
  `cargo run -p utsuri-e2e`, `tests\oumu_e2e.ps1`, `tests\install_test.ps1`.
- Design notes, Rune patches and lessons learned: [docs/NOTES.md](docs/NOTES.md).

`vendor/rune` is Rune at rev `c369cc0` with a few patches (search for "cratus-rs"), and
`vendor/winit` is Rune's winit with one fix. Their licenses are in those folders.

## License

Utsuri and the shared crates are [MIT](LICENSE). Oumu (`crates/oumu`, `crates/oumu-rt`) is
[AGPL-3.0-or-later](crates/oumu/LICENSE), like the original Oumu.
