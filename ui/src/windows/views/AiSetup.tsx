import { useEffect, useState } from "react";
import type { Ctx } from "../Panel";
import { ipc } from "../../lib/ipc";
import { keyKind } from "../../lib/text";
import type { LlmConfig } from "../../lib/types";
import * as I from "../../components/icons";

/** Base URL and a starting model for each provider picked under Advanced. */
const PROVIDER_DEFAULTS: Record<string, { base: string; model: string }> = {
  gemini: { base: "https://generativelanguage.googleapis.com/v1beta", model: "gemini-3.6-flash" },
  openai: { base: "https://api.openai.com", model: "gpt-4o-mini" },
  anthropic: { base: "https://api.anthropic.com", model: "claude-sonnet-5-5" },
};

/**
 * The AI set-up shared by onboarding and Settings: one key field (the provider
 * and model are detected from the key), with the plumbing behind "Advanced".
 */
export function AiSetup({ ctx, mode, onDone }: { ctx: Ctx; mode: "onboarding" | "settings"; onDone?: () => void }) {
  const llm = ctx.settings.llm;
  const connected = llm.provider !== "none" && llm.api_key.trim().length > 0;
  const [open, setOpen] = useState(!connected);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [key, setKey] = useState("");
  const [ping, setPing] = useState<string | null>(null);

  const finish = () => { if (mode === "onboarding") onDone?.(); };
  const connectKey = async () => {
    setBusy(true); setError(null);
    try {
      const r = await ipc.probeKey(key);
      await ctx.updateSettings({ llm: { provider: r.provider, base_url: r.base_url, api_key: key.trim(), model: r.model } });
      setKey(""); setOpen(false); ctx.toast(`Connected · ${r.model}`); finish();
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };

  return (
    <>
      <button className={"choice" + (connected ? " selected" : "")} onClick={() => setOpen(!open)}>
        <div className="choice__title"><I.Lock style={{ width: 20, height: 20 }} />Your AI key</div>
        <div className="choice__desc">Google Gemini, OpenAI, Anthropic, OpenRouter or Groq. Paste the key; the provider and model are picked for you.</div>
        {connected && <div className="choice__status">Connected · {llm.model}</div>}
      </button>
      {open && (
        <div className="card" style={{ marginTop: -4 }}>
          <div className="field"><label>{keyKind(key) || "API key"}</label><input type="password" autoFocus placeholder="AQ.… / AIza… / sk-…" value={key} onChange={(e) => setKey(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") connectKey(); }} /></div>
          {error && <div className="notice">{error}</div>}
          <div className="row-actions">
            <button className="btn btn--primary" disabled={busy || !key.trim()} onClick={connectKey}>{busy ? "Checking the key…" : "Connect"}</button>
            {connected && <button className="btn btn--ghost" disabled={busy} onClick={() => { setOpen(false); setError(null); }}>Cancel</button>}
          </div>
          <div style={{ fontSize: 12, color: "var(--text-muted)", marginTop: 6 }}>The key stays on this computer. Your account's models are listed and the best fit is picked; change it under Advanced. A free key: aistudio.google.com/apikey.</div>
        </div>
      )}

      {mode === "onboarding" && (
        <div className="row-actions" style={{ justifyContent: "center", marginTop: 18 }}>
          <button className="btn btn--ghost" onClick={() => onDone?.()}>Skip for now · spelling and grammar work without AI</button>
        </div>
      )}

      <details className="adv">
        <summary>Advanced</summary>
        <div className="field"><label>Provider</label><div className="select-wrap"><select className="select" value={llm.provider} onChange={(e) => {
          const p = e.target.value as LlmConfig["provider"];
          const d = PROVIDER_DEFAULTS[p] ?? { base: "", model: "" };
          ctx.updateSettings({ llm: { ...llm, provider: p, base_url: d.base, model: d.model } });
        }}>
          <option value="none">Not set up</option><option value="gemini">Google Gemini</option><option value="openai">OpenAI-compatible</option><option value="anthropic">Anthropic</option>
        </select></div></div>
        <AdvField label="Base URL" value={llm.base_url} onSave={(v) => ctx.updateSettings({ llm: { ...llm, base_url: v } })} />
        <AdvField label="Model" value={llm.model} onSave={(v) => ctx.updateSettings({ llm: { ...llm, model: v } })} />
        <AdvField label="API key" value={llm.api_key} secret onSave={(v) => ctx.updateSettings({ llm: { ...llm, api_key: v } })} />
        <div className="row-actions">
          <button className="btn btn--outline" onClick={async () => { setPing("…"); try { const r = await ipc.runFeature<{ reply: string }>("ping_llm"); setPing(`OK: ${r.reply.slice(0, 40)}`); } catch (e) { setPing(String(e)); } }}>Test connection</button>
          {ping && <span style={{ color: "var(--text-subdued)", fontSize: 13, alignSelf: "center" }}>{ping}</span>}
        </div>
      </details>
    </>
  );
}

function AdvField({ label, value, secret, onSave }: { label: string; value: string; secret?: boolean; onSave: (v: string) => void }) {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  return <div className="field"><label>{label}</label><input type={secret ? "password" : "text"} value={v} onChange={(e) => setV(e.target.value)} onBlur={() => { if (v !== value) onSave(v); }} /></div>;
}
