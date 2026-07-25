//! Colour palette and visual styling for the TM-2077 — a matte near-black body
//! with a warm amber backlit LCD, modelled on the Korg TM-60.

use egui::Color32;

pub struct Palette;

impl Palette {
    // --- Device body (matte black plastic) ---
    pub const BODY: Color32 = Color32::from_rgb(0x1a, 0x1b, 0x1d);
    pub const BODY_EDGE_HI: Color32 = Color32::from_rgb(0x34, 0x36, 0x3a);
    pub const BODY_EDGE_LO: Color32 = Color32::from_rgb(0x0a, 0x0b, 0x0c);
    pub const BODY_LABEL: Color32 = Color32::from_rgb(0xcc, 0xd0, 0xd4);
    pub const BODY_LABEL_DIM: Color32 = Color32::from_rgb(0x74, 0x79, 0x7e);

    // --- LCD (amber backlight, dark segments) ---
    pub const LCD_BG: Color32 = Color32::from_rgb(0xf6, 0xac, 0x1e);
    pub const LCD_BG_EDGE: Color32 = Color32::from_rgb(0xe2, 0x93, 0x10);
    pub const LCD_INK: Color32 = Color32::from_rgb(0x2b, 0x1c, 0x04);
    pub const LCD_INK_DIM: Color32 = Color32::from_rgba_premultiplied(0x2b, 0x1c, 0x04, 0x24);
    pub const LCD_FRAME: Color32 = Color32::from_rgb(0x53, 0x56, 0x5a);
    pub const BEZEL: Color32 = Color32::from_rgb(0x08, 0x09, 0x0a);

    // --- LEDs ---
    pub const LED_RED_ON: Color32 = Color32::from_rgb(0xff, 0x44, 0x33);
    pub const LED_RED_OFF: Color32 = Color32::from_rgb(0x47, 0x1c, 0x18);
    pub const LED_GREEN_ON: Color32 = Color32::from_rgb(0x4c, 0xe6, 0x60);
    pub const LED_GREEN_OFF: Color32 = Color32::from_rgb(0x1b, 0x4a, 0x25);

    // --- Buttons (grey rubber) ---
    pub const BTN: Color32 = Color32::from_rgb(0x3b, 0x3f, 0x44);
    pub const BTN_HI: Color32 = Color32::from_rgb(0x4d, 0x52, 0x58);
    pub const BTN_LO: Color32 = Color32::from_rgb(0x22, 0x24, 0x27);
    pub const BTN_LABEL: Color32 = Color32::from_rgb(0xd4, 0xd8, 0xdc);
    pub const BTN_ON: Color32 = Color32::from_rgb(0xf6, 0xac, 0x1e);

    // --- TAP TEMPO (red) ---
    pub const TAP: Color32 = Color32::from_rgb(0xe6, 0x43, 0x3f);
    pub const TAP_HI: Color32 = Color32::from_rgb(0xf4, 0x6d, 0x66);
    pub const TAP_LO: Color32 = Color32::from_rgb(0xb0, 0x2c, 0x2a);
}

/// Apply global egui style tweaks shared across the app (both themes).
pub fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.visuals.panel_fill = Color32::from_rgb(0x0a, 0x0b, 0x0b);
        style.visuals.override_text_color = Some(Palette::BODY_LABEL);
    });
}
