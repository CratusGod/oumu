//! Oumu's look on Rune, from the React UI's tokens.css and app.css: a theme sets the base
//! colours, everything else is derived (color-mix) the same way.

use super::theme::Theme;
pub use rune::{Color, El, Tx, col, el, row, text};
use rune::tok::TextStyle;

/// A theme colour (sRGB bytes) as Rune's linear colour.
pub fn c(rgb: [u8; 3]) -> Color {
    let d = |v: u8| rune::color::srgb_decode(v as f32 / 255.0);
    Color { r: d(rgb[0]), g: d(rgb[1]), b: d(rgb[2]), a: 1.0 }
}

/// CSS color-mix(in srgb, a p%, b).
pub fn mix(a: [u8; 3], p: f32, b: [u8; 3]) -> [u8; 3] {
    let m = |x: u8, y: u8| (x as f32 * p + y as f32 * (1.0 - p)).round() as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

/// Every token the CSS uses, resolved for a theme.
#[derive(Clone, Debug)]
pub struct Tok {
    pub light: bool,
    pub bg: [u8; 3],
    pub elevated: [u8; 3],
    pub subdued: [u8; 3],
    pub surface: [u8; 3],
    pub border: [u8; 3],
    pub border_subdued: [u8; 3],
    pub text: [u8; 3],
    pub text_subdued: [u8; 3],
    pub text_muted: [u8; 3],
    pub icon: [u8; 3],
    pub brand: [u8; 3],
    pub brand_strong: [u8; 3],
    pub on_brand: [u8; 3],
    pub correctness: [u8; 3],
    pub clarity: [u8; 3],
    pub engagement: [u8; 3],
    pub delivery: [u8; 3],
    pub plagiarism: [u8; 3],
    pub warning: [u8; 3],
    pub link: [u8; 3],
    pub accent: [u8; 3],
}

impl Tok {
    pub fn from(th: &Theme) -> Tok {
        let bg = th.bg;
        let brand = th.engagement;
        Tok {
            light: th.light,
            bg,
            elevated: th.elevated,
            subdued: th.subdued,
            surface: mix(bg, 0.85, th.elevated),
            border: th.border,
            border_subdued: mix(th.border, 0.55, bg),
            text: th.text,
            text_subdued: th.text_subdued,
            text_muted: th.text_muted,
            icon: mix(th.text, 0.85, bg),
            brand,
            brand_strong: mix(brand, 0.85, if th.light { [0, 0, 0] } else { [10, 40, 35] }),
            on_brand: if th.light { [255, 255, 255] } else { mix(brand, 0.12, [0, 0, 0]) },
            correctness: th.correctness,
            clarity: th.clarity,
            engagement: brand,
            delivery: th.delivery,
            plagiarism: mix(th.clarity, 0.5, [128, 243, 243]),
            warning: th.warning,
            link: th.link,
            accent: th.accent,
        }
    }

    pub fn cat(&self, category: &str) -> [u8; 3] {
        match category {
            "correctness" => self.correctness,
            "clarity" => self.clarity,
            "engagement" => self.brand,
            "delivery" => self.delivery,
            _ => self.accent,
        }
    }
    // derived (tokens.css)
    pub fn bg_hover(&self) -> [u8; 3] {
        mix(self.text, 0.05, self.bg)
    }
    pub fn bg_sunken(&self) -> [u8; 3] {
        mix([0, 0, 0], 0.35, self.bg)
    }
    pub fn bg_chip(&self) -> [u8; 3] {
        mix(self.text, 0.08, self.bg)
    }
    pub fn brand_tint(&self) -> [u8; 3] {
        mix(self.brand, 0.18, self.bg)
    }
    pub fn brand_tint_soft(&self) -> [u8; 3] {
        mix(self.brand, 0.09, self.bg)
    }
    pub fn brand_chip(&self) -> [u8; 3] {
        mix(self.brand, 0.22, self.bg)
    }
    pub fn brand_chip_ink(&self) -> [u8; 3] {
        mix(self.brand, 0.65, self.text)
    }
    pub fn brand_subdued(&self) -> [u8; 3] {
        mix(self.brand, 0.30, self.bg)
    }
    pub fn brand_hover(&self) -> [u8; 3] {
        mix(self.brand_strong, 0.85, self.text)
    }
    pub fn warn_bg(&self) -> [u8; 3] {
        mix(self.warning, 0.12, self.bg)
    }
    pub fn warn_border(&self) -> [u8; 3] {
        mix(self.warning, 0.35, self.bg)
    }
    pub fn warn_ink(&self) -> [u8; 3] {
        mix(self.warning, 0.80, self.text)
    }
    pub fn ok_bg(&self) -> [u8; 3] {
        mix(self.brand, 0.14, self.bg)
    }
    pub fn ok_border(&self) -> [u8; 3] {
        mix(self.brand, 0.38, self.bg)
    }
    pub fn ok_ink(&self) -> [u8; 3] {
        mix(self.brand, 0.70, self.text)
    }
    pub fn mark_bg(&self) -> [u8; 3] {
        mix(self.warning, 0.25, self.bg)
    }
    pub fn q_bg(&self) -> [u8; 3] {
        mix(self.warning, 0.30, self.bg)
    }
    pub fn q_ink(&self) -> [u8; 3] {
        mix(self.warning, 0.75, self.text)
    }
    pub fn purple_chip(&self) -> [u8; 3] {
        mix(self.delivery, 0.25, self.bg)
    }
    pub fn purple_chip_ink(&self) -> [u8; 3] {
        mix(self.delivery, 0.70, self.text)
    }
    pub fn glass_border(&self) -> [u8; 3] {
        mix(self.brand, 0.30, self.border_subdued)
    }
    /// --glass: the edge tab, Oumu button and hover card.
    pub fn glass(&self, e: El) -> El {
        e.linear_gradient(2.62, [(0.0, c(mix(self.brand, 0.14, self.surface))), (0.6, c(self.surface)), (1.0, c(self.surface))])
    }
}

// ---------------------------------------------------------------- type

pub fn ts(size: f32, weight: f32) -> TextStyle {
    TextStyle::new(size, weight)
}

pub fn t(s: impl Into<String>, size: f32, color: [u8; 3]) -> El {
    text(s).font(ts(size, 420.0)).color(c(color))
}

pub fn tb(s: impl Into<String>, size: f32, color: [u8; 3]) -> El {
    text(s).font(ts(size, 600.0)).color(c(color))
}

/// Wrapping text that takes the width it's given.
pub fn p(s: impl Into<String>, size: f32, color: [u8; 3]) -> El {
    text(s).wrap().w_full().font(ts(size, 420.0)).color(c(color))
}

// ---------------------------------------------------------------- pieces

pub fn icon(name: &str, color: [u8; 3], size: f32) -> El {
    let px = super::icons::pixels(name, color, (size * 2.0).round() as u32);
    el().size(size, size).pixels(&px)
}

pub enum Btn {
    Primary,
    Ghost,
    Outline,
    Premium,
}

/// .btn and its variants. Chain `.on_press(..)`.
pub fn btn(key: &str, kind: Btn, label: &str, k: &Tok) -> El {
    let b = row().key(key).items_center().gap(8.0).radius(10.0).px(16.0).py(9.0).cursor(rune::Cursor::Pointer).role("button").label(label).focusable(true);
    match kind {
        Btn::Primary => b.fill(c(k.brand_strong)).hover(c(k.brand_hover())).child(tb(label, 14.0, k.on_brand)),
        Btn::Ghost => b.hover(c(k.subdued)).child(tb(label, 14.0, k.text)),
        Btn::Outline => b.radius(22.0).px(16.0).py(10.0).border(1.0, c(k.border)).hover(c(k.subdued)).child(text(label).font(ts(14.0, 500.0)).color(c(k.text))),
        Btn::Premium => b.px(22.0).py(11.0).fill(c(k.warning)).child(tb(label, 16.0, k.bg)),
    }
}

/// A button with an icon before its label.
pub fn btn_icon(key: &str, kind: Btn, ic: &str, ic_color: [u8; 3], label: &str, k: &Tok) -> El {
    let b = btn(key, kind, "", k);
    b.child(icon(ic, ic_color, 16.0)).child(text(label).font(ts(14.0, 500.0)).color(c(k.text)))
}

pub fn disabled(e: El, off: bool) -> El {
    if off { e.dim(0.5).cursor(rune::Cursor::Default) } else { e }
}

/// .card
pub fn card(e: El, k: &Tok) -> El {
    e.w_full().radius(12.0).px(16.0).py(14.0).border(1.0, c(mix(k.border, 0.75, k.bg))).fill(c(mix(k.elevated, 0.72, k.bg)))
}

/// .notice (warning) / .notice--ok
pub fn notice(s: impl Into<String>, ok: bool, k: &Tok) -> El {
    let (bg, bo, ink) = if ok { (k.ok_bg(), k.ok_border(), k.ok_ink()) } else { (k.warn_bg(), k.warn_border(), k.warn_ink()) };
    col().w_full().radius(10.0).px(12.0).py(10.0).fill(c(bg)).border(1.0, c(bo)).child(p(s, 14.0, ink))
}

pub fn section_label(s: &str, k: &Tok) -> El {
    t(s, 14.0, k.text_subdued).pt(18.0).pb(10.0)
}

/// .kv: a label on the left, a value or control on the right, a rule under it.
pub fn kv(left: El, right: El, k: &Tok) -> El {
    row().w_full().items_center().py(8.0).gap(10.0).border_sides([0.0, 0.0, 1.0, 0.0], c(k.border_subdued)).child(left.grow(1.0).min_w(0.0)).child(right)
}

/// A checkbox (the React input type=checkbox in a .kv row).
pub fn check(key: &str, on: bool, k: &Tok) -> El {
    let b = el().key(key).size(18.0, 18.0).radius(5.0).center().border(1.5, c(if on { k.brand } else { k.border })).cursor(rune::Cursor::Pointer).role("checkbox").checked(on);
    if on { b.fill(c(k.brand)).child(icon("check", k.on_brand, 14.0)) } else { b }
}

/// .spin
pub fn spinner(k: &Tok, size: f32) -> El {
    let ring = rune::shape::Shape::new()
        .stroke((0..=48).map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            [size / 2.0 + (size / 2.0 - 2.0) * a.cos(), size / 2.0 + (size / 2.0 - 2.0) * a.sin()]
        }).collect::<Vec<_>>(), 2.5, c(k.border_subdued))
        .stroke((0..=12).map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 / 12.0 * std::f32::consts::FRAC_PI_2;
            [size / 2.0 + (size / 2.0 - 2.0) * a.cos(), size / 2.0 + (size / 2.0 - 2.0) * a.sin()]
        }).collect::<Vec<_>>(), 2.5, c(k.brand));
    rune::canvas(ring).size(size, size).cycle(rune::Cycle { period: 0.8, phase: 0.0, stops: &[(0.0, 1.0), (0.5, 0.45), (1.0, 1.0)], still: 1.0, fps: 30.0 })
}

