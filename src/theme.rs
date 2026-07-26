//! Colour palette and visual styling for the TM-2077. Two variants — a dark
//! matte-black body and a light silver body — both with the warm amber backlit
//! LCD of the Korg TM-60. The active `Palette` is stashed in egui's context data
//! each frame so the (painter-only) drawing code can read it without threading a
//! parameter through every function.

use egui::Color32;
use serde::{Deserialize, Serialize};

/// Which visual theme the device wears.
#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    pub fn palette(self) -> Palette {
        match self {
            Theme::Dark => Palette::dark(),
            Theme::Light => Palette::light(),
        }
    }
}

/// The full set of device colours for one theme.
#[derive(Clone, Copy)]
pub struct Palette {
    /// Background behind the device.
    pub panel: Color32,

    // --- Device body ---
    pub body: Color32,
    pub body_edge_hi: Color32,
    pub body_edge_lo: Color32,
    pub body_label: Color32,
    pub body_label_dim: Color32,

    // --- LCD (amber backlight, dark segments) ---
    pub lcd_bg: Color32,
    pub lcd_bg_edge: Color32,
    pub lcd_ink: Color32,
    pub lcd_ink_dim: Color32,
    pub lcd_frame: Color32,
    pub bezel: Color32,

    // --- LEDs ---
    pub led_red_on: Color32,
    pub led_red_off: Color32,
    pub led_green_on: Color32,
    pub led_green_off: Color32,

    // --- Buttons (rubber) ---
    pub btn: Color32,
    pub btn_hi: Color32,
    pub btn_lo: Color32,
    pub btn_label: Color32,
    pub btn_on: Color32,

    // --- TAP TEMPO (red) ---
    pub tap: Color32,
    pub tap_hi: Color32,
    pub tap_lo: Color32,
}

impl Palette {
    /// Matte-black body, the original look.
    pub fn dark() -> Self {
        Self {
            panel: Color32::from_rgb(0x0a, 0x0b, 0x0b),

            body: Color32::from_rgb(0x1a, 0x1b, 0x1d),
            body_edge_hi: Color32::from_rgb(0x34, 0x36, 0x3a),
            body_edge_lo: Color32::from_rgb(0x0a, 0x0b, 0x0c),
            body_label: Color32::from_rgb(0xcc, 0xd0, 0xd4),
            body_label_dim: Color32::from_rgb(0x74, 0x79, 0x7e),

            lcd_bg: Color32::from_rgb(0xf6, 0xac, 0x1e),
            lcd_bg_edge: Color32::from_rgb(0xe2, 0x93, 0x10),
            lcd_ink: Color32::from_rgb(0x2b, 0x1c, 0x04),
            lcd_ink_dim: Color32::from_rgba_premultiplied(0x2b, 0x1c, 0x04, 0x24),
            lcd_frame: Color32::from_rgb(0x53, 0x56, 0x5a),
            bezel: Color32::from_rgb(0x08, 0x09, 0x0a),

            led_red_on: Color32::from_rgb(0xff, 0x44, 0x33),
            led_red_off: Color32::from_rgb(0x47, 0x1c, 0x18),
            led_green_on: Color32::from_rgb(0x4c, 0xe6, 0x60),
            led_green_off: Color32::from_rgb(0x1b, 0x4a, 0x25),

            btn: Color32::from_rgb(0x3b, 0x3f, 0x44),
            btn_hi: Color32::from_rgb(0x4d, 0x52, 0x58),
            btn_lo: Color32::from_rgb(0x22, 0x24, 0x27),
            btn_label: Color32::from_rgb(0xd4, 0xd8, 0xdc),
            btn_on: Color32::from_rgb(0xf6, 0xac, 0x1e),

            tap: Color32::from_rgb(0xe6, 0x43, 0x3f),
            tap_hi: Color32::from_rgb(0xf4, 0x6d, 0x66),
            tap_lo: Color32::from_rgb(0xb0, 0x2c, 0x2a),
        }
    }

    /// Light silver body. The amber LCD, LEDs and red TAP button are unchanged —
    /// only the body, buttons and labels flip to a light scheme.
    pub fn light() -> Self {
        let dark = Self::dark();
        Self {
            panel: Color32::from_rgb(0xbe, 0xc1, 0xc5),

            body: Color32::from_rgb(0xd7, 0xd9, 0xdc),
            body_edge_hi: Color32::from_rgb(0xf2, 0xf4, 0xf6),
            body_edge_lo: Color32::from_rgb(0xa4, 0xa7, 0xab),
            body_label: Color32::from_rgb(0x2a, 0x2c, 0x2f),
            body_label_dim: Color32::from_rgb(0x7a, 0x7d, 0x82),

            btn: Color32::from_rgb(0xc3, 0xc6, 0xca),
            btn_hi: Color32::from_rgb(0xdd, 0xe0, 0xe4),
            btn_lo: Color32::from_rgb(0x9a, 0x9d, 0xa2),
            btn_label: Color32::from_rgb(0x30, 0x32, 0x36),
            btn_on: dark.btn_on,

            // Everything else (LCD, LEDs, TAP, bezel/frame) stays as the dark set.
            ..dark
        }
    }
}

/// egui id under which the active palette is stashed in context data.
fn palette_id() -> egui::Id {
    egui::Id::new("tm2077_palette")
}

/// Read the active palette for painting (defaults to dark before the first
/// `apply`).
pub fn palette(p: &egui::Painter) -> Palette {
    p.ctx()
        .data(|d| d.get_temp::<Palette>(palette_id()))
        .unwrap_or_else(Palette::dark)
}

/// Apply `theme` for this frame: stash its palette for the drawing code and set
/// the egui base visuals (panel background, default text colour).
pub fn apply(ctx: &egui::Context, theme: Theme) {
    let pal = theme.palette();
    ctx.data_mut(|d| d.insert_temp(palette_id(), pal));
    ctx.all_styles_mut(|style| {
        style.visuals.panel_fill = pal.panel;
        style.visuals.override_text_color = Some(pal.body_label);
    });
}
