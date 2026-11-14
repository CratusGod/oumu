import { describe, expect, it } from "vitest";
import { THEMES, fromPalette, luminance, themeById } from "./theme";

describe("themes", () => {
  it("every theme sets every base colour as #rrggbb", () => {
    const keys = Object.keys(THEMES[0].base).sort();
    for (const t of THEMES) {
      expect(Object.keys(t.base).sort()).toEqual(keys);
      for (const v of Object.values(t.base)) expect(v).toMatch(/^#[0-9a-f]{6}$/i);
    }
  });

  it("ids are unique and an unknown id falls back to the default", () => {
    expect(new Set(THEMES.map((t) => t.id)).size).toBe(THEMES.length);
    expect(themeById("nope").id).toBe("default");
  });

  it("light themes are marked light", () => {
    for (const t of THEMES) expect(!!t.light).toBe(luminance(t.base["bg-base"]) > 0.5);
  });
});

describe("matugen palette", () => {
  const ryoku = {
    surfaceContainerLow: "#201a19", surfaceContainer: "#251e1d", surfaceContainerHigh: "#2f2827",
    outline: "#a08c89", outlineVariant: "#534341", onSurface: "#ede0dd", onSurfaceVariant: "#d8c2be",
    primary: "#ffb4a8", onPrimary: "#561e16", tertiary: "#d8c58d", secondary: "#e7bdb6",
    color1: "#ffb4ab", color3: "#e8c26c", color4: "#9ccbfb", color5: "#e0b6f0", color6: "#99d2c2",
  };

  it("maps Material roles onto the bases", () => {
    const t = fromPalette(ryoku);
    expect(t.base["bg-base"]).toBe("#201a19");
    expect(t.base["text-base"]).toBe("#ede0dd");
    expect(t.base.brand).toBe("#ffb4a8");
    expect(t.base["on-brand"]).toBe("#561e16");
    expect(t.base.correctness).toBe("#ffb4ab");
    expect(t.base.clarity).toBe("#9ccbfb");
    expect(t.light).toBe(false);
  });

  it("missing or malformed roles fall back to the default theme", () => {
    const t = fromPalette({ primary: "not-a-colour" });
    expect(t.base.brand).toBe(THEMES[0].base.brand);
    expect(t.base["bg-base"]).toBe(THEMES[0].base["bg-base"]);
  });

  it("a light wallpaper palette gives a light theme", () => {
    expect(fromPalette({ surfaceContainerLow: "#fff8f6", onSurface: "#231918" }).light).toBe(true);
  });
});
