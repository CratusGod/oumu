import { useCallback, useEffect, useState } from "react";
import type { Ctx } from "../Panel";
import { ipc } from "../../lib/ipc";
import type { MatugenStatus } from "../../lib/types";

/** First-time set-up for the Matugen theme: where it stands, one button, the one command left. */
export function MatugenSetup({ ctx }: { ctx: Ctx }) {
  const [st, setSt] = useState<MatugenStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(() => { ipc.matugenStatus().then(setSt).catch((e) => setError(String(e))); }, []);
  // Re-check while this is open: the palette appears once matugen has run.
  useEffect(() => { refresh(); const t = window.setInterval(refresh, 3000); return () => window.clearInterval(t); }, [refresh, ctx.settings.palette_path]);
  if (!st) return null;

  const setup = async () => {
    setBusy(true); setError(null);
    try { setSt(await ipc.matugenSetup()); ctx.toast("matugen set up"); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const copy = (text: string) => navigator.clipboard.writeText(text).then(() => ctx.toast("Copied"));

  if (st.ryoku) {
    return <div className="notice notice--ok">Following Ryoku's wallpaper palette. Nothing to set up: change the wallpaper and the colours follow.</div>;
  }
  const step = (done: boolean, text: React.ReactNode) => (
    <li className="mstep"><span className={"mstep__mark" + (done ? " done" : "")}>{done ? "✓" : "·"}</span><span>{text}</span></li>
  );
  return (
    <div className="card" style={{ marginTop: 8 }}>
      <div className="card__title" style={{ fontSize: 15 }}>Set up matugen</div>
      <ol className="msteps">
        {step(st.installed, st.installed ? "matugen is installed" : <>Install matugen: <code>sudo pacman -S matugen</code> <button className="linkbtn" onClick={() => copy("sudo pacman -S matugen")}>Copy</button></>)}
        {step(st.registered, st.registered ? "Oumu's template is in your matugen config" : "Add Oumu's template to your matugen config (the button below; your config is backed up first)")}
        {step(st.palette_ready, st.palette_ready ? "The palette is there; Oumu follows every matugen run" : <>Run matugen once on your wallpaper: <code>{st.next_command}</code> <button className="linkbtn" onClick={() => copy(st.next_command)}>Copy</button></>)}
      </ol>
      {error && <div className="notice">{error}</div>}
      {!st.registered && <div className="row-actions"><button className="btn btn--primary" disabled={busy || !st.installed} onClick={setup}>{busy ? "Setting up…" : "Set up matugen"}</button></div>}
      <div style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 8 }}>Palette file: {st.palette_path}</div>
    </div>
  );
}
