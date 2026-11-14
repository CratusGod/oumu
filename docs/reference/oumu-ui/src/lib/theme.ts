// Themes: each sets the base colours of styles/tokens.css; every other colour
// is derived there. "matugen" maps a Material palette (Ryoku writes one to
// ~/.cache/ryoku/colors.json on every wallpaper change) onto the same bases.

export type Base = {
  "bg-base": string; "bg-elevated": string; "bg-subdued": string; "bg-surface": string;
  "border-base": string; "border-subdued": string;
  "text-base": string; "text-subdued": string; "text-muted": string; "icon-base": string;
  brand: string; "brand-strong": string; "on-brand": string;
  correctness: string; clarity: string; engagement: string; delivery: string; plagiarism: string;
  warning: string; link: string; accent: string;
};
export type Theme = { id: string; name: string; light?: boolean; base: Base };

export const MATUGEN = "matugen";

export const THEMES: Theme[] = [
  { id: "default", name: "Default (dark)", base: {
    "bg-base": "#1c1c1c", "bg-elevated": "#242424", "bg-subdued": "#2e2e2e", "bg-surface": "#1f1f1f",
    "border-base": "#545454", "border-subdued": "#3a3a3a", "text-base": "#ffffff", "text-subdued": "#bcbcbc", "text-muted": "#8a8a8a", "icon-base": "#d9d9d9",
    brand: "#2cc9b6", "brand-strong": "#15c39a", "on-brand": "#0e1f1c",
    correctness: "#ff7a74", clarity: "#7d99f0", engagement: "#2cc9b6", delivery: "#a598f0", plagiarism: "#80f3f3", warning: "#ffbf47", link: "#7ab8ff", accent: "#6b5bd6",
  } },
  { id: "catppuccin-mocha", name: "Catppuccin Mocha", base: {
    "bg-base": "#1e1e2e", "bg-elevated": "#181825", "bg-subdued": "#313244", "bg-surface": "#181825",
    "border-base": "#585b70", "border-subdued": "#45475a", "text-base": "#cdd6f4", "text-subdued": "#bac2de", "text-muted": "#7f849c", "icon-base": "#bac2de",
    brand: "#cba6f7", "brand-strong": "#cba6f7", "on-brand": "#11111b",
    correctness: "#f38ba8", clarity: "#89b4fa", engagement: "#94e2d5", delivery: "#b4befe", plagiarism: "#89dceb", warning: "#f9e2af", link: "#89b4fa", accent: "#fab387",
  } },
  { id: "catppuccin-latte", name: "Catppuccin Latte", light: true, base: {
    "bg-base": "#eff1f5", "bg-elevated": "#e6e9ef", "bg-subdued": "#ccd0da", "bg-surface": "#e6e9ef",
    "border-base": "#9ca0b0", "border-subdued": "#bcc0cc", "text-base": "#4c4f69", "text-subdued": "#5c5f77", "text-muted": "#8c8fa1", "icon-base": "#5c5f77",
    brand: "#8839ef", "brand-strong": "#8839ef", "on-brand": "#eff1f5",
    correctness: "#d20f39", clarity: "#1e66f5", engagement: "#179299", delivery: "#7287fd", plagiarism: "#04a5e5", warning: "#df8e1d", link: "#1e66f5", accent: "#fe640b",
  } },
  { id: "nord", name: "Nord", base: {
    "bg-base": "#2e3440", "bg-elevated": "#3b4252", "bg-subdued": "#434c5e", "bg-surface": "#3b4252",
    "border-base": "#4c566a", "border-subdued": "#434c5e", "text-base": "#eceff4", "text-subdued": "#d8dee9", "text-muted": "#7b88a1", "icon-base": "#d8dee9",
    brand: "#88c0d0", "brand-strong": "#88c0d0", "on-brand": "#2e3440",
    correctness: "#bf616a", clarity: "#81a1c1", engagement: "#a3be8c", delivery: "#b48ead", plagiarism: "#8fbcbb", warning: "#ebcb8b", link: "#88c0d0", accent: "#5e81ac",
  } },
  { id: "gruvbox-dark", name: "Gruvbox Dark", base: {
    "bg-base": "#282828", "bg-elevated": "#32302f", "bg-subdued": "#3c3836", "bg-surface": "#1d2021",
    "border-base": "#665c54", "border-subdued": "#504945", "text-base": "#ebdbb2", "text-subdued": "#d5c4a1", "text-muted": "#928374", "icon-base": "#d5c4a1",
    brand: "#fabd2f", "brand-strong": "#fabd2f", "on-brand": "#282828",
    correctness: "#fb4934", clarity: "#83a598", engagement: "#b8bb26", delivery: "#d3869b", plagiarism: "#8ec07c", warning: "#fe8019", link: "#83a598", accent: "#d65d0e",
  } },
  { id: "tokyo-night", name: "Tokyo Night", base: {
    "bg-base": "#1a1b26", "bg-elevated": "#1f2335", "bg-subdued": "#292e42", "bg-surface": "#16161e",
    "border-base": "#414868", "border-subdued": "#292e42", "text-base": "#c0caf5", "text-subdued": "#a9b1d6", "text-muted": "#565f89", "icon-base": "#a9b1d6",
    brand: "#7aa2f7", "brand-strong": "#7aa2f7", "on-brand": "#1a1b26",
    correctness: "#f7768e", clarity: "#7dcfff", engagement: "#9ece6a", delivery: "#bb9af7", plagiarism: "#2ac3de", warning: "#e0af68", link: "#7aa2f7", accent: "#ff9e64",
  } },
  { id: "rose-pine", name: "Rosé Pine", base: {
    "bg-base": "#191724", "bg-elevated": "#1f1d2e", "bg-subdued": "#26233a", "bg-surface": "#1f1d2e",
    "border-base": "#524f67", "border-subdued": "#403d52", "text-base": "#e0def4", "text-subdued": "#908caa", "text-muted": "#6e6a86", "icon-base": "#908caa",
    brand: "#ebbcba", "brand-strong": "#ebbcba", "on-brand": "#191724",
    correctness: "#eb6f92", clarity: "#9ccfd8", engagement: "#31748f", delivery: "#c4a7e7", plagiarism: "#9ccfd8", warning: "#f6c177", link: "#c4a7e7", accent: "#f6c177",
  } },
  { id: "purple", name: "Purple", base: {
    "bg-base": "#17141f", "bg-elevated": "#1f1a2b", "bg-subdued": "#2a2338", "bg-surface": "#1d1828",
    "border-base": "#4a3f63", "border-subdued": "#332b45", "text-base": "#f3efff", "text-subdued": "#c6bde0", "text-muted": "#8d84a8", "icon-base": "#d6cff0",
    brand: "#a78bfa", "brand-strong": "#8b5cf6", "on-brand": "#ffffff",
    correctness: "#fb7185", clarity: "#818cf8", engagement: "#c084fc", delivery: "#e879f9", plagiarism: "#67e8f9", warning: "#fbbf24", link: "#a5b4fc", accent: "#8b5cf6",
  } },
  { id: "black", name: "Black", base: {
    "bg-base": "#000000", "bg-elevated": "#0a0a0a", "bg-subdued": "#161616", "bg-surface": "#0d0d0d",
    "border-base": "#3a3a3a", "border-subdued": "#222222", "text-base": "#ffffff", "text-subdued": "#b3b3b3", "text-muted": "#7a7a7a", "icon-base": "#d0d0d0",
    brand: "#ffffff", "brand-strong": "#e6e6e6", "on-brand": "#000000",
    correctness: "#ff6b6b", clarity: "#74a7ff", engagement: "#5eead4", delivery: "#c4b5fd", plagiarism: "#7dd3fc", warning: "#fcd34d", link: "#93c5fd", accent: "#525252",
  } },
  { id: "white", name: "White", light: true, base: {
    "bg-base": "#ffffff", "bg-elevated": "#f7f7f7", "bg-subdued": "#eeeeee", "bg-surface": "#ffffff",
    "border-base": "#c8c8c8", "border-subdued": "#e2e2e2", "text-base": "#111111", "text-subdued": "#4d4d4d", "text-muted": "#7a7a7a", "icon-base": "#333333",
    brand: "#111111", "brand-strong": "#000000", "on-brand": "#ffffff",
    correctness: "#d13438", clarity: "#2f5fd0", engagement: "#0b8a6f", delivery: "#7a4fd6", plagiarism: "#0e7490", warning: "#b7791f", link: "#2563eb", accent: "#6b5bd6",
  } },
  { id: "orange", name: "Orange", base: {
    "bg-base": "#1a1410", "bg-elevated": "#231b15", "bg-subdued": "#2e241c", "bg-surface": "#201914",
    "border-base": "#5a4636", "border-subdued": "#3a2e24", "text-base": "#fff4ea", "text-subdued": "#d6c3b2", "text-muted": "#9c8676", "icon-base": "#e8d6c6",
    brand: "#ff8a3d", "brand-strong": "#f97316", "on-brand": "#1a0d03",
    correctness: "#f87171", clarity: "#60a5fa", engagement: "#fb923c", delivery: "#c084fc", plagiarism: "#5eead4", warning: "#facc15", link: "#fdba74", accent: "#ea580c",
  } },
];

