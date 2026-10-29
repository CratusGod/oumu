import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/app.css";
import { Panel } from "./windows/Panel";
import { OumuButton } from "./windows/OumuButton";
import { Underlines } from "./windows/Underlines";
import { Tab } from "./windows/Tab";

import { getCurrentWindow } from "@tauri-apps/api/window";
import { ipc, on } from "./lib/ipc";
import { MATUGEN, applyTheme, fromPalette, themeById, type Palette } from "./lib/theme";
import type { Settings } from "./lib/types";

let win = "panel";
try { win = getCurrentWindow().label; } catch { win = new URLSearchParams(window.location.search).get("win") ?? "panel"; }
const Root = win === "oumubutton" ? OumuButton : win === "underlines" ? Underlines : win === "tab" ? Tab : Panel;
document.documentElement.dataset.win = win;
// WebKitGTK (Linux) reports "Linux" in its user agent; the macOS webview reports "Macintosh".
document.documentElement.classList.toggle("linux", /Linux/.test(navigator.userAgent) && !/Android/.test(navigator.userAgent));
if (win !== "panel") document.body.style.background = "transparent";

import { invoke } from "@tauri-apps/api/core";
window.addEventListener("error", (e) => { invoke("ui_log", { message: `${win}: ${e.message} @ ${e.filename}:${e.lineno}` }).catch(() => {}); });
window.addEventListener("unhandledrejection", (e) => { invoke("ui_log", { message: `${win}: unhandled ${String((e as PromiseRejectionEvent).reason)}` }).catch(() => {}); });
invoke("ui_ready", { label: win }).catch(() => {});

// Every window follows the theme: the chosen one, or the matugen palette as it changes.
(async () => {
  let theme = "default";
  let palette: Palette | null = null;
  const apply = () => applyTheme(theme === MATUGEN && palette ? fromPalette(palette) : themeById(theme));
  try {
    const s = await ipc.getSettings();
    theme = s.theme || "default";
    if (theme === MATUGEN) palette = await ipc.getPalette();
  } catch { /* outside the app (mock): default theme */ }
  apply();
  on<Settings>("settings://changed", async (s) => {
    theme = s.theme || "default";
    if (theme === MATUGEN && !palette) palette = await ipc.getPalette().catch(() => null);
    apply();
  });
  on<Palette>("theme://palette", (p) => { palette = p; apply(); });
})();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
