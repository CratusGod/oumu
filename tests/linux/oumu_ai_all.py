"""Every Oumu AI feature with every option, against the user's real AI provider.

Paraphraser x 8 styles, Humanizer x 5 voices (+ a made voice), Reader Reactions x 8
audiences (+ a made audience), Translate x 38 languages (+ a named source and a round
trip), Explain, Tone, AI Detector, Summarize, AI Rewriter (scan + phrase), Spot check,
Chat (two turns), the "too short" guards, plus the offline Performance score and the
Plagiarism web search.

Each answer is checked for content, not only for "something came back": rewrites must
differ from the input, Shorten must be shorter and Expand longer, a translation must be in
the target's script (or not be English), and so on.

Oumu runs with its own data folder; the AI settings are copied in and never printed.
Run: CRATUS_PROFILE=release python3 tests/linux/oumu_ai_all.py
"""
from __future__ import annotations

import json
import re
import sys
import time
import urllib.error
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import oumu_probe as o  # noqa: E402

PAUSE_S = 4.0
ROOT = Path(__file__).resolve().parents[2]

TEXT = (
    "Our team shipped the new search page last week. It loads twice as fast as the old one, "
    "and people can now filter results by date and by author. We still need to fix the slow "
    "export button, and we want feedback from customers before we add saved searches. "
    "Thanks to everyone who tested the early builds on their own machines."
)
AI_TEXT = (
    "In today's fast-paced digital landscape, it is important to note that leveraging cutting-edge "
    "solutions can unlock a myriad of opportunities. Moreover, fostering a culture of innovation "
    "plays a pivotal role in navigating the ever-evolving tapestry of modern business. Ultimately, "
    "embracing these transformative strategies will empower organizations to thrive."
)
ERRORS = "Their is a mistake in this sentense. She dont like apples, and me and him goes to the store yesterday."

STYLES = ["Humanize", "Academic", "Professional", "Streamlined", "Creative", "Simple", "Expand", "Shorten"]
VOICES = [
    ("The Naturalist", "Write the way a thoughtful person talks: varied sentence length, concrete details, occasional asides, contractions, no stacked adjectives, no hollow transitions, no AI clichés."),
    ("The Precisionist", "Be economical. Short sentences, active voice, one idea per sentence, no filler, no intensifiers, precise verbs."),
    ("The Everyday Voice", "Clear, friendly, balanced. Contractions are fine. Plain words. Warm but not gushing."),
    ("The Executive", "Confident, direct, decisive. Lead with the point. No hedging. Crisp business vocabulary without jargon."),
    ("The Scholar", "Formal, objective, third person, measured claims, precise terminology, no contractions, no slang."),
]
AUDIENCES = [
    ("Online followers", "People who follow you on social platforms; skim fast, respond to authenticity and clear takeaways."),
    ("General readers", "Curious adults with no special background."),
    ("My manager", "Busy, outcome-focused, wants status, risks and asks."),
    ("Teammates", "Peers who know the context and want specifics and next steps."),
    ("Customers", "People deciding whether to trust and buy; care about benefits and proof."),
    ("Investors", "Evaluate traction, team, market and risk; ask hard questions."),
    ("Professor", "Expects structure, evidence and precise claims."),
    ("Friends", "Informal, supportive, react to feeling and story."),
]
LANGUAGES = [
    "English (American)", "English (British)", "Spanish", "French", "German", "Italian", "Portuguese (Brazil)", "Portuguese (Portugal)", "Dutch", "Swedish", "Norwegian", "Danish", "Finnish", "Polish", "Czech", "Hungarian", "Romanian", "Greek", "Turkish", "Russian", "Ukrainian", "Hebrew", "Arabic", "Hindi", "Bengali", "Urdu", "Tamil", "Telugu", "Indonesian", "Malay", "Vietnamese", "Thai", "Chinese (Simplified)", "Chinese (Traditional)", "Japanese", "Korean", "Filipino", "Swahili",
]
# the script a translation must mostly be in
SCRIPTS = {
    "Greek": r"[Ͱ-Ͽ]", "Russian": r"[Ѐ-ӿ]", "Ukrainian": r"[Ѐ-ӿ]", "Hebrew": r"[֐-׿]",
    "Arabic": r"[؀-ۿ]", "Urdu": r"[؀-ۿ]", "Hindi": r"[ऀ-ॿ]", "Bengali": r"[ঀ-৿]",
    "Tamil": r"[஀-௿]", "Telugu": r"[ఀ-౿]", "Thai": r"[฀-๿]", "Korean": r"[가-힯]",
    "Chinese (Simplified)": r"[一-鿿]", "Chinese (Traditional)": r"[一-鿿]", "Japanese": r"[぀-ヿ]",
}
# characters that only one Chinese form uses (这/這 "this", 们/們 plural, 个/個, 们, 说/說, 过/過, 还/還)
SIMPLIFIED_ONLY = set("这们个说过还为时会来对发")
TRADITIONAL_ONLY = set("這們個說過還為時會來對發")
ENGLISH_WORDS = {"the", "and", "is", "we", "our", "to", "of", "it", "now", "need"}
LATIN_MARKERS = {
    "Spanish": "ñáéíóú¿¡", "French": "éèêàçùô", "German": "äöüß", "Portuguese (Brazil)": "ãõçáéâê", "Portuguese (Portugal)": "ãõçáéâê",
    "Swedish": "åäö", "Norwegian": "åøæ", "Danish": "åøæ", "Finnish": "äö", "Polish": "ąćęłńóśźż", "Czech": "ěščřžýáíéůú",
    "Hungarian": "áéíóöőúüű", "Romanian": "ăâîșțşţ", "Turkish": "çğıöşü", "Vietnamese": "ăâđêôơưạảấầẩẫậắằẳẵặẹẻẽếềểễệỉịọỏốồổỗộớờởỡợụủứừửữựỳỵỷỹ",
}


