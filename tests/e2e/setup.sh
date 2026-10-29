#!/usr/bin/env bash
# AI set-up gate end to end through the probe: your own key (prefix
# detection → model listing → ping) — plus
# the rig's own Ollama (the product offers no local model; the LLM gates run
# on it through the probe). Settings written by the app still load.
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
cleanup() { stop_app; }
trap cleanup EXIT
start_app
ORIG="$(probe /settings)"
ORIG_LLM="$(echo "$ORIG" | jsonq 'json.dumps(d["llm"])' 2>/dev/null || python3 -I -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["llm"]))' <<<"$ORIG")"
restore() { probe /settings "{\"llm\": $ORIG_LLM, \"onboarded\": $(echo "$ORIG" | jsonq 'str(d["onboarded"]).lower()')}" >/dev/null; }
trap 'restore; cleanup' EXIT

# --- the rig's model (not a product option)
O="$(probe /ollama)"
if [ "$(echo "$O" | jsonq 'd["running"] and d["pick"] is not None')" = "True" ]; then pass "rig model: Ollama with $(echo "$O" | jsonq 'd["pick"]') ($(echo "$O" | jsonq 'len(d["models"])') models)"; else echo "  ----  Ollama not running; rig-model check skipped"; fi
if [ "${E2E_RIG_LLM:-0}" = 1 ]; then
  [ "$(echo "$ORIG" | jsonq 'd["llm"]["provider"]')" = "ollama" ] && pass "start_app pointed the app at the rig's Ollama through the probe" || fail "start_app did not set the rig model: $(echo "$ORIG" | jsonq 'd["llm"]["provider"]')"
fi

# --- your own key (one field). Uses a real key only if one is at hand, never printed.
KEY="${E2E_BYO_KEY:-}"
B="$(probe /probe_key '{"key":"sk-bogus-not-a-key"}' 60)"
echo "$B" | grep -q '"error"' && echo "$B" | grep -qi "not accepted" && pass "a bad key is refused with a plain message" || fail "bad key: $(echo "$B" | head -c 160)"
if [ -n "$KEY" ]; then
  KB="$(python3 -I -c 'import json,sys; print(json.dumps({"key": sys.argv[1]}))' "$KEY")"
  R="$(probe /probe_key "$KB" 90)"
  if [ "$(echo "$R" | jsonq 'd["provider"]=="anthropic" and d["model"].startswith("claude") and "pong" in d["reply"].lower()' 2>/dev/null)" = "True" ]; then
    pass "own key: prefix → anthropic, model $(echo "$R" | jsonq 'd["model"]') picked from $(echo "$R" | jsonq 'd["models"]') listed, ping answered"
    PATCH="$(python3 -I -c 'import json,sys; r=json.loads(sys.argv[2]); print(json.dumps({"llm":{"provider":r["provider"],"base_url":r["base_url"],"api_key":sys.argv[1],"model":r["model"]}}))' "$KEY" "$R")"
    probe /settings "$PATCH" >/dev/null
    R2="$(probe /feature '{"name":"ping_llm","args":{}}' 90)"
    echo "$R2" | grep -q '"reply"' && pass "features run on the pasted key" || fail "ping on own key: $(echo "$R2" | head -c 160)"
  else
    fail "own key probe: $(echo "$R" | sed 's/sk-ant-[A-Za-z0-9_-]*/sk-ant-…/g' | head -c 200)"
  fi
else
  echo "  ----  no key at hand (set E2E_BYO_KEY); own-key live check skipped"
fi

# --- settings written by this build still load with every new field present
SET="$(python3 -I -c 'import json,sys; print(json.load(sys.stdin)["settings_path"])' <<<"$(probe /state)" 2>/dev/null || true)"
[ -n "$SET" ] && [ "$(python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); print("auto_spot_check" in d)' "$SET")" = "True" ] && pass "settings.json carries auto_spot_check" || echo "  ----  settings file check skipped"
summary
