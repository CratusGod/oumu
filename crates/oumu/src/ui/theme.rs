//! Oumu's colour themes (ported from ui/src/lib/theme.ts) and the Matugen palette.

#[derive(Clone, Debug)]
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub light: bool,
    pub bg: [u8; 3],
    pub elevated: [u8; 3],
    pub subdued: [u8; 3],
    pub border: [u8; 3],
    pub text: [u8; 3],
    pub text_subdued: [u8; 3],
    pub text_muted: [u8; 3],
    pub correctness: [u8; 3],
    pub clarity: [u8; 3],
    pub engagement: [u8; 3],
    pub delivery: [u8; 3],
    pub warning: [u8; 3],
    pub link: [u8; 3],
    pub accent: [u8; 3],
}

fn c(h: &str) -> [u8; 3] {
    let v = u32::from_str_radix(h.trim_start_matches('#'), 16).unwrap_or(0);
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

macro_rules! th {
    ($id:expr, $name:expr, $light:expr, [$bg:expr, $el:expr, $sub:expr, $bo:expr, $t:expr, $ts:expr, $tm:expr, $co:expr, $cl:expr, $en:expr, $de:expr, $wa:expr, $li:expr, $ac:expr]) => {
        Theme { id: $id, name: $name, light: $light, bg: c($bg), elevated: c($el), subdued: c($sub), border: c($bo), text: c($t), text_subdued: c($ts), text_muted: c($tm), correctness: c($co), clarity: c($cl), engagement: c($en), delivery: c($de), warning: c($wa), link: c($li), accent: c($ac) }
    };
}

pub fn all() -> Vec<Theme> {
    vec![
        th!("default", "Default (dark)", false, ["#1c1c1c", "#242424", "#2e2e2e", "#545454", "#ffffff", "#bcbcbc", "#8a8a8a", "#ff7a74", "#7d99f0", "#2cc9b6", "#a598f0", "#ffbf47", "#7ab8ff", "#6b5bd6"]),
        th!("catppuccin-mocha", "Catppuccin Mocha", false, ["#1e1e2e", "#181825", "#313244", "#585b70", "#cdd6f4", "#bac2de", "#7f849c", "#f38ba8", "#89b4fa", "#94e2d5", "#b4befe", "#f9e2af", "#89b4fa", "#fab387"]),
        th!("catppuccin-latte", "Catppuccin Latte", true, ["#eff1f5", "#e6e9ef", "#ccd0da", "#9ca0b0", "#4c4f69", "#5c5f77", "#8c8fa1", "#d20f39", "#1e66f5", "#179299", "#7287fd", "#df8e1d", "#1e66f5", "#fe640b"]),
        th!("nord", "Nord", false, ["#2e3440", "#3b4252", "#434c5e", "#4c566a", "#eceff4", "#d8dee9", "#7b88a1", "#bf616a", "#81a1c1", "#a3be8c", "#b48ead", "#ebcb8b", "#88c0d0", "#5e81ac"]),
        th!("gruvbox-dark", "Gruvbox Dark", false, ["#282828", "#32302f", "#3c3836", "#665c54", "#ebdbb2", "#d5c4a1", "#928374", "#fb4934", "#83a598", "#b8bb26", "#d3869b", "#fe8019", "#83a598", "#d65d0e"]),
        th!("tokyo-night", "Tokyo Night", false, ["#1a1b26", "#1f2335", "#292e42", "#414868", "#c0caf5", "#a9b1d6", "#565f89", "#f7768e", "#7dcfff", "#9ece6a", "#bb9af7", "#e0af68", "#7aa2f7", "#ff9e64"]),
        th!("rose-pine", "Rosé Pine", false, ["#191724", "#1f1d2e", "#26233a", "#524f67", "#e0def4", "#908caa", "#6e6a86", "#eb6f92", "#9ccfd8", "#31748f", "#c4a7e7", "#f6c177", "#c4a7e7", "#f6c177"]),
        th!("purple", "Purple", false, ["#17141f", "#1f1a2b", "#2a2338", "#4a3f63", "#f3efff", "#c6bde0", "#8d84a8", "#fb7185", "#818cf8", "#c084fc", "#e879f9", "#fbbf24", "#a5b4fc", "#8b5cf6"]),
        th!("black", "Black", false, ["#000000", "#0a0a0a", "#161616", "#3a3a3a", "#ffffff", "#b3b3b3", "#7a7a7a", "#ff6b6b", "#74a7ff", "#5eead4", "#c4b5fd", "#fcd34d", "#93c5fd", "#525252"]),
        th!("white", "White", true, ["#ffffff", "#f7f7f7", "#eeeeee", "#c8c8c8", "#111111", "#4d4d4d", "#7a7a7a", "#d13438", "#2f5fd0", "#0b8a6f", "#7a4fd6", "#b7791f", "#2563eb", "#6b5bd6"]),
        th!("orange", "Orange", false, ["#1a1410", "#231b15", "#2e241c", "#5a4636", "#fff4ea", "#d6c3b2", "#9c8676", "#f87171", "#60a5fa", "#fb923c", "#c084fc", "#facc15", "#fdba74", "#ea580c"]),
    ]
}

/// The theme for a settings id; "matugen" uses the wallpaper palette when there is one.
pub fn pick(id: &str, palette: Option<&crate::theme::Palette>) -> Theme {
    if id == "matugen" {
        if let Some(p) = palette {
            let get = |k: &str, d: &str| c(p.get(k).map(String::as_str).unwrap_or(d));
            let bg = get("surface", "#1c1c1c");
            let light = bg[0] as u32 + bg[1] as u32 + bg[2] as u32 > 382;
            return Theme {
                id: "matugen",
                name: "Matugen",
                light,
                bg,
                elevated: get("surface_container", "#242424"),
                subdued: get("surface_container_high", "#2e2e2e"),
                border: get("outline", "#545454"),
                text: get("on_surface", "#ffffff"),
                text_subdued: get("on_surface_variant", "#bcbcbc"),
                text_muted: get("outline", "#8a8a8a"),
                correctness: get("error", "#ff7a74"),
                clarity: get("primary", "#7d99f0"),
                engagement: get("tertiary", "#2cc9b6"),
                delivery: get("secondary", "#a598f0"),
                warning: c("#ffbf47"),
                link: get("primary", "#7ab8ff"),
                accent: get("primary", "#6b5bd6"),
            };
        }
    }
    let mut all = all();
    let i = all.iter().position(|t| t.id == id).unwrap_or(0);
    all.swap_remove(i)
}

impl Theme {
    pub fn category(&self, cat: &str) -> [u8; 3] {
        match cat {
            "correctness" => self.correctness,
            "clarity" => self.clarity,
            "engagement" => self.engagement,
            "delivery" => self.delivery,
            _ => self.accent,
        }
    }

}
