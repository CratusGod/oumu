//! The underlines (drawn by strips.rs), their suggestion card, the edge tab, and the
//! floating Oumu button. Ported from Underlines.tsx, Tab.tsx and OumuButton.tsx.

use super::style::*;
use super::{Alert, Model};
use crate::host::Shared;
use rune::Cx;
use std::time::{Duration, Instant};
use tauri::AppHandle;

pub const CARD_W: f32 = 300.0;
const CARD_H: f32 = 220.0;
/// Dismissals of one rule before the card offers to turn it off.
const MUTE_OFFER_AFTER: u32 = 2;

/// Which alert's card is open, and when it closes (the app keeps this; the card overlay
/// gets a copy of what it shows).
#[derive(Default)]
pub struct Layer {
    pub open: Option<String>,
    close_at: Option<Instant>,
    pub error: Option<String>,
    /// the card's laid-out size (logical px)
    pub size: Option<(f32, f32)>,
    last_report: Option<(String, [i32; 4])>,
}

/// "SpellCheck" -> "Spell check"; "harper::ThisRule" -> "This rule".
pub fn rule_name(r: &str) -> String {
    let last = r.rsplit("::").next().unwrap_or(r);
    let mut out = String::new();
    for (i, ch) in last.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push(' ');
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out.replace(['_', '-'], " ")
}

impl Layer {
    /// Hover opens the card; leaving both the word and the card closes it after 400 ms.
    /// Returns true if what the card shows changed.
    pub fn update(&mut self, m: &Model) -> bool {
        let before = self.open.clone();
        let g = &m.geometry;
        if g.hovered.is_some() || g.over_card {
            self.close_at = None;
            if let Some(h) = &g.hovered {
                if self.open.as_ref() != Some(h) {
                    self.open = Some(h.clone());
                    self.error = None;
                }
            }
        } else if self.open.is_some() {
            let t = *self.close_at.get_or_insert(Instant::now() + Duration::from_millis(400));
            if Instant::now() >= t {
                self.open = None;
                self.close_at = None;
            }
        }
        if let Some(o) = &self.open {
            if !g.rects.iter().any(|r| &r.id == o) || !m.alerts.iter().any(|a| &a.id == o) {
                self.open = None;
            }
        }
        self.open != before
    }

    /// The bars to draw: (x, y, w, h, colour with alpha) in layer logical px.
    pub fn bars(&self, m: &Model, k: &Tok) -> Vec<(f64, f64, f64, f64, [u8; 4])> {
        let g = &m.geometry;
        g.rects
            .iter()
            .filter(|r| r.sentence)
            .chain(g.rects.iter().filter(|r| !r.sentence))
            .map(|r| {
                let a = if self.open.as_ref() == Some(&r.id) { 1.0 } else if r.sentence { 0.55 } else { 0.9 };
                let h = if r.sentence { 1.0 } else { 2.0 };
                let c = k.cat(&r.category);
                (r.x, r.y + r.h - h, r.w, h, [c[0], c[1], c[2], (a * 255.0) as u8])
            })
            .collect()
    }

    /// Where the card goes in the layer (logical px): below the word, else above it.
    pub fn placement(&self, m: &Model) -> Option<(f32, f32, f32, f32)> {
        let g = &m.geometry;
        let o = self.open.as_ref()?;
        let an = g.rects.iter().find(|r| &r.id == o)?;
        let (w, h) = self.size.unwrap_or((CARD_W, 190.0));
        let left = (an.x as f32).min(g.width as f32 - CARD_W - 4.0).max(4.0);
        let below = (an.y + an.h) as f32 + 8.0;
        let top = if below + CARD_H < g.height as f32 { below } else { ((an.y as f32) - h - 8.0).max(4.0) };
        Some((left, top, w, h))
    }

    /// Tell the host where the card is (the pointer over it keeps the card open).
    pub fn report(&mut self, app: &AppHandle, m: &Model) {
        let mut rects = vec![];
        if let (Some((l, t, w, h)), Some(o)) = (self.placement(m), self.open.as_ref()) {
            rects.push(crate::ax::Rect { x: l as f64, y: t as f64, w: w as f64, h: h as f64 });
            if let Some(an) = m.geometry.rects.iter().find(|r| &r.id == o) {
                let top = (an.y as f32).min(t);
                let bottom = ((an.y + an.h) as f32).max(t + h);
                rects.push(crate::ax::Rect { x: (an.x as f32).min(l) as f64, y: top as f64, w: (an.w as f32).max(48.0) as f64, h: (bottom - top) as f64 });
            }
        }
        let key = rects.first().map(|r| (self.open.clone().unwrap_or_default(), [r.x as i32, r.y as i32, r.w as i32, r.h as i32]));
        if key != self.last_report {
            self.last_report = key;
            crate::commands::set_card_rects(app.state::<Shared>(), rects);
        }
    }
}

