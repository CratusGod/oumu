import { useEffect, useState } from "react";
import { ipc, on } from "../lib/ipc";
import type { Alert } from "../lib/types";
import { OumuMark } from "../components/icons";

/** The edge tab: a sliver at the screen edge while idle; pointing at it slides
 *  the tab out, clicking opens the panel (Esc in the panel tucks it back). */
export function Tab() {
  const [count, setCount] = useState(0);
  const [side, setSide] = useState<"left" | "right">("left");
  const [open, setOpen] = useState(false);
  useEffect(() => {
    ipc.getState().then((s) => {
      setCount(s.alerts.length);
      setSide(s.settings.dock_side === "right" ? "right" : "left");
    }).catch(() => {});
    const u1 = on<Alert[]>("host://alerts", (a) => setCount(a.length));
    const u2 = on<{ open: boolean }>("tab://open", (p) => setOpen(p.open));
    return () => { u1.then((f) => f()); u2.then((f) => f()); };
  }, []);
  // The host slides the tab out while the pointer is on it and says so on
  // tab://open. Where the desktop hides the pointer, the tab's own hover does.
  const hover = (next: boolean) => { if (next !== open) { setOpen(next); ipc.tabHover(next).catch(() => {}); } };
  return (
    <div className={"tabwin side-" + side} onMouseEnter={() => hover(true)} onMouseLeave={() => hover(false)}>
      <div className={"edgetab" + (open ? " open" : "") + (count > 0 ? " has-alerts" : "")} onClick={() => ipc.openPanel("review", true)} title="Open Oumu">
        <i className="edgetab__lip" />
        <OumuMark style={{ color: "var(--brand)" }} />
        {count > 0 && <span className="badge">{count > 99 ? "99+" : count}</span>}
      </div>
    </div>
  );
}