/// .steps: fake-but-honest progress while a feature runs.
pub fn steps(list: &[&str], active: usize, k: &Tok) -> El {
    let mut c_ = col().gap(8.0).pt(16.0);
    for (i, s) in list.iter().enumerate() {
        let mark = if i < active { icon("check", k.brand, 16.0) } else if i == active { spinner(k, 16.0) } else { el().size(16.0, 16.0) };
        c_ = c_.child(row().gap(10.0).items_center().child(mark).child(t(*s, 14.0, if i <= active { k.text } else { k.text_subdued })));
    }
    c_
}

/// .empty
pub fn empty(ic: &str, title: &str, body: &str, k: &Tok) -> El {
    col().w_full().items_center().pt(60.0).px(20.0).pb(20.0).gap(8.0)
        .child(icon(ic, k.text_muted, 64.0).mb(12.0))
        .child(tb(title, 20.0, k.text))
        .child(text(body).wrap().w_full().font(ts(16.0, 420.0)).color(c(k.text_subdued)).align(rune::Align::Center))
}

/// .hero
pub fn hero(ic: &str, title: &str, body: Option<&str>, k: &Tok) -> El {
    let mut h = col().w_full().pt(28.0).pb(8.0).child(icon(ic, k.text, 56.0)).child(text(title).font(ts(28.0, 600.0)).color(c(k.text)).pt(18.0).pb(14.0));
    if let Some(b) = body {
        h = h.child(p(b, 16.0, k.text_subdued));
    }
    h
}

