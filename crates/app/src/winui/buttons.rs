//! Buttons: push buttons of every theme, rows of them fitted to their labels, small and icon
//! buttons, and the progress window's to-tray button.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Normal,
    Default,
    Disabled,
    /// The one answer a window recommends (the default too), filled with the accent as Windows
    /// 11's accent buttons: the first start's "Yes, download them".
    Accent,
}

/// What a face drawn by [`paint_button`] is: a push button, a small button-like part (a spin
/// box's halves, an icon button: its corners no rounder than a third of its height) or a
/// drop-down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    Push,
    Small,
    DropDown,
}

/// A button's colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Face {
    pub(super) fill: Color32,
    pub(super) border: Color32,
    /// The bottom edge (a darker line at rest; flat, the border's own, while pressed).
    pub(super) bottom: Color32,
    pub(super) text: Color32,
}

/// Push button: #333333 fill, 1 px rounded (r 4) border by state, text centred with its
/// baseline at top+14 (h 21) or top+13 (h 19). `mnemonic` underlines that char index.
pub fn button(
    ui: &mut Ui,
    rc: Rect,
    label: &str,
    kind: ButtonKind,
    focus_rect: bool,
    mnemonic: Option<usize>,
) -> Response {
    // The label is translated here (M16); a mnemonic index points into the English text only,
    // and becomes the "&" form every button uses.
    let translated = crate::i18n::t(label);
    let label = match mnemonic.filter(|_| translated == label) {
        Some(i) => {
            let at = label.char_indices().nth(i).map_or(label.len(), |(b, _)| b);
            format!("{}&{}", &label[..at], &label[at..])
        }
        None => translated.to_owned(),
    };
    button_face(ui, rc, &label, kind, focus_rect)
}

/// A button's rect `rc` widened to the left (its right edge stays: a button against the right
/// side, with room before it) to fit its translated `label` whole, with the room [`button`]
/// keeps inside the border, at most to `max_width`. English labels fit IDM's buttons, so
/// English never changes; a label that would still not fit ends in "..." and shows whole on
/// hover, as on every button. (A row of buttons: [`buttons_from_left`], [`buttons_to_right`].)
pub fn button_rect_for(p: &Painter, rc: Rect, label: &str, max_width: f32) -> Rect {
    let w = button_fit_width(p, label, rc.width(), max_width);
    // Widened in English, where its right edge stays; on screen in RTL its left edge stays and
    // it widens to the right, into the room before it (`rect` is its own inverse).
    let english = rect(rc);
    rect(Rect::from_min_max(
        pos2(english.right() - w, english.top()),
        english.max,
    ))
}

/// How wide a [`button`] must be for its translated `label` to show whole, with the room it
/// keeps inside its border: at least `min` (its English width: English labels fit it, so English
/// never changes), at most `max` (past it the label ends in "..." and shows whole on hover, as on
/// every button).
pub fn button_fit_width(p: &Painter, label: &str, min: f32, max: f32) -> f32 {
    label_width(p, label, min).clamp(min, max.max(min))
}

/// `s` in one line where it is no wider than `room` px in `font`, else in two, broken where the
/// wider of the two is narrowest: at a space, after a slash or a hyphen, or between two
/// characters of Chinese or Japanese (a title given a second line where a control beside it
/// needs the room). Still one line where it has no such place.
pub fn fit_lines(p: &Painter, font: FontId, s: &str, room: f32) -> Vec<String> {
    let width = |t: &str| text_width_font(p, font.clone(), t);
    if width(s) <= room {
        return vec![s.to_owned()];
    }
    let g: Vec<(usize, &str)> = s.grapheme_indices(true).collect();
    let mut best: Option<(f32, String, String)> = None;
    for k in 1..g.len() {
        let (at, here) = g[k];
        let before = g[k - 1].1;
        let first = |gr: &str| gr.chars().next().unwrap_or(' ');
        let (a, b) = if before == " " {
            (&s[..g[k - 1].0], &s[at..])
        } else if here != " "
            && (before == "/"
                || before == "-"
                || ((cjk(first(before)) || cjk(first(here))) && !here.chars().all(no_line_start)))
        {
            (&s[..at], &s[at..])
        } else {
            continue;
        };
        let (a, b) = (a.trim_end(), b.trim_start());
        if a.is_empty() || b.is_empty() {
            continue;
        }
        let w = width(a).max(width(b));
        if best.as_ref().is_none_or(|(bw, ..)| w < *bw) {
            best = Some((w, a.to_owned(), b.to_owned()));
        }
    }
    match best {
        Some((_, a, b)) => vec![a, b],
        None => vec![s.to_owned()],
    }
}

