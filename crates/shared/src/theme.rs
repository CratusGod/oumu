//! Colour themes: Utsuri's own blue glass plus seven dark schemes from the Ryoku colour
//! scheme catalogue (imported from noctalia-colorschemes, MIT). Ported from utsuri/themes.py.
//!
//! A scheme's primary/secondary/tertiary become the accents and the glass rim, and its
//! surface colour tints the glass.

/// An sRGB colour with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const fn hex(v: u32) -> Self {
        Rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 255)
    }
    pub const fn with_alpha(self, a: u8) -> Self {
        Rgba(self.0, self.1, self.2, a)
    }
    /// Linear blend towards `other` by `t` (0..=1), alpha included.
    pub fn mix(self, other: Rgba, t: f32) -> Rgba {
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgba(m(self.0, other.0), m(self.1, other.1), m(self.2, other.2), m(self.3, other.3))
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: &'static str,
    /// Gradients run a -> b -> c (primary buttons, sliders, checkboxes).
    pub accent_a: Rgba,
    pub accent_b: Rgba,
    pub accent_c: Rgba,
    /// Tints the glass.
    pub surface: Rgba,
    /// Menus, tooltips, drop-down lists.
    pub surface_variant: Rgba,
    pub text: Rgba,
    pub text_dim: Rgba,
    /// A toggled button (e.g. Text / Image).
    pub check_a: Rgba,
    pub check_b: Rgba,
    /// The glass rim's colour ring; first == last.
    pub rim: &'static [Rgba],
}

pub const DEFAULT: &str = "Default";

macro_rules! ryoku {
    ($name:expr, $p:expr, $s:expr, $t:expr, $e:expr, $surf:expr, $var:expr, $on:expr, $onv:expr) => {
        Theme {
            name: $name,
            accent_a: Rgba::hex($p),
            accent_b: Rgba::hex($s),
            accent_c: Rgba::hex($t),
            surface: Rgba::hex($surf),
            surface_variant: Rgba::hex($var),
            text: Rgba::hex($on),
            text_dim: Rgba::hex($onv),
            check_a: Rgba::hex($p),
            check_b: Rgba::hex($e),
            rim: const { &[Rgba::hex($p), Rgba::hex($s), Rgba::hex($t), Rgba::hex($e), Rgba::hex($p)] },
        }
    };
}

/// All themes, Default first.
pub fn all() -> Vec<Theme> {
    vec![
        Theme {
            name: DEFAULT,
            accent_a: Rgba::hex(0x5ac8ff),
            accent_b: Rgba::hex(0x2f7bff),
            accent_c: Rgba::hex(0x5b5bff),
            surface: Rgba::hex(0x161222),
            surface_variant: Rgba::hex(0x2a2438),
            text: Rgba::hex(0xffffff),
            text_dim: Rgba(255, 255, 255, 173), // rgba(255,255,255,0.68)
            check_a: Rgba(255, 138, 92, 217),
            check_b: Rgba(255, 79, 139, 217),
            rim: const { &[
                Rgba::hex(0xff6b9d),
                Rgba::hex(0xffb36b),
                Rgba::hex(0xffe66b),
                Rgba::hex(0x7dffb0),
                Rgba::hex(0x6bd5ff),
                Rgba::hex(0x8a7bff),
                Rgba::hex(0xe07bff),
                Rgba::hex(0xff6b9d),
            ] },
        },
        // name, primary, secondary, tertiary, error, surface, surface variant, on surface, on variant
        ryoku!("Catppuccin Macchiato", 0xb7bdf8, 0xf0c6c6, 0x8bd5ca, 0xed8796, 0x24273a, 0x363a4f, 0xcad3f5, 0xb8c0e0),
        ryoku!("Tokyo Night Storm", 0x7aa2f7, 0xbb9af7, 0x73daca, 0xf7768e, 0x24283b, 0x343b58, 0xa9b1d6, 0x9aa5ce),
        ryoku!("Gruvbox Dark Hard", 0x83a598, 0xb8bb26, 0x8ec07c, 0xfb4934, 0x1d2021, 0x3c3836, 0xebdbb2, 0xd5c4a1),
        ryoku!("Rosé Pine Moon", 0xea9a97, 0x9ccfd8, 0x3e8fb0, 0xeb6f92, 0x232136, 0x393552, 0xe0def4, 0x908caa),
        ryoku!("Everforest Dark Hard", 0xa7c080, 0x7fbbb3, 0x83c092, 0xe67e80, 0x1e2326, 0x2e383c, 0xd3c6aa, 0x9da9a0),
        ryoku!("Kanagawa Dragon", 0x8a9a7b, 0x8ea4a2, 0xc4746e, 0xc4746e, 0x181616, 0x282727, 0xc5c9c5, 0xc8c093),
        ryoku!("Synthwave 84", 0xff536c, 0x7f9fff, 0xc080ff, 0xff536c, 0x18191f, 0x2a2b32, 0xc4c4d6, 0xc4c4d6),
    ]
}

/// The theme called `name`; an unknown name (an old or hand-edited setting) means Default.
pub fn by_name(name: &str) -> Theme {
    let mut all = all();
    let i = all.iter().position(|t| t.name == name).unwrap_or(0);
    all.swap_remove(i)
}

/// Default's glass colour; call sites tint relative to it.
const BASE: Rgba = Rgba(22, 18, 34, 255);

impl Theme {
    pub fn is_default(&self) -> bool {
        self.name == DEFAULT
    }

    /// The glass colour a call site asked for, moved onto this theme's surface.
    pub fn tint(&self, c: Rgba) -> Rgba {
        if self.is_default() {
            return c;
        }
        let m = |s: u8, v: u8, o: u8| (s as i32 + v as i32 - o as i32).clamp(0, 255) as u8;
        Rgba(m(self.surface.0, c.0, BASE.0), m(self.surface.1, c.1, BASE.1), m(self.surface.2, c.2, BASE.2), c.3)
    }

    /// Menu label colour on dark glass: white by default, the theme's text colour otherwise.
    pub fn label(&self) -> Rgba {
        if self.is_default() { Rgba(255, 255, 255, 255) } else { self.text }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_themes_default_first_rims_closed() {
        let all = all();
        assert_eq!(all.len(), 8);
        assert_eq!(all[0].name, DEFAULT);
        for t in &all {
            assert_eq!(t.rim.first(), t.rim.last(), "{} rim must be a closed ring", t.name);
        }
    }

    #[test]
    fn unknown_name_is_default() {
        assert_eq!(by_name("nope").name, DEFAULT);
        assert_eq!(by_name("Tokyo Night Storm").accent_a, Rgba::hex(0x7aa2f7));
    }

    #[test]
    fn tint_keeps_default_and_moves_others() {
        let c = Rgba(30, 24, 44, 200);
        assert_eq!(by_name(DEFAULT).tint(c), c);
        // Gruvbox surface 1d2021 + (c - base)
        assert_eq!(by_name("Gruvbox Dark Hard").tint(c), Rgba(0x1d + 8, 0x20 + 6, 0x21 + 10, 200));
    }
}
