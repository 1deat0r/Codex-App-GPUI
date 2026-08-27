//! Codex Desktop-inspired design tokens. The native surface uses the same
//! visual hierarchy as the reference: quiet dark chrome, one raised content
//! plane, compact navigation rows, and restrained blue highlights.

use std::sync::atomic::{AtomicU8, Ordering};

const fn color(hex: u32) -> gpui::Rgba {
    gpui::Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

static ACTIVE_THEME: AtomicU8 = AtomicU8::new(0);

pub fn set_active(theme: &str) {
    ACTIVE_THEME.store((theme == "light") as u8, Ordering::Relaxed);
}

fn light() -> bool {
    ACTIVE_THEME.load(Ordering::Relaxed) == 1
}

fn variant(dark: gpui::Rgba, light_value: gpui::Rgba) -> gpui::Rgba {
    if light() {
        light_value
    } else {
        dark
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
pub const SIDEBAR_COLLAPSED_WIDTH: f32 = 58.0;
pub const CONTENT_MAX_WIDTH: f32 = 900.0;
pub const COMPOSER_MAX_WIDTH: f32 = 860.0;

pub const NAV_GAP: f32 = 4.0;
pub const ROW_RADIUS: f32 = 7.0;

pub fn text_color(active: bool) -> gpui::Rgba {
    if active {
        text()
    } else {
        text_muted()
    }
}

pub fn bg_base() -> gpui::Rgba {
    variant(BG_BASE, color(0xf7f7f8))
}

pub fn bg_sidebar() -> gpui::Rgba {
    variant(BG_SIDEBAR, color(0xeeeeef))
}

pub fn bg_surface() -> gpui::Rgba {
    variant(BG_SURFACE, color(0xffffff))
}

pub fn bg_surface_2() -> gpui::Rgba {
    variant(BG_SURFACE_2, color(0xf0f0f2))
}

pub fn bg_hover() -> gpui::Rgba {
    variant(BG_HOVER, color(0xe5e5e8))
}

pub fn bg_selected() -> gpui::Rgba {
    variant(BG_SELECTED, color(0xd9e5f8))
}

pub fn border() -> gpui::Rgba {
    variant(BORDER, color(0xd4d4d8))
}

pub fn text() -> gpui::Rgba {
    variant(TEXT, color(0x1c1c1e))
}

pub fn text_muted() -> gpui::Rgba {
    variant(TEXT_MUTED, color(0x5d5d65))
}

pub fn text_faint() -> gpui::Rgba {
    variant(TEXT_FAINT, color(0x767680))
}

pub fn text_disabled() -> gpui::Rgba {
    variant(TEXT_DISABLED, color(0xa1a1a8))
}

pub fn accent() -> gpui::Rgba {
    variant(ACCENT, color(0x2764c8))
}

pub fn accent_soft() -> gpui::Rgba {
    variant(ACCENT_SOFT, color(0xdce8fb))
}

pub fn success() -> gpui::Rgba {
    variant(SUCCESS, color(0x177245))
}

pub fn warning() -> gpui::Rgba {
    variant(WARNING, color(0xa45500))
}

pub fn danger() -> gpui::Rgba {
    variant(DANGER, color(0xb3261e))
}

pub fn user_bubble() -> gpui::Rgba {
    variant(USER_BUBBLE, color(0xe8e8eb))
}

pub fn code_bg() -> gpui::Rgba {
    variant(CODE_BG, color(0xf0f0f2))
}