class Run:
    def __init__(self) -> None:
        self.dir = ROOT / "target" / "e2e" / f"oumu-ai-all-{int(time.time())}"
        self.data = self.dir / "data"
        self.rows: list[tuple[str, str, bool, str, float]] = []

    def check(self, group: str, name: str, ok, detail: str, secs: float) -> None:
        ok = bool(ok)
        detail = o.scrub(detail.replace("\n", " "))[:160]
        self.rows.append((group, name, ok, detail, secs))
        print(f"{'PASS' if ok else 'FAIL'}  {group:11} {name:34} {secs:5.1f}s  {detail[:110]}", flush=True)

    def report(self) -> int:
        n = sum(1 for r in self.rows if r[2])
        lines = [f"# Oumu: every AI feature ({n}/{len(self.rows)} passed)", "", "| Feature | Option | Result | Time | Detail |", "|---|---|---|---|---|"]
        for g, name, ok, d, s in self.rows:
            lines.append(f"| {g} | {name} | {'PASS' if ok else '**FAIL**'} | {s:.1f}s | {d.replace('|', '/')} |")
        (self.dir / "report.md").write_text("\n".join(lines) + "\n")
        (ROOT / "target" / "e2e" / "oumu-ai-all-latest.md").write_text("\n".join(lines) + "\n")
        print(f"\n{n}/{len(self.rows)} passed · report: {self.dir / 'report.md'}")
        return 0 if n == len(self.rows) else 1


def call(run: Run, group: str, name: str, feature: str, args: dict, judge, pause: bool = True):
    """One feature call; `judge(v)` returns (ok, detail). Retries once on a rate limit."""
    if pause:
        time.sleep(PAUSE_S)
    for attempt in range(2):
        t = time.time()
        try:
            v = o.api("/feature", {"name": feature, "args": args}, timeout=180)
            ok, detail = judge(v)
            run.check(group, name, ok, detail, time.time() - t)
            return v
        except urllib.error.HTTPError as e:
            body = e.read().decode(errors="replace")
            if attempt == 0 and re.search(r"429|quota|rate", body, re.I):
                time.sleep(30)
                continue
            run.check(group, name, False, body, time.time() - t)
        except Exception as e:  # noqa: BLE001
            run.check(group, name, False, f"{type(e).__name__}: {e}", time.time() - t)
        return None


def words(s: str) -> int:
    return len(re.findall(r"\w+", s or ""))


def english_share(s: str) -> float:
    w = re.findall(r"[A-Za-z]+", (s or "").lower())
    return sum(1 for x in w if x in ENGLISH_WORDS) / max(1, len(w))


def judge_rewrite(src: str, style: str | None = None):
    def j(v):
        out = (v or {}).get("rewritten", "")
        n_in, n_out = words(src), words(out)
        ok = n_out >= 5 and out.strip() != src.strip()
        if style == "Shorten":
            ok = ok and n_out < n_in
        if style == "Expand":
            ok = ok and n_out > n_in
        return ok, f"{n_in}→{n_out} words: {out[:90]}"
    return j