// ---------------------------------------------------------------- the overlays' state

/// What the tab, button and card overlays show. The app pushes this in.
pub struct SurfS {
    pub tok: Tok,
    pub kit: rune_kit::Kit,
    pub app: Option<AppHandle>,
    pub front: Option<rune::AppHandle<super::Front>>,
    pub alerts: usize,
    pub linting: bool,
    pub tab_open: bool,
    pub side: String,
    /// the open card: its alert and the underline's category
    pub card: Option<(Alert, String)>,
    pub error: Option<String>,
    /// where Accept is in the card window (logical px), for the E2E run
    pub accept: Option<[f32; 4]>,
}

impl SurfS {
    pub fn new(tok: Tok) -> SurfS {
        SurfS { kit: kit(&tok), tok, app: None, front: None, alerts: 0, linting: false, tab_open: false, side: "left".into(), card: None, error: None, accept: None }
    }
}

fn badge(n: usize, k: &Tok, top: f32, right: f32, size: f32) -> El {
    let label = if n > 99 { "99+".to_string() } else { n.to_string() };
    row().absolute(0.0, top).pin(None, Some(top), Some(right), None).h(size).px(4.0).radius(size / 2.0).items_center().justify_center().fill(c(k.correctness))
        .child(text(label).font(ts(if size > 16.0 { 11.0 } else { 10.0 }, 700.0)).color(c(k.bg)))
}

/// The floating Oumu button near the field: alert count, click opens the panel.
pub fn button_view(s: &SurfS, _: &mut Cx<SurfS>) -> El {
    let k = &s.tok;
    let mut b = k.glass(row().key("obtn").size(48.0, 48.0).radius(16.0).center().border(1.0, c(k.glass_border())).shadow(6.0, 18.0, Color::BLACK.alpha(0.45)).cursor(rune::Cursor::Pointer).role("button").label("Open Oumu"))
        .child(icon("mark", k.brand, 26.0).key("btn-mark"));
    // keyed: unkeyed, the red count and the cyan check morphed into each other (see tab_view)
    b = if s.alerts > 0 {
        b.child(badge(s.alerts, k, -6.0, -6.0, 18.0).key("btn-count"))
    } else {
        b.child(row().key("btn-check").absolute(0.0, -6.0).pin(None, Some(-6.0), Some(-6.0), None).size(18.0, 18.0).radius(9.0).center().fill(c(k.brand)).child(icon("check", k.bg, 11.0)))
    };
    if s.linting {
        b = b.child(el().key("btn-ring").absolute(-3.0, -3.0).size(54.0, 54.0).radius(18.0).border(2.0, c(k.brand)).cycle(rune::Cycle { period: 1.0, phase: 0.0, stops: &[(0.0, 0.3), (0.5, 1.0), (1.0, 0.3)], still: 1.0, fps: 30.0 }));
    }
    col().w_full().h_full().center().child(b.on_press(|s: &mut SurfS| {
        if let Some(app) = &s.app {
            crate::commands::open_panel_inner(app, "review", true);
        }
    }))
}

/// The edge tab: a sliver at the screen edge while idle; pointing at it slides the tab out,
/// clicking opens the panel.
pub fn tab_view(s: &SurfS, _: &mut Cx<SurfS>) -> El {
    let k = &s.tok;
    let right = s.side == "right";
    let corners = if right { [14.0, 0.0, 0.0, 14.0] } else { [0.0, 14.0, 14.0, 0.0] };
    let open = s.tab_open;
    let mut t = k.glass(row().key("tab").size(40.0, 84.0).corners(corners).center().border(1.0, c(k.glass_border())).cursor(rune::Cursor::Pointer).role("button").label("Open Oumu"));
    if open {
        // keyed apart from the lip: unkeyed, Rune took the lip and the icon for one element and
        // morphed the coloured lip into the icon's box, a block over the icon that faded out
        t = t.child(icon("mark", k.brand, 22.0).key("tab-mark"));
        if s.alerts > 0 {
            t = t.child(badge(s.alerts, k, 8.0, 5.0, 16.0).key("tab-badge"));
        }
    } else {
        let lip = if s.alerts > 0 { k.correctness } else { k.brand };
        let x = if right { 2.0 } else { 35.0 };
        t = t.child(el().key("tab-lip").absolute(x, 14.0).size(3.0, 56.0).radius(2.0).fill(c(lip).alpha(0.9)));
    }
    let t = t
        .on_press(|s: &mut SurfS| {
            if let Some(app) = &s.app {
                crate::commands::open_panel_inner(app, "review", true);
            }
        })
        .on_hover_change(|s: &mut SurfS, inside, _| {
            if inside != s.tab_open {
                s.tab_open = inside;
                if let Some(app) = &s.app {
                    crate::commands::tab_hover(app.clone(), inside);
                }
            }
        });
    let r = row().w_full().h_full().items_center();
    if right { r.justify_end().child(t) } else { r.child(t) }
}

