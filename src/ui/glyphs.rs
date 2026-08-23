//! Small vector glyphs drawn with the painter (the default font lacks musical
//! symbols): rocker arrows, ♭ / ♯ accidentals, and a gear.

use egui::{Color32, Pos2, Shape, Stroke, pos2, vec2};

/// A solid triangle arrow, pointing up or down, centred at `c`.
pub fn arrow(p: &egui::Painter, c: Pos2, size: f32, up: bool, color: Color32) {
    let dy = if up { -1.0 } else { 1.0 };
    let pts = vec![
        c + vec2(0.0, dy * size),
        c + vec2(-size, -dy * size * 0.8),
        c + vec2(size, -dy * size * 0.8),
    ];
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A gear / cog icon (settings), centred at `c` with outer `radius`.
pub fn gear(p: &egui::Painter, c: Pos2, radius: f32, color: Color32) {
    const TEETH: usize = 8;
    let rim = radius * 0.62;
    let sw = (radius * 0.16).max(1.3);
    let tw = sw * 2.0; // tooth width
    for k in 0..TEETH {
        let a = k as f32 / TEETH as f32 * std::f32::consts::TAU;
        let d = vec2(a.cos(), a.sin());
        rounded_bar(p, c + d * rim, c + d * radius, tw, tw * 0.35, color);
    }
    // Thicken inward: shrink the radius by half the width so the outer edge
    // stays at `rim` (where the teeth meet it) and the ring grows toward centre.
    let ring_w = sw * 1.7;
    p.circle_stroke(c, rim - ring_w * 0.5, Stroke::new(ring_w, color));
}

/// A filled bar (rotated rectangle) from centre-line `a` to `b`, width `w`, with
/// its four corners rounded by radius `r`.
fn rounded_bar(p: &egui::Painter, a: Pos2, b: Pos2, w: f32, r: f32, color: Color32) {
    let axis = b - a;
    let len = axis.length();
    if len <= 1e-4 {
        return;
    }
    let u = axis / len; // along the bar
    let v = egui::vec2(-u.y, u.x); // across it
    let hw = w * 0.5;
    let r = r.min(hw).min(len * 0.5);
    // Four corners as (arc centre, inward-edge normal, outward-edge normal).
    let corners = [
        (b - u * r + v * (hw - r), u, v),
        (a + u * r + v * (hw - r), v, -u),
        (a + u * r - v * (hw - r), -u, -v),
        (b - u * r - v * (hw - r), -v, u),
    ];
    const A: usize = 3;
    let mut pts = Vec::with_capacity((A + 1) * 4);
    for (center, n_in, n_out) in corners {
        for i in 0..=A {
            let th = (i as f32 / A as f32) * std::f32::consts::FRAC_PI_2;
            pts.push(center + (n_in * th.cos() + n_out * th.sin()) * r);
        }
    }
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A solid triangle marker pointing down (used on the tuner arc).
pub fn marker_down(p: &egui::Painter, c: Pos2, size: f32, color: Color32) {
    let pts = vec![
        c + vec2(0.0, size),
        c + vec2(-size, -size * 0.7),
        c + vec2(size, -size * 0.7),
    ];
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// A flat (♭) glyph centred at `c`; `h` is the half-height. A thin tall stem
/// with a hollow bowl that has calligraphic weight — the loop swells at its
/// belly and tapers where it meets the stem, enclosing an open hole.
pub fn flat(p: &egui::Painter, c: Pos2, h: f32, color: Color32) {
    flat_bowl(p, c, h, color, 1.0);
}

/// A half-flat (demiflat): the flat sign mirrored horizontally (bowl on the
/// left, stem on the right).
pub fn half_flat(p: &egui::Painter, c: Pos2, h: f32, color: Color32) {
    flat_bowl(p, c, h, color, -1.0);
}

/// The flat glyph, with `sign` = 1.0 for a normal flat or -1.0 for a mirrored
/// (half-flat) one.
fn flat_bowl(p: &egui::Painter, c: Pos2, h: f32, color: Color32, sign: f32) {
    let w = h * 0.85;
    let stem_w = (h * 0.15).max(1.1);
    let stem_x = c.x - sign * w * 0.3;
    // Tall, thin vertical stem.
    p.line_segment(
        [pos2(stem_x, c.y - h), pos2(stem_x, c.y + h * 0.9)],
        Stroke::new(stem_w, color),
    );

    // Bowl outline as a cubic bézier from the mid-stem, out to the side and back
    // to a point low on the stem. Sampled into overlapping dots whose radius
    // swells at the belly and tapers to the stem, so the loop is heavier in the
    // middle and leaves an open hole against the stem.
    let (p0, p1, p2, p3) = (
        pos2(stem_x, c.y - h * 0.1),
        pos2(stem_x + sign * w * 1.15, c.y - h * 0.15),
        pos2(stem_x + sign * w * 0.95, c.y + h * 0.55),
        pos2(stem_x, c.y + h * 0.9),
    );
    let (r_min, r_max) = (stem_w * 0.3, stem_w * 0.8);
    const N: usize = 32;
    for i in 0..=N {
        let t = i as f32 / N as f32;
        let mt = 1.0 - t;
        let (b0, b1, b2, b3) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
        let at = pos2(
            b0 * p0.x + b1 * p1.x + b2 * p2.x + b3 * p3.x,
            b0 * p0.y + b1 * p1.y + b2 * p2.y + b3 * p3.y,
        );
        // Thin at both stem attaches (t=0,1), heaviest across the belly.
        let r = r_min + (r_max - r_min) * (std::f32::consts::PI * t).sin();
        p.circle_filled(at, r, color);
    }
}

/// A sharp (♯) glyph centred at `c`; `h` is the half-height. Two upright bars
/// crossed by two thicker upward-slanting bars, both pairs overshooting the
/// crossing as in a real music sharp.
pub fn sharp(p: &egui::Painter, c: Pos2, h: f32, color: Color32) {
    sharp_bars(p, c, h, color, false);
}

/// A half-sharp (demisharp): the sharp sign with a single (central) vertical bar.
pub fn half_sharp(p: &egui::Painter, c: Pos2, h: f32, color: Color32) {
    sharp_bars(p, c, h, color, true);
}

/// The sharp glyph, shared by the full and half variants. `single` draws the
/// half-sharp — one central upright and a narrower, less-slanted body — while the
/// full sharp gets two flanking uprights; the two rising crossbars are common.
fn sharp_bars(p: &egui::Painter, c: Pos2, h: f32, color: Color32, single: bool) {
    // Horizontals are distinctly heavier than the uprights, as in a real sharp.
    let v_stroke = Stroke::new((h * 0.11).max(1.0), color);
    let h_stroke = Stroke::new((h * 0.30).max(1.8), color);
    // The half-sharp is narrower and its crossbars slant less.
    let (w, slant) = if single {
        (h * 0.28, h * 0.09)
    } else {
        (h * 0.5, h * 0.16)
    };

    // Vertical bar(s), overshooting top and bottom.
    let upright = |dx: f32| {
        p.line_segment(
            [
                pos2(c.x + dx, c.y - h * 1.05),
                pos2(c.x + dx, c.y + h * 1.05),
            ],
            v_stroke,
        );
    };
    if single {
        upright(0.0);
    } else {
        upright(-h * 0.25);
        upright(h * 0.25);
    }

    // Two rising horizontal bars.
    for dy in [-h * 0.36, h * 0.36] {
        p.line_segment(
            [
                pos2(c.x - w, c.y + dy + slant),
                pos2(c.x + w, c.y + dy - slant),
            ],
            h_stroke,
        );
    }
}