/// The width a [`button`] `english` px wide wants for its translated `label`: the label and the
/// room inside the border; no wider than `english` where the label shows whole in it (a label
/// that fits as tightly as some English ones do keeps the button as it is), and narrower where the
/// label needs less.
pub(super) fn label_width(p: &Painter, label: &str, english: f32) -> f32 {
    let shown = crate::i18n::t(label).replace('&', "");
    let text = text_width(p, &shown);
    let want = (text + 4.0 + 12.0).ceil();
    // Whole on the button as it is ([`fit_label`]): it stays.
    if text <= english - 4.0 {
        want.min(english)
    } else {
        want
    }
}

/// A row of [`button`]s laid out for their translated labels, `gap` apart, the first starting
/// at English x `left`: each `(label, English width)` as wide as [`button_fit_width`] says (at
/// most `max`). Their English (x, width) in the order given, for [`r`]; in English every button
/// keeps its place.
pub fn buttons_from_left(
    p: &Painter,
    left: f32,
    gap: f32,
    buttons: &[(&str, f32)],
    max: f32,
) -> Vec<(f32, f32)> {
    let mut x = left;
    buttons
        .iter()
        .map(|(label, min)| {
            let w = button_fit_width(p, label, *min, max);
            let at = (x, w);
            x += w + gap;
            at
        })
        .collect()
}

/// [`buttons_from_left`] for a row that ends at English x `right` (the last button's right
/// edge): it grows to the left.
pub fn buttons_to_right(
    p: &Painter,
    right: f32,
    gap: f32,
    buttons: &[(&str, f32)],
    max: f32,
) -> Vec<(f32, f32)> {
    let mut x = right + gap;
    let mut out: Vec<(f32, f32)> = buttons
        .iter()
        .rev()
        .map(|(label, min)| {
            let w = button_fit_width(p, label, *min, max);
            x -= gap + w;
            (x, w)
        })
        .collect();
    out.reverse();
    out
}

/// A row of [`button`]s at their English places, `(label, English x, English width)` from left
/// to right (IDM's rows, their gaps not all alike), laid out for their translated labels: the row
/// keeps its right end, and starts no further left than English x `from` (what is beside it).
/// Each button is as wide as [`button_fit_width`] says (at most `max`), widening to the left; one
/// that comes closer than `gap` to the one before it (its English gap, if smaller) pushes it left,
/// and one pushed past `from` pushes those after it back right, each moved no further than it
/// must. When the row would not fit, the buttons wider than their label needs give back that room
/// first (to their label, at least 73 px, Windows' button), then the widest are narrowed, their
/// labels cut with "..." (whole on hover). Their English (x, width) in the order given, for
/// [`r`]; in English every button keeps its place. ([`button_row_from_left`]: a row that keeps
/// its left end.)
pub fn button_row(
    p: &Painter,
    buttons: &[(&str, f32, f32)],
    gap: f32,
    from: f32,
    max: f32,
) -> Vec<(f32, f32)> {
    let row: Vec<(f32, f32, f32)> = buttons
        .iter()
        .map(|(label, x, w)| (*x, *w, label_width(p, label, *w)))
        .collect();
    place_row(&row, gap, from, max)
}

