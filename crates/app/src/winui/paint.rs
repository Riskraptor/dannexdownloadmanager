//! Painting by the pixel: fills, lines, shaded fills, etched lines, group boxes, and the states a
//! control takes its look from (`State`).

use super::*;

/// How an era theme's glossy parts shade (#45): Windows XP's soft fall from white, Windows 7's
/// two halves, Mac OS X's gel (a bright top, a dark middle, light coming back at the bottom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shine {
    Luna,
    Aero,
    Aqua,
}

impl Shine {
    /// The theme's shine, from its button style; None for the flat looks.
    pub fn of_theme() -> Option<Self> {
        Self::of_button(crate::theme::current().controls.button)
    }

    pub(super) fn of_button(look: crate::palette::ButtonLook) -> Option<Self> {
        use crate::palette::ButtonLook as B;
        match look {
            B::Fluent | B::Breeze => None,
            B::Luna => Some(Shine::Luna),
            B::Aero => Some(Shine::Aero),
            B::Aqua => Some(Shine::Aqua),
        }
    }

    /// A face of colour `f` shaded toward the theme's shine `g` (`gloss`), top to bottom, for
    /// [`fill_shaded`].
    pub fn stops(self, f: Color32, g: Color32) -> [(f32, Color32); 4] {
        use crate::derive::mix;
        match self {
            Shine::Luna => [
                (0.0, mix(f, g, 0.85)),
                (0.4, mix(f, g, 0.6)),
                (0.85, mix(f, g, 0.2)),
                (1.0, f),
            ],
            Shine::Aero => [
                (0.0, mix(f, g, 0.75)),
                (0.5, mix(f, g, 0.5)),
                (0.5, mix(f, g, 0.25)),
                (1.0, f),
            ],
            Shine::Aqua => [
                (0.0, mix(f, g, 0.85)),
                (0.5, mix(f, g, 0.4)),
                (0.5, f),
                (1.0, mix(f, g, 0.45)),
            ],
        }
    }
}

// ---- control states -------------------------------------------------------------------------

/// What a control shows this frame: at rest, under the pointer (hot), with the mouse button held
/// on it (pressed), with its list or menu open, or disabled. Every control takes its look from
/// one of these, so they all answer the pointer the same way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Normal,
    Hot,
    Pressed,
    Open,
    Disabled,
}

impl State {
    /// From the control's response this frame.
    pub fn of(resp: &Response, enabled: bool) -> Self {
        if !enabled {
            State::Disabled
        } else if resp.is_pointer_button_down_on() && resp.contains_pointer() {
            // Held, it looks pressed only while the pointer is on it (as in Windows: moved
            // off, it lets go without a click).
            State::Pressed
        } else if resp.hovered() {
            State::Hot
        } else {
            State::Normal
        }
    }

    /// Shown open while `open` (a combo's list, a drop-down's menu); a press still shows.
    pub fn open_if(self, open: bool) -> Self {
        if open && matches!(self, State::Normal | State::Hot) {
            State::Open
        } else {
            self
        }
    }
}

/// Fill an exact pixel rect with no anti-aliasing (egui feathers `rect_filled` by half a pixel,
/// which erases 1 × 1 rects and softens 1 px lines).
pub fn solid(p: &Painter, rc: Rect, c: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.add_colored_rect(snap(p.pixels_per_point(), rc), c);
    p.add(egui::Shape::mesh(mesh));
}

/// Fills `rc`, its corners rounded by `radius`, shading from top to bottom through `stops`
/// (each a place in the height, 0 at the top and 1 at the bottom, and its colour, in order).
/// Two stops at one place make a sharp change (Windows 7's glossy halves). The era themes'
/// buttons, boxes, bars and title bars are drawn with it (#45).
pub fn fill_shaded(p: &Painter, rc: Rect, radius: f32, stops: &[(f32, Color32)]) {
    fill_shaded_corners(p, rc, radius, radius, stops);
}

