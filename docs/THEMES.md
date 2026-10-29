# Themes

Open **Settings → Appearance → Theme**. The change applies at once to the panel, the edge tab, the
Oumu button and the underline colours.

| Theme | Look |
|---|---|
| Default | Dark grey, teal accent |
| Catppuccin Mocha / Latte | Pastel dark / light |
| Nord | Cool blue-grey |
| Gruvbox Dark | Warm retro |
| Tokyo Night | Deep blue, neon accents |
| Rosé Pine | Muted purple, rose accent |
| Purple, Black, White, Orange | Plain single-accent themes (Black suits OLED screens) |
| **Matugen** | Your wallpaper's colours, updated live |

## Matugen: colours from your wallpaper

Choose **Matugen** in the theme list. Oumu then shows where the set-up stands, with a checkmark for
each step it finds done.

### On Ryoku

Nothing to set up. Ryoku already writes its wallpaper palette to `~/.cache/ryoku/colors.json`, and
Oumu reads it. Change the wallpaper and Oumu's colours follow within 2 seconds. Ryoku's own matugen
config is never touched.

### On any other Arch setup (Hyprland, Sway, GNOME, KDE, …)

1. Install matugen:
   ```bash
   sudo pacman -S matugen
   ```
2. Click **Set up matugen** under the theme list. It does two things:
   - copies Oumu's template to `~/.config/matugen/templates/oumu-colors.json`
   - adds a `[templates.oumu]` section to `~/.config/matugen/config.toml`. Your config is backed up
     first to `config.toml.before-oumu`. The section is only appended, so the rest of your config
     stays as it was.
3. Run matugen once on your wallpaper:
   ```bash
   matugen image /path/to/your/wallpaper.jpg
   ```
   Or pick a colour instead: `matugen color hex "#6750a4"`.

From then on, every matugen run (including your wallpaper tool's) rewrites
`~/.config/oumu/colors.json`, and Oumu picks it up within 2 seconds, with no restart.

### By hand, or a palette file of your own

The set-up button only does what's described here:

```toml
# ~/.config/matugen/config.toml
[templates.oumu]
input_path = "~/.config/matugen/templates/oumu-colors.json"   # packaging/matugen/colors.json
output_path = "~/.config/oumu/colors.json"
```

To use any other JSON file of Material colour roles (`"primary": "#ffb4a8"`, …), enter its path
under **Palette file**. Oumu reads `surfaceContainerLow`, `surfaceContainer`,
`surfaceContainerHigh`, `outline`, `outlineVariant`, `onSurface`, `onSurfaceVariant`, `primary`,
`onPrimary`, `secondary`, `tertiary` and `error`. It also reads `color1`–`color6`, if present, for
the suggestion categories. Roles it can't find fall back to the Default theme.

### Undo

Delete the `[templates.oumu]` section, or restore the backup:

```bash
mv ~/.config/matugen/config.toml.before-oumu ~/.config/matugen/config.toml
```

## Troubleshooting

- **Colours didn't change after a wallpaper change.** Check that the palette file shown under the
  theme list exists and was just modified. On Ryoku that's `~/.cache/ryoku/colors.json`.
- **"matugen is not installed".** Run `sudo pacman -S matugen`, then click the button again.