/// [`button_row`] for a row that keeps its left end and ends no further right than English x
/// `to`: its buttons widen to the right.
pub fn button_row_from_left(
    p: &Painter,
    buttons: &[(&str, f32, f32)],
    gap: f32,
    to: f32,
    max: f32,
) -> Vec<(f32, f32)> {
    // The same row seen in a mirror (x → −x): its left end becomes the right one.
    let mirrored: Vec<(f32, f32, f32)> = buttons
        .iter()
        .rev()
        .map(|(label, x, w)| (-(x + w), *w, label_width(p, label, *w)))
        .collect();
    place_row(&mirrored, gap, -to, max)
        .into_iter()
        .rev()
        .map(|(x, w)| (-(x + w), w))
        .collect()
}

/// [`button_row`]'s arithmetic: each button's English (x, width) and the width its label needs.
pub(super) fn place_row(row: &[(f32, f32, f32)], gap: f32, from: f32, max: f32) -> Vec<(f32, f32)> {
    let n = row.len();
    if n == 0 {
        return Vec::new();
    }
    // Each at least `gap` (or its English gap) from the next.
    let gaps: Vec<f32> = (1..n)
        .map(|i| (row[i].0 - (row[i - 1].0 + row[i - 1].1)).min(gap))
        .collect();
    // As wide as the label needs, never narrower than in English.
    let mut widths: Vec<f32> = row
        .iter()
        .map(|&(_, w, fit)| fit.clamp(w, max.max(w)))
        .collect();
    // Too long for its room even side by side: the overflow taken back one pixel at a time,
    // first from the room labels don't need (down to their label, or 73 px), then from the widest
    // of the widened buttons (down to their English width).
    let right = row[n - 1].0 + row[n - 1].1;
    let room = right - from - gaps.iter().sum::<f32>();
    // Never narrower than this: its English width, or less where its label needs less (73 px
    // at least, Windows' button).
    let least: Vec<f32> = row
        .iter()
        .map(|&(_, w, fit)| w.min(fit.max(73.0)))
        .collect();
    // What its label needs (at least `least`).
    let needs: Vec<f32> = row
        .iter()
        .zip(&least)
        .map(|(&(_, _, fit), &l)| fit.max(l))
        .collect();
    let mut over = (widths.iter().sum::<f32>() - room).ceil();
    // The button with the most room above `low` (or the widest with some), if any; the last
    // first when alike.
    let most = |widths: &[f32], low: &[f32], by_width: bool| -> Option<usize> {
        (0..n)
            .filter(|&i| widths[i] - low[i] >= 1.0)
            .max_by(|&a, &b| {
                let key = |i: usize| {
                    if by_width {
                        widths[i]
                    } else {
                        widths[i] - low[i]
                    }
                };
                key(a).total_cmp(&key(b)).then(a.cmp(&b))
            })
    };
    while over > 0.0 {
        let Some(i) = most(&widths, &needs, false).or_else(|| most(&widths, &least, true)) else {
            break;
        };
        widths[i] -= 1.0;
        over -= 1.0;
    }
    // From the right end: each keeps its English right edge while it can, else stands its gap
    // before the next.
    let mut xs = vec![0.0; n];
    let mut limit = f32::INFINITY;
    for i in (0..n).rev() {
        xs[i] = (row[i].0 + row[i].1).min(limit) - widths[i];
        if i > 0 {
            limit = xs[i] - gaps[i - 1];
        }
    }
    // Then from `from`: those pushed past it come back right, as little as they must.
    let mut least_x = from;
    for i in 0..n {
        xs[i] = xs[i].max(least_x);
        least_x = xs[i] + widths[i] + gaps.get(i).copied().unwrap_or(0.0);
    }
    xs.into_iter().zip(widths).collect()
}

/// A lone [`button`] at English `x`, `w` wide, widened for its translated `label` around its
/// centre, between English x `from` and `to` (what is beside it), moved inwards where it would
/// reach either. Its English (x, width); in English, its place.
pub fn button_around(p: &Painter, label: &str, x: f32, w: f32, from: f32, to: f32) -> (f32, f32) {
    let width = button_fit_width(p, label, w, (to - from).max(w));
    let left = (x + (w - width) / 2.0).floor();
    (left.min(to - width).max(from), width)
}

