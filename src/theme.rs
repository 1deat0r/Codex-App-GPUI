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

const fn color_with_alpha(hex: u32, alpha: f32) -> gpui::Rgba {
    gpui::Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: alpha,
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

/// The reference Electron surface is transparent when the application menu
/// chrome is not active. Keep opaque fallbacks here for cards and light mode,
/// while the public accessors below reproduce the dark Electron compositing.
pub const BG_BASE: gpui::Rgba = color(0x181818);
pub const BG_SIDEBAR: gpui::Rgba = color(0x212121);
pub const BG_SURFACE: gpui::Rgba = color(0x181818);
pub const BG_SURFACE_2: gpui::Rgba = color(0x2d2d2d);
pub const BG_HOVER: gpui::Rgba = color(0x303030);
pub const BG_SELECTED: gpui::Rgba = color(0x383838);
pub const BORDER: gpui::Rgba = color(0x343434);
pub const TEXT: gpui::Rgba = color(0xdfdfdf);
pub const TEXT_MUTED: gpui::Rgba = color(0xb3b3b3);
pub const TEXT_FAINT: gpui::Rgba = color(0x8f8f8f);
pub const TEXT_DISABLED: gpui::Rgba = color(0x5e5e5e);
pub const ACCENT: gpui::Rgba = color(0x339cff);
pub const ACCENT_SOFT: gpui::Rgba = color(0x013566);
pub const SUCCESS: gpui::Rgba = color(0x40c977);
pub const WARNING: gpui::Rgba = color(0xff8549);
pub const DANGER: gpui::Rgba = color(0xff6764);
pub const USER_BUBBLE: gpui::Rgba = color(0x252525);
pub const CODE_BG: gpui::Rgba = color(0x0d0d0d);

pub const SIDEBAR_WIDTH: f32 = 280.0;
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
    variant(color_with_alpha(0x181818, 0.0), color(0xf7f7f8))
}

pub fn bg_sidebar() -> gpui::Rgba {
    variant(color_with_alpha(0x212121, 0.70), color(0xeeeeef))
}

pub fn bg_surface() -> gpui::Rgba {
    variant(color(0x181818), color(0xffffff))
}

pub fn bg_surface_2() -> gpui::Rgba {
    variant(color_with_alpha(0xffffff, 0.03), color(0xf0f0f2))
}

pub fn bg_hover() -> gpui::Rgba {
    variant(color_with_alpha(0xffffff, 0.08), color(0xe5e5e8))
}

pub fn bg_selected() -> gpui::Rgba {
    variant(color_with_alpha(0xffffff, 0.12), color(0xd9e5f8))
}

pub fn border() -> gpui::Rgba {
    variant(color_with_alpha(0xffffff, 0.08), color(0xd4d4d8))
}

pub fn text() -> gpui::Rgba {
    variant(color(0xdfdfdf), color(0x1c1c1e))
}

pub fn text_muted() -> gpui::Rgba {
    variant(color_with_alpha(0xffffff, 0.70), color(0x5d5d65))
}

pub fn text_faint() -> gpui::Rgba {
    variant(color_with_alpha(0xffffff, 0.50), color(0x767680))
}

pub fn text_disabled() -> gpui::Rgba {
    variant(color_with_alpha(0xffffff, 0.30), color(0xa1a1a8))
}

pub fn accent() -> gpui::Rgba {
    variant(color(0x339cff), color(0x2764c8))
}

pub fn accent_soft() -> gpui::Rgba {
    variant(color_with_alpha(0x339cff, 0.16), color(0xdce8fb))
}

pub fn success() -> gpui::Rgba {
    variant(color(0x40c977), color(0x177245))
}

pub fn warning() -> gpui::Rgba {
    variant(color(0xff8549), color(0xa45500))
}

pub fn danger() -> gpui::Rgba {
    variant(color(0xff6764), color(0xb3261e))
}

pub fn user_bubble() -> gpui::Rgba {
    variant(color_with_alpha(0xffffff, 0.05), color(0xe8e8eb))
}

pub fn code_bg() -> gpui::Rgba {
    variant(color(0x0d0d0d), color(0xf0f0f2))
}
