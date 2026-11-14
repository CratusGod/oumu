//! Oumu's icons (components/icons.tsx), rasterized from SVG in the colour asked for.

use std::collections::HashMap;
use std::sync::Mutex;

const BASE: &str = r#"fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round""#;

/// The inner SVG of each icon (24×24 view box, stroked in currentColor unless it says so).
fn body(name: &str) -> &'static str {
    match name {
        "mark" => r#"<g fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13.2 5.3 A7.2 7.2 0 1 0 14.4 19.2"/><path d="M13.2 5.3 C17.7 5.1 21 8.5 20.7 12.8 C20.5 15.6 19.3 17.6 17 18.3"/><path d="M14.5 12.5 C15.9 13.3 16.6 15.4 17 18.3"/><path d="M9.2 5.6 C8.6 3.6 6.8 2.5 4.8 2.8"/><path d="M7.4 7.1 C6.2 5.6 4.3 5.1 2.6 5.8"/></g><circle cx="11" cy="10.4" r="1.35" fill="currentColor"/>"#,
        "chat" => r#"<path d="M4 5.5A2.5 2.5 0 0 1 6.5 3h11A2.5 2.5 0 0 1 20 5.5v8a2.5 2.5 0 0 1-2.5 2.5H10l-4.5 4v-4H6.5A2.5 2.5 0 0 1 4 13.5z"/><path d="M12 6.5l1 2.5 2.5 1-2.5 1-1 2.5-1-2.5-2.5-1 2.5-1z" fill="currentColor" stroke="none"/>"#,
        "incognito" => r#"<path d="M3 11h18"/><path d="M6.5 11l1.8-5.2A1.5 1.5 0 0 1 9.7 5h4.6a1.5 1.5 0 0 1 1.4.8L17.5 11"/><circle cx="8" cy="16" r="2.6"/><circle cx="16" cy="16" r="2.6"/><path d="M10.6 16h2.8"/>"#,
        "tone" => r#"<path d="M4 5.5A2.5 2.5 0 0 1 6.5 3h11A2.5 2.5 0 0 1 20 5.5v8a2.5 2.5 0 0 1-2.5 2.5H10l-4.5 4v-4H6.5A2.5 2.5 0 0 1 4 13.5z"/><path d="M7.5 10.5l1.5-2 1.5 4 1.5-5 1.5 5 1.5-3 1.5 1"/>"#,
        "target" => r#"<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="4.5"/><circle cx="12" cy="12" r="1" fill="currentColor"/>"#,
        "paraphrase" => r#"<path d="M17 4.5A8 8 0 0 0 4.6 9.2"/><path d="M4 5v4.5h4.5"/><path d="M7 19.5a8 8 0 0 0 12.4-4.7"/><path d="M20 19v-4.5h-4.5"/><path d="M14.5 10.5l2 2-4.5 4.5H10v-2z"/>"#,
        "heart" => r#"<path d="M12 20s-7.5-4.6-7.5-10A4.2 4.2 0 0 1 12 7.4 4.2 4.2 0 0 1 19.5 10c0 5.4-7.5 10-7.5 10z"/>"#,
        "book" => r#"<path d="M4 5.5A1.5 1.5 0 0 1 5.5 4H10a2 2 0 0 1 2 2v13a2 2 0 0 0-2-2H5.5A1.5 1.5 0 0 1 4 15.5z"/><path d="M20 5.5A1.5 1.5 0 0 0 18.5 4H14a2 2 0 0 0-2 2v13a2 2 0 0 1 2-2h4.5a1.5 1.5 0 0 0 1.5-1.5z"/><path d="M12 6V3"/>"#,
        "translate" => r#"<path d="M3 6h9"/><path d="M7.5 3.5V6"/><path d="M10.5 6c-.8 4-3.6 7-7 8.5"/><path d="M5 9c1.2 2.6 3.3 4.7 6 5.5"/><path d="M13 20l4-9 4 9"/><path d="M14.3 17h5.4"/>"#,
        "glasses" => r#"<circle cx="7" cy="14" r="3.5"/><circle cx="17" cy="14" r="3.5"/><path d="M10.5 14h3"/><path d="M3.5 14l2-6h3"/><path d="M20.5 14l-2-6h-3"/>"#,
        "bolt" => r#"<path d="M11 3L5 13h5l-1 8 7-11h-5z" fill="currentColor" stroke="none"/><path d="M15 6h5"/><path d="M16 10h4"/><path d="M17 14h3"/>"#,
        "scanner" => r#"<path d="M4 8V5.5A1.5 1.5 0 0 1 5.5 4H8"/><path d="M16 4h2.5A1.5 1.5 0 0 1 20 5.5V8"/><path d="M20 16v2.5a1.5 1.5 0 0 1-1.5 1.5H16"/><path d="M8 20H5.5A1.5 1.5 0 0 1 4 18.5V16"/><path d="M12 8l1.2 2.8L16 12l-2.8 1.2L12 16l-1.2-2.8L8 12l2.8-1.2z" fill="currentColor" stroke="none"/>"#,
        "clock" => r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>"#,
        "edit" => r#"<path d="M4 20h4l10.5-10.5a2 2 0 0 0 0-2.8l-1.2-1.2a2 2 0 0 0-2.8 0L4 16z"/><path d="M13.5 6.5l4 4"/>"#,
        "refresh" => r#"<path d="M20 12a8 8 0 0 1-14.2 5"/><path d="M4 12a8 8 0 0 1 14.2-5"/><path d="M4 7v5h5"/><path d="M20 17v-5h-5"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5"/><circle cx="12" cy="8" r=".6" fill="currentColor"/>"#,
        "check" => r#"<path d="M5 12.5l4.5 4.5L19 7.5"/>"#,
        "lock" => r#"<rect x="5" y="10.5" width="14" height="10" rx="2"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/>"#,
        "arrow" => r#"<path d="M5 12h14"/><path d="M13 6l6 6-6 6"/>"#,
        "arrow-up" => r#"<path d="M12 19V5"/><path d="M6 11l6-6 6 6"/>"#,
        "plus" => r#"<path d="M12 5v14"/><path d="M5 12h14"/>"#,
        "chevron-down" => r#"<path d="M6 9l6 6 6-6"/>"#,
        "chevron-right" => r#"<path d="M9 6l6 6-6 6"/>"#,
        "chevron-left" => r#"<path d="M15 6l-6 6 6 6"/>"#,
        "globe" => r#"<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17"/><path d="M12 3.5c3 3 3 14 0 17"/><path d="M12 3.5c-3 3-3 14 0 17"/>"#,
        "shield" => r#"<path d="M12 3l7 3v5c0 5-3.5 8.5-7 10-3.5-1.5-7-5-7-10V6z"/>"#,
        "users" => r#"<circle cx="9" cy="8" r="3"/><circle cx="17" cy="9" r="2.3"/><path d="M3.5 19a5.5 5.5 0 0 1 11 0"/><path d="M14.5 18.5a4 4 0 0 1 6 0"/>"#,
        "face" => r#"<rect x="4" y="4" width="16" height="16" rx="4"/><circle cx="9.5" cy="10" r=".8" fill="currentColor"/><circle cx="14.5" cy="10" r=".8" fill="currentColor"/><path d="M9 14.5c1.5 1.5 4.5 1.5 6 0"/>"#,
        "scissors" => r#"<circle cx="7" cy="7" r="2.5"/><circle cx="7" cy="17" r="2.5"/><path d="M9 8.5L19 17"/><path d="M9 15.5L19 7"/>"#,
        "cup" => r#"<path d="M5 9h11v5a4 4 0 0 1-4 4H9a4 4 0 0 1-4-4z"/><path d="M16 10.5h1.5a2 2 0 0 1 0 4H16"/><path d="M8 5.5v1.5"/><path d="M11 5v2"/><path d="M14 5.5v1.5"/>"#,
        "compass" => r#"<circle cx="12" cy="12" r="8.5"/><path d="M15.5 8.5l-2 5-5 2 2-5z" fill="currentColor" stroke="none"/>"#,
        "quill" => r#"<path d="M19 4c-6 0-10 4-11 10l-3 6"/><path d="M8 14h3c4 0 7-3 8-10z"/>"#,
        "sparkle" => r#"<path d="M12 4l1.6 4.4L18 10l-4.4 1.6L12 16l-1.6-4.4L6 10l4.4-1.6z" fill="currentColor" stroke="none"/><path d="M18.5 15.5l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z" fill="currentColor" stroke="none"/>"#,
        "close" => r#"<path d="M6 6l12 12"/><path d="M18 6L6 18"/>"#,
        "gear" => r#"<circle cx="12" cy="12" r="3"/><path d="M12 3v2.5M12 18.5V21M3 12h2.5M18.5 12H21M5.6 5.6l1.8 1.8M16.6 16.6l1.8 1.8M5.6 18.4l1.8-1.8M16.6 7.4l1.8-1.8"/>"#,
        _ => r#"<path d="M14.5 4.5l5 5L8 21H3v-5z"/><path d="M12.5 6.5l5 5"/>"#, // pencil
    }
}

/// Icon pixels (straight RGBA) at `px` × `px`, cached by name, colour and size.
pub fn pixels(name: &str, color: [u8; 3], px: u32) -> rune::Pixels {
    static CACHE: Mutex<Option<HashMap<(String, [u8; 3], u32), rune::Pixels>>> = Mutex::new(None);
    let key = (name.to_string(), color, px);
    if let Some(p) = CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(&key) {
        return p.clone();
    }
    let hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
    let inner = body(name).replace("currentColor", &hex);
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" color="{hex}"><g {}>{inner}</g></svg>"#, BASE.replace("currentColor", &hex));
    let out = rune::Pixels::new();
    if let Ok(tree) = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()) {
        if let Some(mut pm) = resvg::tiny_skia::Pixmap::new(px, px) {
            let k = px as f32 / 24.0;
            resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pm.as_mut());
            // tiny-skia is premultiplied; Pixels wants straight alpha
            let mut rgba = pm.take();
            for p in rgba.chunks_exact_mut(4) {
                let a = p[3] as u32;
                if a > 0 && a < 255 {
                    for c in &mut p[..3] {
                        *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                    }
                }
            }
            out.set(px, px, rgba);
        }
    }
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, out.clone());
    out
}