/// Of a button's labels in turn (Pause and Resume), the one whose translation is the widest: the
/// one to lay the button out for, so it keeps its size as its label changes.
pub fn widest_label<'a>(p: &Painter, labels: &[&'a str]) -> &'a str {
    labels
        .iter()
        .copied()
        .max_by(|a, b| {
            let shown_width = |l: &str| text_width(p, &crate::i18n::t(l).replace('&', ""));
            shown_width(a).total_cmp(&shown_width(b))
        })
        .unwrap_or("")
}

/// `label` as it fits on a button `rc`: whole, or cut with "..." (true) when it would reach the
/// border (English labels fit IDM's buttons, so English never changes).
pub(super) fn fit_label(p: &Painter, label: &str, rc: Rect) -> (String, bool) {
    let room = rc.width() - 4.0;
    if text_width(p, label) <= room {
        (label.to_owned(), false)
    } else {
        (ellipsize(p, &tahoma(), label, room), true)
    }
}

/// The 22 × 21 "+" / "..." buttons.
pub fn small_button(ui: &mut Ui, rc: Rect, glyph: &str) -> Response {
    button(ui, rc, glyph, ButtonKind::Normal, false, None)
}

/// A button label's baseline below the button's top: as measured on Windows' buttons at their
/// usual heights (13), centred ([`baseline_in`]) in taller ones; both modes alike.
pub(super) fn button_baseline(height: f32) -> f32 {
    match height {
        h if h <= 21.0 => 13.0,
        h => baseline_in(0.0, h),
    }
}

/// Light push button: #fdfdfd fill, radius 3; border #d0d0d0 (bottom #bababa), default
/// #0078d4 (bottom #006bbe), disabled #e9e9e9 with #838383 text; dotted focus rect.
pub fn button_light(
    ui: &mut Ui,
    rc: Rect,
    label: &str,
    kind: ButtonKind,
    focus_rect: bool,
) -> Response {
    button_face(ui, rc, crate::i18n::t(label), kind, focus_rect)
}

/// How wide a light button must be for its (translated) `label`: at least `min` (Windows'
/// 73 px), else the label and its margins, so a longer translation is never cut.
pub fn button_width(p: &Painter, label: &str, min: f32) -> f32 {
    (text_width(p, crate::i18n::t(label)) + 24.0)
        .ceil()
        .max(min)
}

/// Light buttons side by side, the row ending at English x `right` (window-relative to `o`),
/// each as wide as [`button_width`] says, 8 px apart, in the order given. Their responses, in
/// that order.
pub fn buttons_light_right(
    ui: &mut Ui,
    o: Pos2,
    right: f32,
    top: f32,
    buttons: &[(&str, ButtonKind)],
) -> Vec<Response> {
    let p = ui.painter().clone();
    let widths: Vec<f32> = buttons
        .iter()
        .map(|(l, _)| button_width(&p, l, 73.0))
        .collect();
    let mut x = right - widths.iter().sum::<f32>() - 8.0 * (buttons.len().saturating_sub(1)) as f32;
    let mut out = Vec::new();
    for ((label, kind), w) in buttons.iter().zip(widths) {
        out.push(button_light(ui, r(o, x, top, w, 21.0), label, *kind, false));
        x += w + 8.0;
    }
    out
}