/** A Material palette, as matugen and Ryoku write it (role name -> "#rrggbb"). */
export type Palette = Record<string, string>;

/** Relative luminance of "#rrggbb" (0 dark .. 1 light). */
export function luminance(hex: string): number {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return 0;
  const n = parseInt(m[1], 16);
  const ch = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((c) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
}

/** Map a Material palette onto the bases. Missing roles fall back to the default theme. */
export function fromPalette(p: Palette): Theme {
  const d = THEMES[0].base;
  const pick = (...keys: string[]) => keys.map((k) => p[k]).find((v) => typeof v === "string" && /^#[0-9a-f]{6}$/i.test(v));
  const bg = pick("surfaceContainerLow", "surface", "background") ?? d["bg-base"];
  const base: Base = {
    "bg-base": bg,
    "bg-elevated": pick("surfaceContainer", "surface") ?? d["bg-elevated"],
    "bg-subdued": pick("surfaceContainerHigh", "surfaceVariant") ?? d["bg-subdued"],
    "bg-surface": pick("surfaceContainer", "surface") ?? d["bg-surface"],
    "border-base": pick("outline") ?? d["border-base"],
    "border-subdued": pick("outlineVariant") ?? d["border-subdued"],
    "text-base": pick("onSurface", "foreground") ?? d["text-base"],
    "text-subdued": pick("onSurfaceVariant") ?? d["text-subdued"],
    "text-muted": pick("outline") ?? d["text-muted"],
    "icon-base": pick("onSurfaceVariant") ?? d["icon-base"],
    brand: pick("primary") ?? d.brand,
    "brand-strong": pick("primary") ?? d["brand-strong"],
    "on-brand": pick("onPrimary") ?? d["on-brand"],
    // Categories stay apart in colour: the terminal colours a palette carries, else Material roles.
    correctness: pick("color1", "error") ?? d.correctness,
    clarity: pick("color4", "tertiary") ?? d.clarity,
    engagement: pick("primary") ?? d.engagement,
    delivery: pick("color5", "secondary") ?? d.delivery,
    plagiarism: pick("color6", "tertiary") ?? d.plagiarism,
    warning: pick("color3", "tertiary") ?? d.warning,
    link: pick("primary") ?? d.link,
    accent: pick("tertiary", "secondary") ?? d.accent,
  };
  return { id: MATUGEN, name: "Matugen", light: luminance(bg) > 0.5, base };
}

export function themeById(id: string): Theme {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

/** Write a theme's bases onto :root. */
export function applyTheme(t: Theme, root: HTMLElement = document.documentElement): void {
  for (const [k, v] of Object.entries(t.base)) root.style.setProperty(`--${k}`, v);
  root.dataset.mode = t.light ? "light" : "dark";
}