fn card_act(s: &mut SurfS, what: &'static str) {
    let Some((a, _)) = s.card.clone() else { return };
    if let Some(f) = &s.front {
        f.update(move |f, _| f.card_action(&a.id, what));
    }
}

/// The suggestion card under a hovered underline.
pub fn card_view(s: &SurfS, _: &mut Cx<SurfS>) -> El {
    let k = &s.tok;
    let Some((a, cat)) = &s.card else { return el() };
    let head = row().gap(8.0).items_center().child(el().size(10.0, 10.0).radius(5.0).fill(c(k.cat(cat)))).child(t(format!("{} · {}", cap(&a.category), a.title), 13.0, k.text_subdued).grow(1.0).min_w(0.0));
    let preview = if a.suggestions.is_empty() {
        p(a.original.clone(), 17.0, k.text)
    } else if !a.suggestions[0].is_empty() && a.original.split_whitespace().count() > 6 {
        diff(&a.original, &a.suggestions[0], 17.0, k)
    } else {
        let sug = if a.suggestions[0].is_empty() { "Remove".to_string() } else { a.suggestions[0].clone() };
        let full = format!("{} {}", a.original, sug);
        let n = a.original.len();
        text(full.clone()).wrap().w_full().font(ts(17.0, 420.0)).color(c(k.text)).spans(vec![rune::text::Span::new(0, n).color(c(k.text_muted)), rune::text::Span::new(n + 1, full.len()).weight(600.0)]).strikes(vec![(0, n, Some(c(k.text_muted)))])
    };
    let mut actions = row().gap(12.0).items_center().flex_wrap();
    if !a.suggestions.is_empty() {
        actions = actions.child(btn("accept", Btn::Primary, "Accept", k).px(14.0).py(8.0).on_layout(|s: &mut SurfS, r| {
                if s.accept != Some(r) {
                    s.accept = Some(r);
                    if let Some(f) = &s.front {
                        f.update(move |f, _| f.card_accept(r));
                    }
                }
            }).on_press(|s: &mut SurfS| card_act(s, "accept")));
    }
    actions = actions.child(btn("dismiss", Btn::Ghost, "Dismiss", k).on_press(|s: &mut SurfS| card_act(s, "dismiss")));
    if a.kind == "Spelling" {
        actions = actions.child(btn("learn", Btn::Ghost, "Add to dictionary", k).on_press(|s: &mut SurfS| card_act(s, "learn")));
    }
    let mut card = k.glass(col().key("card").w(CARD_W).radius(14.0).px(14.0).py(12.0).gap(10.0).border(1.0, c(k.glass_border())))
        .child(head)
        .child(col().w_full().max_h(220.0).scroll(rune::ScrollSpec { stick_bottom: false }).child(preview))
        .child(p(a.message.clone(), 13.0, k.text_subdued))
        .child(actions);
    if !a.rule.is_empty() && a.rule_dismissals >= MUTE_OFFER_AFTER {
        card = card.child(
            row().gap(6.0).items_center().flex_wrap()
                .child(t(format!("You dismissed “{}” {} times.", rule_name(&a.rule), a.rule_dismissals), 12.0, k.text_subdued))
                .child(btn("mute", Btn::Ghost, "Turn it off", k).px(6.0).py(2.0).on_press(|s: &mut SurfS| card_act(s, "mute"))),
        );
    }
    if let Some(e) = &s.error {
        card = card.child(p(e.clone(), 12.0, k.correctness));
    }
    col().w_full().child(card.on_layout(|s: &mut SurfS, r| {
        if let Some(f) = &s.front {
            let size = (r[2], r[3]);
            f.update(move |f, _| f.card_size(size));
        }
    }))
}