/// A push button in the current theme (dark: IDM's dark buttons; light: Windows 11's), its
/// (translated) `label` with "&" before the access letter: the letter is underlined and
/// Alt+letter presses it. Hot and pressed as Windows 11 draws them: a pressed button's label
/// dims and its darker bottom edge flattens. `default_focus`: the dialog's default button, whose
/// focus rectangle shows while no other control has the keyboard focus; any control with the
/// focus shows it.
pub(super) fn button_face(
    ui: &mut Ui,
    rc: Rect,
    label: &str,
    kind: ButtonKind,
    default_focus: bool,
) -> Response {
    let enabled = kind != ButtonKind::Disabled;
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    // Keyed by its place, not its label: Pause becoming Resume keeps the focus.
    let resp = ui.interact(
        rc,
        widget_id(ui, ("button", rc.min.x as i32, rc.min.y as i32)),
        sense,
    );
    pass_focus_on(ui, &resp, enabled);
    expose(
        &resp,
        egui::Role::Button,
        crate::i18n::t(label),
        enabled,
        None,
    );
    focus_on_press(&resp);
    // A focused button answers Enter itself (egui clicks it): the dialog's default button,
    // read after it, doesn't run as well.
    if resp.has_focus() {
        take_key(ui, egui::Key::Enter);
    }
    let state = State::of(&resp, enabled);
    let p = ui.painter();
    let face = button_colors(kind, state, crate::theme::colors());
    let radius = paint_button(p, rc, kind, state, &face, Shape::Push);
    // "&Start now": the letter after '&' is underlined and Alt+letter presses it.
    let (shown, mnemonic) = match label.split_once('&') {
        Some((before, after)) if !after.is_empty() => (format!("{before}{after}"), Some(before)),
        _ => (label.to_owned(), None),
    };
    // A translation wider than the button ends in "..." and shows whole on hover (UI rule).
    let (shown, cut) = fit_label(p, &shown, rc);
    let mnemonic = mnemonic.filter(|_| !cut);
    let w = text_width(p, &shown);
    let x = (rc.center().x - w / 2.0).round();
    // "..." sits on the baseline (no capitals above it): the dots themselves centred.
    let baseline = if shown.trim() == "..." {
        rc.top() + (rc.height() / 2.0).round() + 1.0
    } else {
        rc.top() + button_baseline(rc.height())
    };
    // Centred in a rect on screen: placed by its left, not by an English pen.
    let drawn = text_at(p, tahoma(), x, baseline, &shown, face.text);
    let mut pressed_by_key = false;
    if let Some(before) = mnemonic
        && let Some(letter) = shown[before.len()..].chars().next()
    {
        // Underlined as the menus are (always dark; light once the keyboard is in use), while
        // Alt+letter always works.
        if crate::chrome::cues(ui.ctx()) {
            let (x0, x1) = char_span(p, &tahoma(), &shown, before.chars().count(), drawn);
            hline(p, x0, x1, baseline + 3.0, face.text);
        }
        pressed_by_key = enabled && mnemonic_pressed(ui, letter);
    }
    let resp = if pressed_by_key {
        let mut r = resp;
        r.flags.insert(egui::response::Flags::CLICKED);
        r
    } else {
        resp
    };
    if resp.has_focus() || (default_focus && ui.ctx().memory(|m| m.focused().is_none())) {
        focus_ring_round(ui.painter(), rc.shrink(2.0), (radius - 2.0).max(0.0));
    }
    if cut {
        resp.on_hover_text(label.replace('&', ""))
    } else {
        resp
    }
}

