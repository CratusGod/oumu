import { useEffect, useState } from "react";
import { MATUGEN, THEMES } from "../../lib/theme";
import { MatugenSetup } from "./MatugenSetup";
import type { Ctx } from "../Panel";
import * as I from "../../components/icons";
import { TitleBar, Empty } from "../Panel";
import { ipc } from "../../lib/ipc";
import { relativeTime, ruleName } from "../../lib/text";
import type { Settings } from "../../lib/types";
import { AiSetup } from "./AiSetup";

export function History({ ctx }: { ctx: Ctx }) {
  return (
    <>
      <TitleBar title="History" />
      <div className="panel__scroll">
        {ctx.history.length === 0 ? <Empty icon={<I.Clock />} title="Nothing yet">Documents you write in show up here.</Empty> : ctx.history.map((h) => (
          <div className="hist" key={h.doc_id}><b>{h.title}</b><span>{h.app_name} · {h.words} words{h.score != null ? ` · score ${h.score}` : ""} · {relativeTime(h.last_seen)}</span></div>
        ))}
      </div>
    </>
  );
}

export function SettingsView({ ctx }: { ctx: Ctx }) {
  const [s, setS] = useState<Settings>(ctx.settings);
  useEffect(() => { setS(ctx.settings); }, [ctx.settings]);
  const save = async (next: Settings) => { setS(next); await ctx.updateSettings(next); };
  return (
    <>
      <TitleBar title="Settings" />
      <div className="panel__scroll">
        <div className="section-label">AI</div>
        <AiSetup ctx={ctx} mode="settings" />
        <div className="section-label">Appearance</div>
        <div className="field"><label>Theme</label><div className="select-wrap"><select className="select" value={s.theme || "default"} onChange={(e) => save({ ...s, theme: e.target.value })}>
          {THEMES.map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
          <option value={MATUGEN}>Matugen (follows the wallpaper)</option>
        </select></div></div>
        {s.theme === MATUGEN && (
          <>
            <div className="field"><label>Palette file (empty: Ryoku's wallpaper palette, else ~/.config/oumu/colors.json)</label><input placeholder="~/.cache/ryoku/colors.json" value={s.palette_path} onChange={(e) => setS({ ...s, palette_path: e.target.value })} onBlur={() => save(s)} /></div>
            <MatugenSetup ctx={ctx} />
          </>
        )}
        <div className="section-label">Suggestions</div>
        <label className="kv"><span>Spot-check sentences when I stop typing</span><input type="checkbox" checked={s.auto_spot_check} onChange={(e) => save({ ...s, auto_spot_check: e.target.checked })} /></label>
        <label className="kv"><span>Show the Oumu button next to text fields</span><input type="checkbox" checked={s.show_button} onChange={(e) => save({ ...s, show_button: e.target.checked })} /></label>
        <label className="kv"><span>Draw underlines in the app</span><input type="checkbox" checked={s.show_underlines} onChange={(e) => save({ ...s, show_underlines: e.target.checked })} /></label>
        <div className="row-actions"><button className="btn btn--outline" onClick={() => save({ ...s, dismissed: [] })}>Restore dismissed suggestions ({s.dismissed.length})</button></div>
        <div className="section-label">Learned</div>
        <p style={{ color: "var(--text-subdued)", fontSize: 13, margin: "2px 0 8px" }}>Words you added from a spelling card, and checks you turned off from a card.</p>
        {s.dictionary.length === 0 && s.muted_rules.length === 0 && <p style={{ color: "var(--text-subdued)", fontSize: 13, margin: "0 0 8px" }}>Nothing yet.</p>}
        {s.dictionary.map((w) => (
          <div className="kv" key={"w:" + w}><span>{w}</span><button className="btn btn--outline" style={{ padding: "4px 10px" }} onClick={() => save({ ...s, dictionary: s.dictionary.filter((x) => x !== w) })}>Remove</button></div>
        ))}
        {s.muted_rules.map((r) => (
          <div className="kv" key={"r:" + r}><span>{ruleName(r)} (off)</span><button className="btn btn--outline" style={{ padding: "4px 10px" }} onClick={() => save({ ...s, muted_rules: s.muted_rules.filter((x) => x !== r), rule_dismissals: { ...s.rule_dismissals, [r]: 0 } })}>Turn on</button></div>
        ))}
        <div className="section-label">Skipped apps</div>
        <p style={{ color: "var(--text-subdued)", fontSize: 13, margin: "2px 0 8px" }}>Oumu stays out of terminals. Add other apps with "Skip this app" in the Review view.</p>
        {s.skip_apps.length === 0 ? <p style={{ color: "var(--text-subdued)", fontSize: 13, margin: "0 0 8px" }}>No apps added.</p> : s.skip_apps.map((a) => (
          <div className="kv" key={a.bundle_id}><span>{a.name || a.bundle_id}</span><button className="btn btn--outline" style={{ padding: "4px 10px" }} onClick={() => save({ ...s, skip_apps: s.skip_apps.filter((x) => x.bundle_id !== a.bundle_id) })}>Turn on</button></div>
        ))}
        <div className="section-label">You</div>
        <div className="field"><label>Name (used for the avatar and greetings)</label><input value={s.user_name} onChange={(e) => setS({ ...s, user_name: e.target.value })} onBlur={() => save(s)} /></div>
        <div className="section-label">Panel</div>
        <div className="field"><label>Dock side</label><div className="select-wrap"><select className="select" value={s.dock_side} onChange={(e) => save({ ...s, dock_side: e.target.value as any })}><option value="left">Left</option><option value="right">Right</option></select></div></div>
        <div className="field"><label>Icon rail</label><div className="select-wrap"><select className="select" value={s.rail_side} onChange={(e) => save({ ...s, rail_side: e.target.value as any })}><option value="right">Right (like Grammarly)</option><option value="left">Left</option></select></div></div>
        <div className="section-label">Plagiarism search</div>
        <SearchSettings s={s} setS={setS} save={save} />
        <div className="section-label">App</div>
        <div className="row-actions"><button className="btn btn--outline" onClick={() => ipc.permissionPrompt()}>Accessibility permission</button><button className="btn btn--ghost" onClick={() => ipc.quit()}>Quit Oumu</button></div>
      </div>
    </>
  );
}

const HINT = { color: "var(--text-subdued)", fontSize: 13, margin: "2px 0 8px" } as const;

/** Plagiarism search: keyless by default, a SearXNG address or a Brave key for steadier results. */
function SearchSettings({ s, setS, save }: { s: Settings; setS: (s: Settings) => void; save: (s: Settings) => Promise<void> }) {
  const [probe, setProbe] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const setSearch = (patch: Partial<Settings["search"]>) => setS({ ...s, search: { ...s.search, ...patch } });
  const test = async () => {
    setBusy(true); setProbe(null);
    try {
      const n = await ipc.probeSearxng(s.search.searxng_url);
      setProbe({ ok: true, text: `Works: a test search found ${n} results.` });
      await save(s);
    } catch (e) {
      setProbe({ ok: false, text: String(e) });
    } finally { setBusy(false); }
  };
  return (
    <>
      <p style={HINT}>Plagiarism checks search the web for free without any setup. For steadier results, point Oumu at your own SearXNG (free, see docs/LINUX.md) or paste a Brave Search key. Oumu uses the Brave key first, then SearXNG, then the free search.</p>
      <div className="field"><label>SearXNG address</label>
        <div style={{ display: "flex", gap: 8 }}>
          <input style={{ flex: 1 }} placeholder="Optional, e.g. http://localhost:8888" value={s.search.searxng_url} onChange={(e) => { setSearch({ searxng_url: e.target.value }); setProbe(null); }} onBlur={() => save(s)} />
          <button className="btn btn--outline" disabled={busy || !s.search.searxng_url.trim()} onClick={test}>{busy ? "Testing…" : "Test"}</button>
        </div>
        {probe && <p style={{ ...HINT, color: probe.ok ? "var(--text-subdued)" : "var(--warn-ink)" }}>{probe.text}</p>}
      </div>
      <div className="field"><label>Brave Search API key</label><input type="password" placeholder="Optional" value={s.search.brave_key} onChange={(e) => setSearch({ brave_key: e.target.value })} onBlur={() => save(s)} /></div>
    </>
  );
}