/// .titlebar with optional action buttons on the right.
pub fn titlebar(title: &str, actions: Vec<El>, k: &Tok) -> El {
    let mut r = row().w_full().h(44.0).items_center().justify_center().child(tb(title, 15.0, k.text));
    if !actions.is_empty() {
        let mut a = row().gap(6.0).absolute(0.0, 8.0).pin(None, Some(8.0), Some(12.0), None);
        for b in actions {
            a = a.child(b);
        }
        r = r.child(a);
    }
    r
}

/// A 28 px square icon button for the title bar.
pub fn title_btn(key: &str, ic: &str, help: &str, k: &Tok) -> El {
    el().key(key).size(28.0, 28.0).radius(8.0).center().hover(c(k.subdued)).cursor(rune::Cursor::Pointer).help(help).role("button").label(help).child(icon(ic, k.icon, 18.0))
}

/// .select: Windows' own menu as the drop-down.
pub fn select<S: 'static>(key: &'static str, options: Vec<(String, String)>, current: &str, k: &Tok, set: impl Fn(&mut S, String) + Clone + 'static) -> El {
    let cur = current.to_string();
    let label = options.iter().find(|(id, _)| id == current).map(|(_, l)| l.clone()).unwrap_or_else(|| current.to_string());
    row().key(key).w_full().items_center().radius(10.0).px(14.0).py(12.0).fill(c(k.bg)).border(1.0, c(k.border)).cursor(rune::Cursor::Pointer).role("button").label(label.clone())
        .child(t(label, 16.0, k.text).grow(1.0).min_w(0.0))
        .child(icon("chevron-down", k.text_subdued, 16.0))
        .on_press_tx(move |_s: &mut S, tx: &mut Tx| {
            let set = set.clone();
            let menu = rune::PopupMenu::picker(options.clone(), &cur);
            tx.popup_menu(menu, key, move |s: &mut S, picked, _| {
                if let Some(id) = picked {
                    set(s, id.to_string());
                }
            });
        })
}

