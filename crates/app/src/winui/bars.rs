//! Bars: the progress bar, meters and the era look's bar.

use super::*;

/// A Windows 11 progress bar in `rc` (a few pixels tall): a thin track and the accent filled to
/// `fraction`; while the total is unknown (`None`) a segment runs along it (`time`: seconds,
/// for its place).
pub fn progress_bar(p: &Painter, rc: Rect, fraction: Option<f32>, time: f64) {
    let c = crate::theme::colors();
    // The era themes: XP's blocks, the glossy bars of Windows 7 and Mac OS X.
    let (from, to) = match fraction {
        Some(f) => (0.0, f.clamp(0.0, 1.0)),
        None => {
            let t = (time * 0.6).fract() as f32;
            ((t * 1.4 - 0.4).max(0.0), (t * 1.4).min(1.0))
        }
    };
    if era_bar(p, rect(rc), from, to, c.progress_bar, rtl()) {
        return;
    }
    // Windows 8: a bordered bar, filled.
    if crate::theme::current().controls.progress == crate::palette::ProgressLook::Bar {
        let border = crate::theme::metrics().border;
        p.rect_filled(rect(rc), 0.0, c.meter_track);
        let inner = rc.shrink(border);
        if to > from {
            let w = inner.width();
            p.rect_filled(
                rect(Rect::from_min_max(
                    pos2(inner.left() + from * w, inner.top()),
                    pos2(inner.left() + to * w, inner.bottom()),
                )),
                0.0,
                c.progress_bar,
            );
        }
        p.rect_stroke(
            rect(rc),
            0.0,
            Stroke::new(border, c.meter_edge),
            StrokeKind::Inside,
        );
        return;
    }
    let (track, accent) = (c.progress_track, c.progress_bar);
    let r = rc.height() / 2.0;
    // The track is a 1 px line through the middle, as Windows 11 draws it.
    let mid = rc.center().y;
    p.rect_filled(
        rect(Rect::from_min_max(
            pos2(rc.left(), mid - 0.5),
            pos2(rc.right(), mid + 0.5),
        )),
        0.5,
        track,
    );
    if to > from {
        let w = rc.width();
        p.rect_filled(
            rect(Rect::from_min_max(
                pos2(rc.left() + from * w, rc.top()),
                pos2(rc.left() + to * w, rc.bottom()),
            )),
            r,
            accent,
        );
    }
}

/// How far along something is, as IDM shows a download: its green fill on a track (themed: the
/// track was light in dark mode too), the controls' rounded corners. `fraction` in 0..=1, filled
/// from the leading edge (`rc` on screen): from the right in a right-to-left interface, as the
/// speed graph, the connections' bar and Statistics' bars run.
pub fn meter(p: &Painter, rc: Rect, fraction: f32) {
    let radius = control_radius().min(rc.height() / 4.0).floor();
    let c = crate::theme::colors();
    if era_bar(p, rc, 0.0, fraction.clamp(0.0, 1.0), c.meter_fill, rtl()) {
        return;
    }
    let border = crate::theme::metrics().border;
    p.rect_filled(rc, radius, c.meter_track);
    let inner = rc.shrink(border);
    let w = (fraction.clamp(0.0, 1.0) * inner.width()).floor();
    if w > 0.0 {
        let done = if rtl() {
            Rect::from_min_max(pos2(inner.right() - w, inner.top()), inner.max)
        } else {
            Rect::from_min_size(inner.min, vec2(w, inner.height()))
        };
        p.rect_filled(done, (radius - border).max(0.0), c.meter_fill);
    }
    p.rect_stroke(
        rc,
        radius,
        Stroke::new(border, c.meter_edge),
        StrokeKind::Inside,
    );
}

/// An era theme's bar (`palette::ProgressLook::Blocks` or `Gloss`) in `rc` (on screen): a
/// shaded track inside its edge, filled from `from` to `to` (fractions of its inside, from its
/// left, or from its right when `mirrored`) in `fill`: in blocks (Windows XP), or glossy as
/// the theme's buttons (Windows 7, Mac OS X). False, drawing nothing, for the other looks.
pub(super) fn era_bar(
    p: &Painter,
    rc: Rect,
    from: f32,
    to: f32,
    fill: Color32,
    mirrored: bool,
) -> bool {
    use crate::derive::mix;
    use crate::palette::ProgressLook;
    let look = crate::theme::current().controls.progress;
    if !matches!(look, ProgressLook::Blocks | ProgressLook::Gloss) {
        return false;
    }
    let c = crate::theme::colors();
    let m = crate::theme::metrics();
    let blocks = look == ProgressLook::Blocks;
    let radius = if blocks {
        m.button_radius
    } else {
        control_radius()
    }
    .min((rc.height() / 2.0).floor());
    fill_shaded(
        p,
        rc,
        radius,
        &[
            (0.0, mix(c.meter_track, c.meter_edge, 0.3)),
            (0.35, c.meter_track),
            (1.0, c.meter_track),
        ],
    );
    let inner = rc.shrink(if blocks { 2.0 + m.border } else { m.border });
    let w = inner.width();
    // A part of the inside, `a` to `b` of it from the bar's start.
    let span = |a: f32, b: f32| {
        if mirrored {
            Rect::from_min_max(
                pos2(inner.right() - b, inner.top()),
                pos2(inner.right() - a, inner.bottom()),
            )
        } else {
            Rect::from_min_max(
                pos2(inner.left() + a, inner.top()),
                pos2(inner.left() + b, inner.bottom()),
            )
        }
    };
    let (a, b) = ((from * w).round(), (to * w).round());
    if b > a && w > 0.0 {
        if blocks {
            // Windows XP: blocks 2 points apart, lighter across their middle; whole ones, the
            // last cut at the bar's end.
            let stops = [(0.0, fill), (0.45, mix(fill, c.gloss, 0.55)), (1.0, fill)];
            let bw = (inner.height() * 0.6).round().max(4.0);
            let mut x = (a / (bw + 2.0)).floor() * (bw + 2.0);
            while x < b {
                let block = span(x.max(a), (x + bw).min(w));
                if block.width() > 0.0 {
                    fill_shaded(p, block, 0.0, &stops);
                }
                x += bw + 2.0;
            }
        } else {
            let shine = Shine::of_theme().unwrap_or(Shine::Aero);
            fill_shaded(
                p,
                span(a, b),
                (radius - m.border).max(0.0),
                &shine.stops(fill, c.gloss),
            );
        }
    }
    p.rect_stroke(
        rc,
        radius,
        Stroke::new(m.border, c.meter_edge),
        StrokeKind::Inside,
    );
    true
}
