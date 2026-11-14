import { useEffect, useRef, useState } from "react";
import { ipc, on } from "../lib/ipc";
import { ruleName, wordDiff } from "../lib/text";
import type { Alert, Geometry } from "../lib/types";

const CARD_W = 300;
/** The card's usual height: room for it below the word, else it goes above. */
const CARD_H = 220;
/** Dismissals of one rule before the card offers to turn it off. */
const MUTE_OFFER_AFTER = 2;

export function Underlines() {
  const [geo, setGeo] = useState<Geometry>({ origin_x: 0, origin_y: 0, width: 0, height: 0, rects: [], hovered: null, over_card: false });
  const [alerts, setAlerts] = useState<Record<string, Alert>>({});
  const [open, setOpen] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const closeTimer = useRef<number | null>(null);
  const cardRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    ipc.getState().then((s) => setAlerts(Object.fromEntries(s.alerts.map((a) => [a.id, a])))).catch(() => {});
    const u1 = on<Geometry>("host://geometry", setGeo);
    const u2 = on<Alert[]>("host://alerts", (a) => setAlerts(Object.fromEntries(a.map((x) => [x.id, x]))));
    return () => { u1.then((f) => f()); u2.then((f) => f()); };
  }, []);

  // The host hit-tests the pointer against the underlines and the reported card
  // rect every tick. This window ignores mouse events whenever the pointer is
  // over neither, so DOM mouseleave is not reliable here: open and close follow
  // the host's signal only. A short grace covers the poll interval while the
  // pointer travels from the underline into the card.
  useEffect(() => {
    const inside = !!geo.hovered || geo.over_card;
    if (inside) {
      if (closeTimer.current) { window.clearTimeout(closeTimer.current); closeTimer.current = null; }
      if (geo.hovered && geo.hovered !== open) setOpen(geo.hovered);
    } else if (open && !closeTimer.current) {
      closeTimer.current = window.setTimeout(() => { closeTimer.current = null; setOpen(null); }, 400);
    }
  }, [geo.hovered, geo.over_card, open]);

  // Close when the open alert or its underline is gone (accepted, dismissed,
  // re-linted away, scrolled out of view). A card with nothing to anchor to
  // must close, or its stale rect keeps the host treating the pointer as over it.
  useEffect(() => {
    if (open && (!geo.rects.some((r) => r.id === open) || !alerts[open])) setOpen(null);
  }, [geo.rects, alerts, open]);
  useEffect(() => setError(null), [open]);

  // Report the card rect plus a bridge from the underline to the card, so the host
  // keeps this window live (and "over_card" true) while the pointer crosses the gap.
  useEffect(() => {
    const el = cardRef.current;
    if (!open || !el) { ipc.setCardRects([]).catch(() => {}); return; }
    const r = el.getBoundingClientRect();
    const rects = [{ x: r.left, y: r.top, w: r.width, h: r.height }];
    const a = geo.rects.find((x) => x.id === open);
    if (a) {
      const top = Math.min(a.y, r.top);
      const bottom = Math.max(a.y + a.h, r.top + r.height);
      rects.push({ x: Math.min(a.x, r.left), y: top, w: Math.max(a.w, 48), h: bottom - top });
    }
    ipc.setCardRects(rects).catch(() => {});
  }, [open, alerts, geo.width, geo.height, geo.rects]);

  const act = (call: Promise<unknown>) => {
    call.then(() => setOpen(null)).catch((e) => setError(String(e)));
  };

  const alert = open ? alerts[open] : undefined;
  const anchor = open ? geo.rects.find((r) => r.id === open) : undefined;
  let cardStyle: React.CSSProperties = {};
  if (anchor) {
    const left = Math.max(4, Math.min(anchor.x, geo.width - CARD_W - 4));
    // The card is about 210 px tall; the host makes this layer tall enough for it
    // (CARD_ROOM in host.rs), below a short field, or above it near the screen's end.
    const below = anchor.y + anchor.h + 8;
    const top = below + CARD_H < geo.height ? below : Math.max(4, anchor.y - CARD_H - 8);
    cardStyle = { left, top };
  }

  return (
    <div className="ul-layer">
      {[...geo.rects.filter((r) => r.sentence), ...geo.rects.filter((r) => !r.sentence)].map((r, i) => (
        <div key={r.id + i} className={"ul" + (r.sentence ? " sentence" : "")} style={{ left: r.x, top: r.y + r.h - (r.sentence ? 1 : 2), width: r.w, background: `var(--${r.category}, ${r.color})`, opacity: open === r.id ? 1 : r.sentence ? 0.55 : 0.9 }} />
      ))}
      {alert && anchor && (
        <div ref={cardRef} className="ul-card" style={cardStyle}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, color: "var(--text-subdued)", fontSize: 13 }}>
            <i style={{ width: 10, height: 10, borderRadius: 5, background: `var(--${anchor.category}, ${anchor.color})`, display: "inline-block" }} />
            {cap(alert.category)} · {alert.title}
          </div>
          <div style={{ fontSize: 17, margin: "10px 0 12px", maxHeight: 220, overflowY: "auto" }}>
            {alert.suggestions.length === 0 ? (
              <span style={{ color: "var(--text-base)" }}>{alert.original}</span>
            ) : (
              alert.suggestions[0] !== "" && alert.original.split(/\s+/).length > 6 ? (
                <span className="diff">
                  {wordDiff(alert.original, alert.suggestions[0]!).map((op, i) => op.kind === "same" ? <span key={i}>{op.text}</span> : op.kind === "del" ? <del key={i}>{op.text}</del> : <ins key={i}>{op.text}</ins>)}
                </span>
              ) : (
                <>
                  <del style={{ color: "var(--text-muted)", marginRight: 6 }}>{alert.original}</del>
                  <ins style={{ textDecoration: "none", fontWeight: 600 }}>{alert.suggestions[0] === "" ? "Remove" : alert.suggestions[0]}</ins>
                </>
              )
            )}
          </div>
          <div style={{ fontSize: 13, color: "var(--text-subdued)", marginBottom: 12 }}>{alert.message}</div>
          <div style={{ display: "flex", gap: 12, alignItems: "center", flexWrap: "wrap" }}>
            {alert.suggestions.length > 0 && (
              <button className="btn btn--primary" style={{ padding: "8px 14px" }} onClick={() => act(ipc.acceptAlert(alert.id, 0))}>Accept</button>
            )}
            <button className="btn btn--ghost" onClick={() => act(ipc.dismissAlert(alert.id))}>Dismiss</button>
            {alert.kind === "Spelling" && (
              <button className="btn btn--ghost" onClick={() => act(ipc.learnWord(alert.id))}>Add to dictionary</button>
            )}
          </div>
          {alert.rule && alert.rule_dismissals >= MUTE_OFFER_AFTER && (
            <div style={{ fontSize: 12, color: "var(--text-subdued)", marginTop: 10 }}>
              You dismissed “{ruleName(alert.rule)}” {alert.rule_dismissals} times.{" "}
              <button className="btn btn--ghost" style={{ padding: "2px 6px", fontSize: 12 }} onClick={() => act(ipc.muteRule(alert.id))}>Turn it off</button>
            </div>
          )}
          {error && <div style={{ fontSize: 12, color: "var(--correctness)", marginTop: 8 }}>{error}</div>}
        </div>
      )}
    </div>
  );
}

function cap(s: string) { return s.charAt(0).toUpperCase() + s.slice(1); }
