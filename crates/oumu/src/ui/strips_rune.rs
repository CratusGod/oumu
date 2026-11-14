//! The underline strips on Linux: a Rune overlay (a layer-shell surface on Wayland) that
//! takes no pointer input at all, so every click reaches the app below. It draws only
//! coloured bars, positioned in the underline frame's own coordinates.

use super::style::*;
use rune::{Cx, OverlayHandle, OverlayPlacement};

/// One bar: x, y, w, h in px from the frame's top-left, and straight RGBA.
pub type Bar = (i32, i32, i32, i32, [u8; 4]);

#[derive(Default)]
pub struct StripS {
    pub bars: Vec<Bar>,
}

pub fn view(s: &StripS, _: &mut Cx<StripS>) -> El {
    s.bars.iter().enumerate().fold(el().w_full().h_full(), |root, (i, b)| {
        let color = c([b.4[0], b.4[1], b.4[2]]).alpha(b.4[3] as f32 / 255.0);
        root.child(el().key(format!("bar-{i}")).absolute(b.0 as f32, b.1 as f32).size(b.2.max(1) as f32, b.3.max(1) as f32).fill(color))
    })
}

pub struct Strips {
    ov: OverlayHandle<StripS>,
    frame: (i32, i32, i32, i32),
    visible: bool,
    last: Vec<Bar>,
}

impl Strips {
    /// The overlay is made with the app (see `ui::run`); there is nothing to create later.
    pub fn new() -> Option<Strips> {
        None
    }

    pub fn with_overlay(ov: OverlayHandle<StripS>) -> Strips {
        Strips { ov, frame: (0, 0, 0, 0), visible: false, last: vec![] }
    }

    /// Cover the frame (layout px), or hide.
    pub fn place(&mut self, x: i32, y: i32, w: i32, h: i32, visible: bool) {
        let visible = visible && w > 0 && h > 0;
        if (x, y, w, h) != self.frame && visible {
            self.frame = (x, y, w, h);
            self.ov.set_placement(Some(OverlayPlacement::Rect { x: x as f32, y: y as f32, width: w as f32, height: h as f32 }));
        }
        if visible != self.visible {
            self.visible = visible;
            if visible {
                self.ov.show();
            } else {
                self.ov.hide();
            }
        }
    }

    pub fn draw(&mut self, bars: &[Bar]) {
        if bars == self.last.as_slice() {
            return;
        }
        self.last = bars.to_vec();
        let bars = self.last.clone();
        self.ov.update(move |s, _| s.bars = bars);
    }
}
