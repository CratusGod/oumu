# Changelog

## Unreleased

Better suggestions:

- The AI check now fixes grammar too ("studies has" → have, its → it's, "three day" → three-day),
  shown on the wrong word only. Rewrites dressed up as grammar fixes are refused.
- Fewer wrong notes: 18 Harper rules that misfire on everyday writing are off; names, acronyms,
  code, file names and chat words ("lol", "ngl", "hotfix") are not spelling mistakes; Vietnamese,
  with or without accents, gets no English notes.
- Missing apostrophes get the right word ("im" → I'm, "dont" → don't).
- Chat apps (Discord, Vesktop, Telegram, Signal, Slack, ...) skip capitalisation and formality notes.
- Style notes only fire where they are clearly right, and each carries a fix ("very good" →
  excellent, "I think that it works" → It works).
- Learning: Add to dictionary on spelling cards; after two dismissals of one kind of note the card
  offers to turn that check off. Settings → Learned lists both and undoes them.
- Dismissed suggestions stay dismissed when the text around them changes; the card no longer
  gets stuck on screen.
- AI checks spend fewer requests: verdicts are kept across restarts, the sentence being typed
  waits for its full stop, and automatic checks rest an hour when the key is out of quota.
- Fixed: Gemini answers could be cut short, which made AI checks fail silently.
- Optional local model through Ollama (Settings → AI → Local model).
- Lower CPU while a field is focused; underlines follow scrolling about twice as fast; X11 overlays
  never take keyboard focus.

## 0.1.0

First release.

- Grammar and spelling from Harper, offline, with underlines in place and Accept into the app
  (GTK 3/4, Qt 6, Chromium, Firefox). Full overlays on Hyprland; X11 and other Wayland desktops
  supported.
- AI tools with your own key (Gemini, OpenAI, Anthropic, OpenRouter, Groq): spot check,
  paraphraser, humanizer, tone, reader reactions, translate, explain it, AI rewriter, AI detector
  and AI chat. A busy Gemini model hands over to the next one.
- Translate any highlighted text: `oumu --translate`, DragShift, or a right-click item in Zen/Firefox.
- Plagiarism check with no key, through your SearXNG, or with a Brave Search key.
- Edge tab, 11 themes plus Matugen.
