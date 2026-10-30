//! Test introspection: a loopback HTTP probe enabled only when
//! `OUMU_DEBUG_PORT` is set. Mirrors the `test_introspection` commands
//! Grammarly's own inkwell binary carries.

use crate::host::Shared;
use serde_json::{json, Value};
use tauri::AppHandle;

pub fn spawn(app: AppHandle, shared: Shared) {
    let Ok(port) = std::env::var("OUMU_DEBUG_PORT") else { return };
    let Ok(port) = port.parse::<u16>() else { return };
    std::thread::Builder::new()
        .name("debug-server".into())
        .spawn(move || {
            let server = match tiny_http::Server::http(("127.0.0.1", port)) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("debug server: {e}");
                    return;
                }
            };
            log::info!("debug server on 127.0.0.1:{port}");
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let url = req.url().to_string();
                let path = url.split('?').next().unwrap_or("").to_string();
                let args: Value = if body.trim().is_empty() { json!({}) } else { serde_json::from_str(&body).unwrap_or(json!({})) };
                let result = handle(&app, &shared, &path, args);
                let (code, text) = match result {
                    Ok(v) => (200, v.to_string()),
                    Err(e) => (500, json!({"error": e}).to_string()),
                };
                let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
                let resp = tiny_http::Response::from_string(text).with_status_code(code).with_header(header);
                let _ = req.respond(resp);
            }
        })
        .expect("debug server thread");
}