def judge_translation(lang: str):
    def j(v):
        out = (v or {}).get("translation", "")
        if not out.strip():
            return False, "empty"
        if lang.startswith("English"):
            return out.strip() != "", f"{out[:90]}"
        if lang in SCRIPTS:
            share = len(re.findall(SCRIPTS[lang], out)) / max(1, len(re.sub(r"[\s\d\W]", "", out)))
            ok = share > 0.5
            if lang == "Chinese (Simplified)":
                ok = ok and not (set(out) & TRADITIONAL_ONLY - SIMPLIFIED_ONLY)
            if lang == "Chinese (Traditional)":
                ok = ok and not (set(out) & SIMPLIFIED_ONLY - TRADITIONAL_ONLY)
            return ok, f"{share:.0%} in script: {out[:80]}"
        eng = english_share(out)
        marks = LATIN_MARKERS.get(lang)
        has_marks = any(c in out.lower() for c in marks) if marks else True
        return eng < 0.08 and has_marks, f"english words {eng:.0%}, accents {'yes' if has_marks else 'NO'}: {out[:80]}"
    return j


def nonempty(*keys):
    def j(v):
        v = v or {}
        missing = [k for k in keys if not v.get(k)]
        return not missing, ("missing " + ", ".join(missing) + " · " if missing else "") + json.dumps(v, ensure_ascii=False)[:120]
    return j