/// A push button's face, edge and bottom line in the theme's button style (`palette::ButtonLook`):
/// flat (Windows 11, KDE), or shaded toward the theme's `gloss` (Luna, Aero, Aqua), as `shape`
/// says it is used. Every button and every part drawn as one comes here. Its corners' radius.
pub(super) fn paint_button(
    p: &Painter,
    rc: Rect,
    kind: ButtonKind,
    state: State,
    face: &Face,
    shape: Shape,
) -> f32 {
    use crate::palette::ButtonLook;
    let c = crate::theme::colors();
    let m = crate::theme::metrics();
    let look = crate::theme::current().controls.button;
    let part = shape != Shape::Push;
    let radius = match look {
        _ if shape == Shape::DropDown => control_radius(),
        _ if part => control_radius().min((rc.height() / 3.0).floor()),
        ButtonLook::Fluent | ButtonLook::Breeze => control_radius(),
        ButtonLook::Luna | ButtonLook::Aero => m.button_radius,
        // Mac OS X's push buttons are round at the ends.
        ButtonLook::Aqua => (rc.height() / 2.0).floor(),
    };
    let Some(shine) = Shine::of_button(look) else {
        p.rect_filled(rc, radius, face.fill);
        p.rect_stroke(
            rc,
            radius,
            Stroke::new(m.border, face.border),
            StrokeKind::Inside,
        );
        // The bottom edge: a push button's and a drop-down's 3 points in from its ends, a small
        // part's from its corners (none on one too low for it).
        if shape != Shape::Small || rc.height() >= 14.0 {
            let inset = if shape == Shape::Small { radius } else { 3.0 };
            hline(
                p,
                rc.left() + inset,
                rc.right() - inset - 1.0,
                rc.bottom() - 1.0,
                face.bottom,
            );
        }
        return radius;
    };
    fill_shaded(p, rc, radius, &shine.stops(face.fill, c.gloss));
    // Windows XP: a glow inside the edge, orange under the pointer, blue on the default button
    // (its own border's colour; the edge itself stays dark).
    let mut border = face.border;
    if look == ButtonLook::Luna && state != State::Disabled {
        let glow = match (state, kind) {
            (State::Hot, _) => Some(c.button_glow),
            (State::Pressed, _) => None,
            (_, ButtonKind::Default) => {
                border = c.button_border;
                Some(face.border)
            }
            _ => None,
        };
        if let Some(glow) = glow.filter(|g| g.a() > 0) {
            p.rect_stroke(
                rc.shrink(m.border),
                (radius - m.border).max(0.0),
                Stroke::new(2.0, glow),
                StrokeKind::Inside,
            );
        }
    }
    p.rect_stroke(
        rc,
        radius,
        Stroke::new(m.border, border),
        StrokeKind::Inside,
    );
    radius
}

/// A button's colours for `kind` in `state`, from `c` (a theme's variant). The states are
/// decided here, once for every button: the pressed one flattens its bottom edge.
pub(super) fn button_colors(kind: ButtonKind, state: State, c: &crate::palette::Palette) -> Face {
    button_colors_in(kind, state, c, crate::theme::current().controls.button)
}

/// [`button_colors`] in the button style `look`.
pub(super) fn button_colors_in(
    kind: ButtonKind,
    state: State,
    c: &crate::palette::Palette,
    look: crate::palette::ButtonLook,
) -> Face {
    let pressed = state == State::Pressed;
    // Mac OS X: the default button is the accent's gel, its edge a shade darker.
    if look == crate::palette::ButtonLook::Aqua && kind == ButtonKind::Default {
        let face = button_colors_in(ButtonKind::Accent, state, c, look);
        return Face {
            border: crate::derive::shade(face.fill, -0.12),
            ..face
        };
    }
    if kind == ButtonKind::Accent {
        // The accent filled, a shade lighter under the pointer and darker pressed.
        let (fill, text) = match state {
            State::Pressed => (c.button_accent_pressed, c.button_accent_text_pressed),
            State::Hot => (c.button_accent_hot, c.button_accent_text),
            _ => (c.button_accent, c.button_accent_text),
        };
        return Face {
            fill,
            border: fill,
            bottom: if pressed {
                fill
            } else {
                c.button_accent_bottom
            },
            text,
        };
    }
    // Windows 11's buttons: a face a little lighter than the window in dark, a quiet border,
    // the default button outlined in the accent.
    let (border, bottom) = match kind {
        ButtonKind::Normal => (c.button_border, c.button_bottom),
        ButtonKind::Default | ButtonKind::Accent => {
            (c.button_default_border, c.button_default_bottom)
        }
        ButtonKind::Disabled => (c.button_disabled_border, c.button_disabled_border),
    };
    // KDE and Windows 7: the edge in the accent under the pointer and pressed (its face tinted,
    // by the theme's colours).
    let breeze = matches!(
        look,
        crate::palette::ButtonLook::Breeze | crate::palette::ButtonLook::Aero
    );
    let (border, bottom) = if breeze && matches!(state, State::Hot | State::Pressed) {
        (c.accent, c.accent)
    } else {
        (border, bottom)
    };
    let face = Face {
        fill: match state {
            State::Disabled => c.button_disabled,
            State::Pressed => c.button_pressed,
            State::Hot => c.button_hot,
            _ => c.button,
        },
        border,
        bottom,
        text: match state {
            State::Disabled => c.text_disabled,
            State::Pressed => c.button_text_pressed,
            _ => c.button_text,
        },
    };
    if pressed {
        Face {
            bottom: face.border,
            ..face
        }
    } else {
        face
    }
}

