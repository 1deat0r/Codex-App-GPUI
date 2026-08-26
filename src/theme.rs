//! Codex Desktop-inspired design tokens. The native surface uses the same
//! visual hierarchy as the reference: quiet dark chrome, one raised content
//! plane, compact navigation rows, and restrained blue highlights.

const fn color(hex: u32) -> gpui::Rgba {
    gpui::Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

pub const BG_BASE: gpui::Rgba = color(0x171717);
pub const BG_SIDEBAR: gpui::Rgba = color(0x1b1b1b);
pub const BG_SURFACE: gpui::Rgba = color(0x242424);
pub const BG_SURFACE_2: gpui::Rgba = color(0x2a2a2a);
pub const BG_HOVER: gpui::Rgba = color(0x303030);
pub const BG_SELECTED: gpui::Rgba = color(0x383838);
pub const BORDER: gpui::Rgba = color(0x343434);
pub const TEXT: gpui::Rgba = color(0xf1f1f1);
pub const TEXT_MUTED: gpui::Rgba = color(0xb6b6b6);
pub const TEXT_FAINT: gpui::Rgba = color(0x7e7e7e);
pub const TEXT_DISABLED: gpui::Rgba = color(0x5e5e5e);
pub const ACCENT: gpui::Rgba = color(0x8ab4ff);
pub const ACCENT_SOFT: gpui::Rgba = color(0x30466a);
pub const SUCCESS: gpui::Rgba = color(0x82d9aa);
pub const WARNING: gpui::Rgba = color(0xffb86b);
pub const DANGER: gpui::Rgba = color(0xff8585);
pub const USER_BUBBLE: gpui::Rgba = color(0x303030);
pub const CODE_BG: gpui::Rgba = color(0x111111);

pub const SIDEBAR_WIDTH: f32 = 318.0;
pub const CONTENT_MAX_WIDTH: f32 = 900.0;
pub const COMPOSER_MAX_WIDTH: f32 = 860.0;

pub const NAV_GAP: f32 = 4.0;
pub const ROW_RADIUS: f32 = 7.0;

pub fn text_color(active: bool) -> gpui::Rgba {
    if active {
        TEXT
    } else {
        TEXT_MUTED
    }
}