/// [`fill_shaded`] with the top corners rounded by `top` and the bottom ones by `bottom` (a
/// button hung from a title bar's edge).
pub fn fill_shaded_corners(p: &Painter, rc: Rect, top: f32, bottom: f32, stops: &[(f32, Color32)]) {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return;
    };
    if rc.width() <= 0.0 || rc.height() <= 0.0 {
        return;
    }
    let most = rc.width().min(rc.height()) / 2.0;
    let (rt, rb) = (top.clamp(0.0, most), bottom.clamp(0.0, most));
    // How far in from the sides the shape is at `y`: its corners' curve.
    let inset = |y: f32| {
        let (r, d) = if y < rc.top() + rt {
            (rt, rc.top() + rt - y)
        } else if y > rc.bottom() - rb {
            (rb, y - (rc.bottom() - rb))
        } else {
            return 0.0;
        };
        r - (r * r - d * d).max(0.0).sqrt()
    };
    let lerp = |a: Color32, b: Color32, t: f32| {
        let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        Color32::from_rgba_premultiplied(
            m(a.r(), b.r()),
            m(a.g(), b.g()),
            m(a.b(), b.b()),
            m(a.a(), b.a()),
        )
    };
    let mut all = Vec::with_capacity(stops.len() + 2);
    all.push((0.0, first.1));
    all.extend(stops.iter().map(|&(t, c)| (t.clamp(0.0, 1.0), c)));
    all.push((1.0, last.1));
    let h = rc.height();
    let mut mesh = egui::Mesh::default();
    for w in all.windows(2) {
        let ((t0, c0), (t1, c1)) = (w[0], w[1]);
        if t1 <= t0 {
            continue;
        }
        let (y0, y1) = (rc.top() + t0 * h, rc.top() + t1 * h);
        // A row a pixel high where a corner curves, else the band whole.
        let mut ys = vec![y0];
        let mut y = y0.floor() + 1.0;
        while y < y1 {
            if y < rc.top() + rt || y > rc.bottom() - rb {
                ys.push(y);
            }
            y += 1.0;
        }
        for edge in [rc.top() + rt, rc.bottom() - rb] {
            if edge > y0 && edge < y1 {
                ys.push(edge);
            }
        }
        ys.push(y1);
        ys.sort_by(f32::total_cmp);
        ys.dedup();
        for pair in ys.windows(2) {
            let (ya, yb) = (pair[0], pair[1]);
            let ca = lerp(c0, c1, (ya - y0) / (y1 - y0));
            let cb = lerp(c0, c1, (yb - y0) / (y1 - y0));
            let (ia, ib) = (inset(ya), inset(yb));
            let i = mesh.vertices.len() as u32;
            mesh.colored_vertex(pos2(rc.left() + ia, ya), ca);
            mesh.colored_vertex(pos2(rc.right() - ia, ya), ca);
            mesh.colored_vertex(pos2(rc.left() + ib, yb), cb);
            mesh.colored_vertex(pos2(rc.right() - ib, yb), cb);
            mesh.add_triangle(i, i + 1, i + 2);
            mesh.add_triangle(i + 1, i + 3, i + 2);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

/// `rc` on whole screen pixels at `ppp` pixels a point (125 %, 150 % scaling put a 1 pt line
/// across two pixels, blurred or uneven), never thinner than one pixel. At 100 % it is `rc`.
/// Its edges are rounded each on its own, so what is drawn side by side meets; a line is as
/// thick everywhere only where its place is (a whole pixel, then the same fraction past it):
/// [`on_pixel`] the rows that repeat.
pub(super) fn snap(ppp: f32, rc: Rect) -> Rect {
    let round = |v: f32| (v * ppp).round() / ppp;
    let (x0, y0) = (round(rc.min.x), round(rc.min.y));
    let one = 1.0 / ppp;
    let x1 = round(rc.max.x).max(x0 + if rc.width() > 0.0 { one } else { 0.0 });
    let y1 = round(rc.max.y).max(y0 + if rc.height() > 0.0 { one } else { 0.0 });
    Rect::from_min_max(pos2(x0, y0), pos2(x1, y1))
}

/// `v` (points) on the nearest whole screen pixel at `ppp` pixels a point: where something
/// that repeats starts (a list's rows), so what is drawn in each lands on the pixels alike. Rows
/// 17 pt apart are 21.25 px at 125 %: their grid lines, snapped from fractional places, came
/// out two pixels thick on every fourth row.
pub fn on_pixel(ppp: f32, v: f32) -> f32 {
    (v * ppp).round() / ppp
}

/// Fill inclusive pixel coordinates `x0..=x1, y0..=y1`.
pub fn fill(p: &Painter, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
    solid(
        p,
        Rect::from_min_max(pos2(x0, y0), pos2(x1 + 1.0, y1 + 1.0)),
        c,
    );
}

pub fn px(p: &Painter, x: f32, y: f32, c: Color32) {
    fill(p, x, y, x, y, c);
}

pub(super) fn hline(p: &Painter, x0: f32, x1: f32, y: f32, c: Color32) {
    fill(p, x0, y, x1, y, c);
}

pub(super) fn vline(p: &Painter, x: f32, y0: f32, y1: f32, c: Color32) {
    fill(p, x, y0, x, y1, c);
}

/// The rounding of a control's corners (buttons, text boxes, drop-downs), in both modes.
pub fn control_radius() -> f32 {
    crate::theme::metrics().control_radius
}

/// The fill of a part with no face of its own (a combo's arrow, a glyph button, a column
/// header) in `state`: none at rest, a tint under the pointer, a deeper one pressed or open.
/// Colours from `c`, a theme's variant (the active one: `crate::theme::colors()`).
pub fn subtle_fill(state: State, c: &crate::palette::Palette) -> Option<Color32> {
    match state {
        State::Hot => Some(c.subtle_hot),
        State::Pressed | State::Open => Some(c.subtle_pressed),
        _ => None,
    }
}

/// The blue highlight of a calendar's days and arrows (Win32's month calendar) in `state`:
/// a light accent tint under the pointer, a deeper one pressed (from `c`).
pub fn highlight_fill(state: State, c: &crate::palette::Palette) -> Option<Color32> {
    match state {
        State::Hot => Some(c.highlight_hot),
        State::Pressed | State::Open => Some(c.highlight_pressed),
        _ => None,
    }
}

/// Tints `zone` (a part already drawn at rest) with `tint` (none at rest: [`subtle_fill`]) and
/// draws its glyph again on top.
pub fn tint_part(p: &Painter, zone: Rect, tint: Option<Color32>, glyph: impl FnOnce(&Painter)) {
    if let Some(c) = tint {
        p.rect_filled(zone, crate::theme::metrics().part_radius, c);
        glyph(p);
    }
}

/// Group box frame (`rc` on screen) with a gap in its top line for a checkbox caption (`gap` =
/// English x range, inclusive pixels).
pub fn group_box(p: &Painter, rc: Rect, gap: Option<(f32, f32)>) {
    let (l, t, rr, b) = (rc.left(), rc.top(), rc.right() - 1.0, rc.bottom() - 1.0);
    let gap = gap.map(|(g0, g1)| {
        let (a, b) = (mcol(g0), mcol(g1));
        (a.min(b), a.max(b))
    });
    match gap {
        Some((g0, g1)) => {
            hline(p, l, g0 - 1.0, t, GROUP_LINE());
            hline(p, g1 + 1.0, rr, t, GROUP_LINE());
        }
        None => hline(p, l, rr, t, GROUP_LINE()),
    }
    vline(p, l, t, b, GROUP_LINE());
    vline(p, rr, t, b, GROUP_LINE());
    hline(p, l, rr, b, GROUP_LINE());
}

/// Etched horizontal separator in English rect `x, y, w × 2`: `#a0a0a0` over `#ffffff`, the
/// white line 1 px longer so it covers the far end.
pub fn etched_h(p: &Painter, x: f32, y: f32, w: f32) {
    let x = rect(Rect::from_min_size(pos2(x, y), vec2(w, 2.0))).left();
    hline(p, x, x + w - 2.0, y, ETCH_SHADOW());
    px(p, x + w - 1.0, y, ETCH_LIGHT());
    hline(p, x, x + w - 1.0, y + 1.0, ETCH_LIGHT());
}

/// Etched vertical separator in English rect `x, y, 2 × h`.
pub fn etched_v(p: &Painter, x: f32, y: f32, h: f32) {
    let x = rect(Rect::from_min_size(pos2(x, y), vec2(2.0, h))).left();
    vline(p, x, y, y + h - 2.0, ETCH_SHADOW());
    px(p, x, y + h - 1.0, ETCH_LIGHT());
    vline(p, x + 1.0, y, y + h - 1.0, ETCH_LIGHT());
}

/// [`fill`] in English pixels (inclusive): on screen in a mirrored surface.
pub fn fill_l(p: &Painter, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
    let (a, b) = (mcol(x0), mcol(x1));
    fill(p, a.min(b), y0, a.max(b), y1, c);
}

/// [`px`] at an English pixel.
pub fn px_l(p: &Painter, x: f32, y: f32, c: Color32) {
    px(p, mcol(x), y, c);
}

/// The accent (Windows' blue): borders of open and focused controls.
pub fn accent() -> Color32 {
    crate::theme::colors().accent
}
