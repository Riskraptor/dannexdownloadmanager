//! Right to left and placement (D-019): each window's client area is a surface, mirrored in a
//! right-to-left interface, and every rect, pen x and edge is placed through it.

use super::*;

// ---- right to left (D-019) -----------------------------------------------------------------
//
// Every window is laid out to IDM's English pixels. In a right-to-left interface (Arabic,
// Persian, Hebrew, Urdu, Uyghur) each window's client area is a mirrored *surface*: what is made
// for it is made at its mirrored place, so it is drawn and hit-tested there.
//
// - `r` and the rect helpers (`rect`) give a rect where it is on screen: `x → L + R − x − w`.
//   Everything that takes a `Rect` takes such a rect, and puts its insides by their leading
//   (start) and trailing (end) edges (`lead`, `trail`, `fwd`).
// - Text helpers take an English pen x: the *start* of the text. In RTL the text box is placed
//   with its start at the mirrored edge — right-aligned there; a right-aligned text
//   (`text_right`) ends left-aligned; a centred one stays centred. Text itself is never
//   mirrored, and is laid out in the interface's direction (`direction`), so a mixed line reads in
//   order; data goes in as isolates (`i18n::ltr`, `i18n::isolate`).
// - Points given to widgets (a check box's glyph) are English points.
// - The pixel painters (`fill`, `px`, `solid`) paint where they are told; `fill_l` takes English
//   pixels.

thread_local! {
    /// The surfaces being laid out (left and right edge of each), innermost last; only in a
    /// right-to-left interface.
    pub(super) static SURFACES: std::cell::RefCell<Vec<(f32, f32)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A surface laid out while this lives: a window's client area from `left` to `right`, mirrored
/// in a right-to-left interface (see above). A left-to-right one needs nothing.
#[must_use]
pub struct Surface(pub(super) ());

impl Surface {
    pub fn enter(left: f32, right: f32) -> Self {
        Self::enter_if(crate::i18n::rtl(), left, right)
    }

    /// [`Surface::enter`] with the direction given (tests; a surface that is never mirrored).
    pub fn enter_if(rtl: bool, left: f32, right: f32) -> Self {
        SURFACES.with(|s| {
            s.borrow_mut().push(if rtl {
                (left, right)
            } else {
                (f32::NAN, f32::NAN)
            })
        });
        Self(())
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        SURFACES.with(|s| s.borrow_mut().pop());
    }
}

/// The mirrored surface being laid out: its left + right edge.
pub(super) fn axis() -> Option<f32> {
    SURFACES.with(|s| {
        s.borrow()
            .last()
            .filter(|(l, _)| !l.is_nan())
            .map(|(l, r)| l + r)
    })
}

/// Is the surface being laid out mirrored (a right-to-left interface)?
pub fn rtl() -> bool {
    axis().is_some()
}

/// The screen x of an English edge x (a rect's side, a text's pen).
pub fn mx(x: f32) -> f32 {
    axis().map_or(x, |a| a - x)
}

/// The screen column of an English pixel column (inclusive pixel coordinates).
pub fn mcol(x: f32) -> f32 {
    axis().map_or(x, |a| a - 1.0 - x)
}

/// The screen rect of an English rect.
pub fn rect(rc: Rect) -> Rect {
    match axis() {
        Some(a) => Rect::from_min_max(pos2(a - rc.max.x, rc.min.y), pos2(a - rc.min.x, rc.max.y)),
        None => rc,
    }
}

/// The screen point of an English point that starts something `w` wide (its top-left in English
/// is its top-right in RTL): the top-left of the screen rect.
pub fn at(p: Pos2, w: f32) -> Pos2 {
    rect(Rect::from_min_size(p, vec2(w, 0.0))).min
}

/// A screen rect's leading (start) edge: left, or right in RTL.
pub fn lead(rc: Rect) -> f32 {
    if rtl() { rc.right() } else { rc.left() }
}

/// The reading direction along x: +1, or −1 in RTL.
pub fn fwd() -> f32 {
    if rtl() { -1.0 } else { 1.0 }
}

/// A widget's id: `key` within its window. Ids are made from a widget's label and place, so the
/// viewport is part of them: two windows with the same widget in the same place (two progress
/// windows' Pause buttons) must not share one.
pub(super) fn widget_id(ui: &Ui, key: impl std::hash::Hash + std::fmt::Debug) -> egui::Id {
    egui::Id::unique((ui.ctx().viewport_id(), key))
}

/// Pixel rect from spec numbers (x, y, w, h) relative to `origin`, where it is on screen.
pub fn r(origin: Pos2, x: f32, y: f32, w: f32, h: f32) -> Rect {
    rect(Rect::from_min_size(origin + vec2(x, y), vec2(w, h)))
}

/// The paragraph direction of interface text: the interface's.
pub fn direction() -> egui::epaint::text::TextDirection {
    use egui::epaint::text::TextDirection;
    if crate::i18n::rtl() {
        TextDirection::RightToLeft
    } else {
        TextDirection::LeftToRight
    }
}

/// The English x of the window's near and far edges, 6 px in: where a text may start and end.
pub(super) fn window_edges(p: &Painter) -> (f32, f32) {
    let clip = p.clip_rect();
    if rtl() {
        (mx(clip.right()) + 6.0, mx(clip.left()) - 6.0)
    } else {
        (clip.left() + 6.0, clip.right() - 6.0)
    }
}