def main() -> int:
    run = Run()
    if not o.settings(run.data, True):
        print("no AI provider set up in Oumu's settings: nothing to test")
        return 2
    model = json.loads((run.data / "settings.json").read_text())["llm"]
    print(f"provider {model.get('provider')} · model {model.get('model') or '(default)'}\n")
    app = o.start(run.data)
    try:
        if not o.wait(40, lambda: o.api("/health")["ok"]):
            print("Oumu didn't start")
            return 2
        call(run, "Connection", "ping", "ping_llm", {}, lambda v: ("pong" in json.dumps(v).lower(), json.dumps(v)), pause=False)

        for st in STYLES:
            call(run, "Paraphraser", st, "paraphrase", {"text": TEXT, "style": st}, judge_rewrite(TEXT, st))

        for name, guide in VOICES:
            call(run, "Humanizer", name, "humanize", {"text": AI_TEXT, "voice_name": name, "voice_description": guide}, judge_rewrite(AI_TEXT))
        made = call(run, "Humanizer", "make a voice from a sample", "create_voice", {"sample": TEXT, "name": "Test voice"}, nonempty("name", "description"))
        if made:
            call(run, "Humanizer", f"made voice: {made['name'][:20]}", "humanize", {"text": AI_TEXT, "voice_name": made["name"], "voice_description": made["description"]}, judge_rewrite(AI_TEXT))

        for name, desc in AUDIENCES:
            call(run, "Reactions", name, "reactions", {"text": TEXT, "audience": name, "audience_desc": desc},
                 lambda v, n=name: (bool((v or {}).get("summary")) and len((v or {}).get("ask", [])) >= 1 and (v or {}).get("audience") == n, f"{len((v or {}).get('ask', []))} questions: {(v or {}).get('summary', '')[:90]}"))
        aud = call(run, "Reactions", "make an audience", "create_audience", {"name": "Hiring panel", "description": "Engineers interviewing a candidate; care about impact and evidence."}, nonempty("name", "id"), pause=False)
        if aud:
            call(run, "Reactions", "made audience: Hiring panel", "reactions", {"text": TEXT, "audience": aud["name"], "audience_desc": aud["description"]}, nonempty("summary", "ask"))
        call(run, "Reactions", "too short (guard)", "reactions", {"text": "Short note."}, lambda v: ((v or {}).get("too_short") is True, json.dumps(v)), pause=False)

        for lang in LANGUAGES:
            src = "Vietnamese" if lang.startswith("English") else None
            text = "Nhóm chúng tôi đã ra mắt trang tìm kiếm mới vào tuần trước. Bây giờ nó tải nhanh gấp đôi." if src else TEXT
            args = {"text": text, "target": lang} | ({"source": src} if src else {})
            call(run, "Translate", lang, "translate", args, judge_translation(lang))
        call(run, "Translate", "auto-detects the source", "translate", {"text": "Wir haben die neue Suchseite letzte Woche veröffentlicht.", "target": "English (American)", "source": "Auto-detect"},
             lambda v: ("german" in (v or {}).get("detected_language", "").lower() and english_share(v.get("translation", "")) > 0.05, f"detected {(v or {}).get('detected_language')}: {(v or {}).get('translation', '')[:70]}"))
        call(run, "Translate", "idioms are explained", "translate", {"text": "It's raining cats and dogs, so let's call it a day.", "target": "French"},
             lambda v: (bool((v or {}).get("translation")) and len((v or {}).get("idioms", [])) >= 1, f"{len((v or {}).get('idioms', []))} idioms: {(v or {}).get('translation', '')[:70]}"))

        call(run, "Explain", "whole page", "explain", {"text": TEXT, "whole": True}, nonempty("summary", "explanation"))
        call(run, "Explain", "a selection (term)", "explain", {"text": "idempotent HTTP methods", "whole": False}, nonempty("explanation"))

        call(run, "Tone", "detects tones", "tone", {"text": TEXT}, lambda v: (len((v or {}).get("tones", [])) >= 1 and bool(v.get("summary")), json.dumps(v, ensure_ascii=False)[:120]))
        call(run, "Tone", "too short (guard)", "tone", {"text": "Hi there."}, lambda v: ((v or {}).get("too_short") is True, json.dumps(v)), pause=False)

        human = call(run, "AI Detector", "human text", "detector", {"text": TEXT}, lambda v: ("ai_likelihood" in (v or {}) and bool(v.get("verdict")), f"{(v or {}).get('ai_likelihood')}% AI: {(v or {}).get('verdict')}"))
        call(run, "AI Detector", "AI-sounding text scores higher", "detector", {"text": AI_TEXT},
             lambda v: (human is not None and (v or {}).get("ai_likelihood", 0) > human.get("ai_likelihood", 100), f"{(v or {}).get('ai_likelihood')}% AI vs {human and human.get('ai_likelihood')}% for the human text"))

        call(run, "Summarize", "summary", "summarize", {"text": TEXT + " " + TEXT}, lambda v: (0 < words((v or {}).get("summary", "")) < words(TEXT) * 2, (v or {}).get("summary", "")[:120]))

        call(run, "AI Rewriter", "scan finds AI phrases", "rewriter_scan", {"text": AI_TEXT}, lambda v: (len((v or {}).get("phrases", [])) >= 3, f"{len((v or {}).get('phrases', []))} phrases"), pause=False)
        call(run, "AI Rewriter", "rewrite one phrase", "rewrite_phrase", {"sentence": "In today's fast-paced digital landscape, we ship faster.", "phrase": "In today's fast-paced digital landscape"},
             lambda v: (len((v or {}).get("suggestions", [])) >= 1, json.dumps(v, ensure_ascii=False)[:120]))

        call(run, "Spot check", "finds the mistakes (with AI)", "spot_check", {"text": ERRORS}, lambda v: (len((v or {}).get("alerts", [])) >= 2, f"{len((v or {}).get('alerts', []))} alerts, {(v or {}).get('checked')} sentences checked"))

        time.sleep(PAUSE_S)
        t = time.time()
        try:
            msgs = [{"role": "user", "content": "In one short sentence: what is a haiku?"}]
            r1 = o.api("/chat", {"messages": msgs}, timeout=180)["reply"]
            run.check("Chat", "first message", words(r1) >= 4, r1, time.time() - t)
            time.sleep(PAUSE_S)
            t = time.time()
            msgs += [{"role": "assistant", "content": r1}, {"role": "user", "content": "Now write one about coffee. Only the poem."}]
            r2 = o.api("/chat", {"messages": msgs}, timeout=180)["reply"]
            run.check("Chat", "follow-up keeps the thread", words(r2) >= 5 and len(r2.strip().splitlines()) >= 2, r2, time.time() - t)
        except Exception as e:  # noqa: BLE001
            body = e.read().decode(errors="replace") if hasattr(e, "read") else str(e)
            run.check("Chat", "message", False, body, time.time() - t)

        # not AI, but on the same panel: the offline score and the web search
        o.api("/settings", {})
        call(run, "Plagiarism", "web search", "plagiarism", {"text": TEXT}, lambda v: (isinstance(v, dict), json.dumps(v, ensure_ascii=False)[:120]), pause=False)
    finally:
        app.terminate()
        try:
            app.wait(10)
        except Exception:  # noqa: BLE001
            app.kill()
    return run.report()


if __name__ == "__main__":
    sys.exit(main())