/// .field: a label over a text input.
pub fn field<S: 'static>(key: &str, label: &str, edit: &rune::TextEdit, secret: bool, kit: &rune_kit::Kit, k: &Tok, bind: impl for<'a> Fn(&'a mut S) -> &'a mut rune::TextEdit + 'static) -> El {
    let mut input = rune_kit::TextInput::new(edit).width(None).on_edit(bind);
    if secret {
        input = input.secure();
    }
    col().w_full().gap(6.0).pb(14.0).child(t(label, 13.0, k.text_subdued)).child(input.build(kit).key(key).w_full())
}

/// .field with a placeholder.
pub fn field_ph<S: 'static>(key: &str, label: &str, placeholder: &str, edit: &rune::TextEdit, secret: bool, kit: &rune_kit::Kit, k: &Tok, bind: impl for<'a> Fn(&'a mut S) -> &'a mut rune::TextEdit + 'static) -> El {
    let mut input = rune_kit::TextInput::new(edit).width(None).placeholder(placeholder).on_edit(bind);
    if secret {
        input = input.secure();
    }
    col().w_full().gap(6.0).pb(14.0).child(t(label, 13.0, k.text_subdued)).child(input.build(kit).key(key).w_full())
}

/// .field with a several-line text box (a textarea).
pub fn field_area<S: 'static>(key: &str, label: &str, placeholder: &str, edit: &rune::TextEdit, kit: &rune_kit::Kit, k: &Tok, bind: impl for<'a> Fn(&'a mut S) -> &'a mut rune::TextEdit + 'static) -> El {
    let input = rune_kit::TextArea::new(edit).width(None).auto_grow(4, 10).placeholder(placeholder).on_edit(bind);
    col().w_full().gap(6.0).pb(14.0).child(t(label, 13.0, k.text_subdued)).child(input.build(kit).key(key).w_full())
}