fn handle(app: &AppHandle, shared: &Shared, path: &str, args: Value) -> Result<Value, String> {
    match path {
        "/health" => Ok(json!({"ok": true, "trusted": crate::ax::is_trusted()})),
        // The permission button, for the harness (Linux: switches the a11y bus on).
        "/trust" => Ok(json!({"ok": true, "trusted": crate::ax::prompt_trust()})),
        "/state" => {
            // Window queries round-trip through the main thread; take them before
            // holding the shared lock so a main-thread command can never deadlock us.
            let panel = crate::overlay::panel_state(app);
            let st = shared.lock().unwrap();
            Ok(json!({
                "doc": st.doc,
                "alerts": st.alerts,
                "status": st.status,
                "geometry": st.geometry,
                "card_rects": st.card_rects,
                "score": st.doc.as_ref().map(|d| crate::score::compute(&d.text, &st.alerts)),
                "ui_ready": st.ui_ready,
                "ui_log": st.ui_log,
                "panel": panel,
                "ipc": ipc_counts(),
            }))
        }
        "/accept" => {
            let id = args["id"].as_str().ok_or("id required")?.to_string();
            let idx = args["suggestion"].as_u64().unwrap_or(0) as usize;
            crate::commands::accept_alert_inner(app, shared, &id, idx).map(|r| serde_json::to_value(r).unwrap())
        }
        "/dismiss" | "/learn" | "/mute" => {
            use crate::commands::Forget;
            let id = args["id"].as_str().ok_or("id required")?.to_string();
            let what = match path { "/learn" => Forget::LearnWord, "/mute" => Forget::MuteRule, _ => Forget::Dismiss };
            crate::commands::forget_alert_inner(app, shared, &id, what)?;
            Ok(json!({"ok": true}))
        }
        "/replace" => {
            let start = args["start"].as_u64().ok_or("start")? as usize;
            let end = args["end"].as_u64().ok_or("end")? as usize;
            let text = args["text"].as_str().ok_or("text")?.to_string();
            crate::commands::replace_range_inner(app, shared, start, end, &text).map(|r| serde_json::to_value(r).unwrap())
        }
        "/feature" => {
            let name = args["name"].as_str().ok_or("name required")?.to_string();
            let fargs = args["args"].clone();
            let app2 = app.clone();
            let shared2 = shared.clone();
            tauri::async_runtime::block_on(async move { crate::commands::run_feature_inner(&app2, &shared2, &name, fargs).await })
        }
        "/panel" => {
            // {"tucked": true|false} slides the panel; otherwise opens it on `view`.
            match args["tucked"].as_bool() {
                Some(true) => crate::commands::tuck_panel_inner(app),
                Some(false) => crate::commands::open_panel_inner(app, args["view"].as_str().unwrap_or("review"), false),
                None => {
                    let view = args["view"].as_str().unwrap_or("review").to_string();
                    crate::commands::open_panel_inner(app, &view, args["focus"].as_bool().unwrap_or(false));
                }
            }
            Ok(json!({"ok": true, "panel": crate::overlay::panel_state(app)}))
        }
        "/mouse" => {
            // {"x","y"} moves the pointer; no args returns where it is.
            match (args["x"].as_f64(), args["y"].as_f64()) {
                (Some(x), Some(y)) => Ok(json!({"ok": crate::ax::post_mouse_move(x, y)})),
                _ => {
                    let (x, y) = crate::ax::mouse_location_top_left().unwrap_or((0.0, 0.0));
                    Ok(json!({"x": x, "y": y}))
                }
            }
        }
        "/settings" => {
            // No body: read. A body: a (shallow, one level deep for objects) patch, saved and broadcast.
            if args.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                let st = shared.lock().unwrap();
                return Ok(serde_json::to_value(&st.settings).unwrap());
            }
            let mut cur = serde_json::to_value(shared.lock().unwrap().settings.clone()).unwrap();
            if let (Some(c), Some(p)) = (cur.as_object_mut(), args.as_object()) {
                for (k, v) in p {
                    match (c.get_mut(k), v.as_object()) {
                        (Some(Value::Object(existing)), Some(patch)) => {
                            for (k2, v2) in patch {
                                existing.insert(k2.clone(), v2.clone());
                            }
                        }
                        _ => {
                            c.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
            let next: crate::settings::Settings = serde_json::from_value(cur).map_err(|e| e.to_string())?;
            {
                let mut st = shared.lock().unwrap();
                st.settings = next.clone();
                crate::settings::save(&st.settings).map_err(|e| e.to_string())?;
                st.alerts = st.visible_alerts();
            }
            use tauri::Emitter;
            let _ = app.emit("settings://changed", &next);
            Ok(serde_json::to_value(next).unwrap())
        }
        "/probe_key" => {
            let key = args["key"].as_str().unwrap_or("").to_string();
            tauri::async_runtime::block_on(async move { crate::commands::probe_key_inner(&key).await })
        }
        "/ollama" => Ok(tauri::async_runtime::block_on(crate::commands::detect_ollama_inner())),
        "/lint" => {
            let text = args["text"].as_str().ok_or("text required")?.to_string();
            let alerts = std::panic::catch_unwind(|| crate::engine::Engine::new().check(&text)).map_err(|_| "engine panicked".to_string())?;
            Ok(json!({"alerts": alerts, "score": crate::score::compute(&text, &alerts)}))
        }
        "/pause" => {
            let mut st = shared.lock().unwrap();
            st.paused = args["paused"].as_bool().unwrap_or(true);
            Ok(json!({"paused": st.paused}))
        }
        _ => Err(format!("unknown path {path}")),
    }
}

/// How many accessibility calls and Hyprland requests Oumu has made since it started.
#[cfg(target_os = "linux")]
fn ipc_counts() -> serde_json::Value {
    use std::sync::atomic::Ordering::Relaxed;
    json!({
        "a11y_calls": crate::linux::ax::CALLS.load(Relaxed),
        "hypr_requests": crate::linux::hyprland::REQUESTS.load(Relaxed),
        "ticks": crate::host::TICKS.load(Relaxed),
        "wakes": crate::pace::WAKES.load(Relaxed),
        "woken_early": crate::pace::WOKEN_EARLY.load(Relaxed),
    })
}

#[cfg(not(target_os = "linux"))]
fn ipc_counts() -> serde_json::Value {
    json!({})
}