/// The frame of a light drop-down (combo or date picker): rounded `#c8c8c8` border, `#fefefe`
/// fill; enabled ones darken on hover and take the accent border while open.
/// A small button's face (a spin box's halves, the icon buttons): a push button's colours in
/// every state ([`button_colors`]): they had their own, and a disabled one looked hovered.
pub fn button_like(p: &Painter, rc: Rect, state: State) {
    let kind = if state == State::Disabled {
        ButtonKind::Disabled
    } else {
        ButtonKind::Normal
    };
    let face = button_colors(kind, state, crate::theme::colors());
    paint_button(p, rc, kind, state, &face, Shape::Small);
}

/// A flat 19 × 21 button with a 16 × 16 icon (the Scheduler's move up / down / remove), its
/// `tip` (English, translated here) on hover.
pub fn icon_button_light(ui: &mut Ui, rc: Rect, id: &str, enabled: bool, tip: &str) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(rc, widget_id(ui, ("icon-button-light", id)), sense);
    pass_focus_on(ui, &resp, enabled);
    expose(
        &resp,
        egui::Role::Button,
        crate::i18n::t(tip),
        enabled,
        None,
    );
    let p = ui.painter();
    button_like(p, rc, State::of(&resp, enabled));
    show_focus(ui, &resp, rc.shrink(2.0));
    // No label of its own: its tooltip says what it does (translated).
    resp.on_hover_text(crate::i18n::t(tip))
}

/// The flat icon button with our "to the notification area" glyph, drawn in the text ink: hides
/// a window to the tray (#63) where the system draws the title bar (Linux, macOS). `tip` is
/// English, translated here.
#[cfg_attr(windows, allow(dead_code))]
pub fn to_tray_button(ui: &mut Ui, rc: Rect, id: &str, tip: &str) -> Response {
    let resp = icon_button_light(ui, rc, id, true, tip);
    to_tray_glyph(
        ui.painter(),
        rc.center().round(),
        Stroke::new(1.0, LIGHT_TEXT()),
        rtl(),
    );
    resp
}

/// Our "minimize it away to the notification area" glyph (#63; our own drawing): Minimize's dash
/// at its height, shortened to 6 of its 10 px on the start side, and from its end a 45° arrow
/// (shaft about 5.7 px, open head) down towards the screen's corner where the tray is. No
/// vertical arrow and no tray box, so it reads neither as "download" nor as "restore". Right to
/// left it is mirrored, pointing down to the left: Windows in a right-to-left language mirrors its
/// taskbar, the notification area at the bottom left, and our caption mirrors with it. Centred on
/// `c` in `ink`. The title bar's hide button, and the tab-row button where the system draws the
/// title bar.
pub(super) fn to_tray_glyph(p: &Painter, c: Pos2, ink: Stroke, rtl: bool) {
    to_tray_glyph_sized(p, c, ink, rtl, 1.0);
}

/// [`to_tray_glyph`] `size` times as large.
pub(super) fn to_tray_glyph_sized(p: &Painter, c: Pos2, ink: Stroke, rtl: bool, size: f32) {
    let s = if rtl { -size } else { size };
    let at = |x: f32, y: f32| c + vec2(s * x, size * y);
    // The dash, where Minimize's is (y + 0.5 keeps a 1 px line on the pixel grid).
    p.line_segment([at(-5.0, 0.5), at(1.0, 0.5)], ink);
    // The arrow from the dash's end, down to the end side at 45°, and its open head; the head's
    // strokes on half pixels, so they stay one pixel wide at 100 %.
    p.line_segment([at(1.0, 0.5), at(4.5, 4.0)], ink);
    p.add(egui::Shape::line(
        vec![at(4.5, 1.5), at(4.5, 4.5), at(1.5, 4.5)],
        ink,
    ));
}
