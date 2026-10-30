export type Category = "correctness" | "clarity" | "engagement" | "delivery";

export interface Rect { x: number; y: number; w: number; h: number }

export interface Doc {
  id: string; app_name: string; bundle_id: string; pid: number; role: string;
  text: string; sel_start: number; sel_end: number; selected_text: string;
  frame: Rect; visible: Rect; version: number; active: boolean; truncated: boolean;
}

export interface Alert {
  id: string; category: Category; kind: string; title: string; message: string;
  start: number; end: number; u16_start: number; u16_end: number; original: string;
  suggestions: string[]; priority: number; stable_hash: number;
  rule: string; rule_dismissals: number;
}

export interface Status { trusted: boolean; frontmost: string; frontmost_bid: string; skipped: boolean; skip_built_in: boolean; has_doc: boolean; linting: boolean }
export interface SkipApp { bundle_id: string; name: string }

export interface Score {
  words: number; sentences: number; characters: number; reading_time_sec: number; flesch: number;
  overall: number | null; correctness_count: number; clarity_count: number; engagement_count: number; delivery_count: number;
  correctness_label: string; clarity_label: string; engagement_label: string; delivery_label: string;
  correctness_fill: number; clarity_fill: number; engagement_fill: number; delivery_fill: number; note: string | null;
}

export interface Voice { id: string; name: string; description: string; custom: boolean }
export interface Audience { id: string; name: string; description: string; custom: boolean }

/** "ollama" is the optional local model (Settings → AI → Local model). */
export type ProviderId = "none" | "ollama" | "openai" | "anthropic" | "gemini";
export interface LlmConfig { provider: ProviderId; base_url: string; api_key: string; model: string }

export interface Settings {
  llm: LlmConfig; dock_side: "left" | "right"; rail_side: "left" | "right"; user_name: string;
  search: { brave_key: string; searxng_url: string }; custom_voices: Voice[]; custom_audiences: Audience[];
  dismissed: number[]; target_language: string; citation_style: string; show_button: boolean; show_underlines: boolean; onboarded: boolean;
  auto_spot_check: boolean; skip_apps: SkipApp[]; theme: string; palette_path: string;
  dictionary: string[]; muted_rules: string[]; rule_dismissals: Record<string, number>;
}

export interface MatugenStatus { ryoku: boolean; installed: boolean; registered: boolean; palette_ready: boolean; palette_path: string; config_path: string; next_command: string }
export interface KeyProbe { provider: ProviderId; base_url: string; model: string; models: number; reply: string }
export interface SpotResult { busy?: boolean; stale?: boolean; sentences: number; checked: number; from_cache: number; batches: number; skipped: number; rejected: number; suggestions: number; ms: number }

export interface Session { doc_id: string; app_name: string; bundle_id: string; title: string; words: number; score: number | null; last_seen: number }

export interface AppState { doc: Doc | null; alerts: Alert[]; status: Status; settings: Settings; history: Session[]; score: Score | null; initials: string; spot_running: boolean }

export type View =
  | "review" | "chat" | "plagiarism" | "tone" | "performance" | "paraphraser" | "humanizer"
  | "reactions" | "translate" | "explain" | "rewriter" | "detector" | "history" | "settings" | "onboarding";

export interface UnderlineRect { id: string; x: number; y: number; w: number; h: number; color: string; category: string; sentence: boolean }
export interface Geometry { origin_x: number; origin_y: number; width: number; height: number; rects: UnderlineRect[]; hovered: string | null; over_card: boolean; }

export interface LocalModels { running: boolean; base_url: string; models: string[]; pick: string | null }