pub fn kit(k: &Tok) -> rune_kit::Kit {
    rune_kit::Kit::from_palette(rune_kit::PaletteSeed { bg: c(k.bg), fg: c(k.text), accent: c(k.brand) })
}

/// Word-level diff (LCS), as lib/text.ts.
pub fn word_diff(a: &str, b: &str) -> Vec<(u8, String)> {
    fn split(s: &str) -> Vec<String> {
        let mut out = vec![];
        let mut cur = String::new();
        let mut ws = None;
        for ch in s.chars() {
            let w = ch.is_whitespace();
            if ws.is_some_and(|x| x != w) {
                out.push(std::mem::take(&mut cur));
            }
            ws = Some(w);
            cur.push(ch);
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }
    let (a, b) = (split(a), split(b));
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let mut out: Vec<(u8, String)> = vec![];
    let mut push = |kind: u8, t: &str| match out.last_mut() {
        Some(l) if l.0 == kind => l.1.push_str(t),
        _ => out.push((kind, t.to_string())),
    };
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            push(0, &a[i]);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            push(1, &a[i]);
            i += 1;
        } else {
            push(2, &b[j]);
            j += 1;
        }
    }
    while i < n {
        push(1, &a[i]);
        i += 1;
    }
    while j < m {
        push(2, &b[j]);
        j += 1;
    }
    out
}

/// A word diff as wrapping text: deletions struck through and muted, insertions bold on a
/// brand tint.
pub fn diff(a: &str, b: &str, size: f32, k: &Tok) -> El {
    let ops = word_diff(a, b);
    let mut full = String::new();
    let mut spans = vec![];
    let mut strikes = vec![];
    for (kind, s) in ops {
        let start = full.len();
        full.push_str(&s);
        let end = full.len();
        match kind {
            1 => {
                spans.push(rune::text::Span::new(start, end).color(c(k.text_muted)));
                strikes.push((start, end, Some(c(k.text_muted))));
            }
            2 => spans.push(rune::text::Span::new(start, end).weight(600.0).bg(c(k.brand_tint()), 1.0)),
            _ => {}
        }
    }
    text(full).wrap().w_full().font(ts(size, 420.0)).color(c(k.text)).spans(spans).strikes(strikes)
}

pub fn cap(s: &str) -> String {
    let mut ch = s.chars();
    ch.next().map(|f| f.to_uppercase().collect::<String>() + ch.as_str()).unwrap_or_default()
}

/// A progress track filled to `frac` (0..1).
pub fn meter(frac: f32, h: f32, color: [u8; 3], k: &Tok) -> El {
    let f = frac.clamp(0.0, 1.0);
    row().w_full().h(h).radius(h / 2.0).clip_rounded(h / 2.0).fill(c(k.border_subdued)).child(el().h_full().grow(f.max(0.0001)).radius(h / 2.0).fill(c(color))).child(el().h_full().grow((1.0 - f).max(0.0001)))
}
