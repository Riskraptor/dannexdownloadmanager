//! Check boxes and radio buttons: their captions, the glyphs of each control style (Windows 11,
//! Fluent, Breeze, the era looks) and the mixed state.

use super::*;

/// The English x where the translated `caption` of a checkbox whose glyph is at English x `x`
/// ends ([`checkbox_light_until`]'s caption starts 16 px after its glyph): what a control after
/// it may come up to.
pub fn checkbox_caption_end(p: &Painter, x: f32, caption: &str) -> f32 {
    x + 16.0 + text_width(p, crate::i18n::t(caption))
}

/// Classic sunken 13 × 13 checkbox with its caption 5 px to the right (baseline box top + 10).
pub fn checkbox(
    ui: &mut Ui,
    box_min: Pos2,
    checked: &mut bool,
    caption: &str,
    enabled: bool,
) -> Response {
    checkbox_until(ui, box_min, checked, caption, enabled, None)
}

/// A checkbox's caption drawn apart from it (two lines, a paragraph) in English `caption`: a
/// click on it ticks the box too, as on a caption the box draws itself.
pub fn caption_click(
    ui: &mut Ui,
    caption: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    checked: &mut bool,
    enabled: bool,
) -> Response {
    let sense = if enabled {
        pointer_only()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(rect(caption), widget_id(ui, ("caption", id)), sense);
    pass_focus_on(ui, &resp, enabled);
    if resp.clicked() {
        *checked = !*checked;
    }
    resp
}

/// [`checkbox`] whose caption also stops before English x `until` (a control after it).
pub fn checkbox_until(
    ui: &mut Ui,
    box_min: Pos2,
    checked: &mut bool,
    caption: &str,
    enabled: bool,
    until: Option<f32>,
) -> Response {
    // Windows 11's check box in both themes (dark mode drew a classic white 3-D box beside the
    // Windows 11 ones of the same window).
    checkbox_light_until(
        ui,
        box_min,
        checked,
        crate::i18n::t(caption),
        enabled,
        until,
    )
}

/// The gap a check box at English `x` leaves in a group box's top line for its caption (English
/// pixels): IDM's measured end (`english_end`) in English, else 2 px past the translated caption
/// (`light`: a [`checkbox_light`], caption at +16; else a [`checkbox`], at +18).
pub fn caption_gap(
    p: &Painter,
    x: f32,
    english_end: f32,
    caption: &str,
    light: bool,
) -> (f32, f32) {
    let shown = crate::i18n::t(caption);
    // Every check box's caption starts 16 px after it (`light`: kept for its callers); the gap
    // is as wide as the caption in any language, English's too (its wording changes).
    let _ = (light, english_end);
    let lead = 16.0;
    (x, x + lead + text_width(p, shown) + 2.0)
}

/// Win11 light checkbox glyph (13 × 13) with its label at glyph.x + 16 (baseline glyph top + 10).
pub fn checkbox_light(
    ui: &mut Ui,
    glyph: Pos2,
    checked: &mut bool,
    caption: &str,
    enabled: bool,
) -> Response {
    checkbox_light_until(ui, glyph, checked, caption, enabled, None)
}

/// [`checkbox_light`] whose caption must end before English x `until` (a box after it on the
/// same row).
pub fn checkbox_light_until(
    ui: &mut Ui,
    glyph: Pos2,
    checked: &mut bool,
    caption: &str,
    enabled: bool,
    until: Option<f32>,
) -> Response {
    let caption = crate::i18n::t(caption);
    let p = ui.painter().clone();
    // A caption longer than its room ends in "..." at the edge, whole on hover (UI rule).
    let (shown, cut) = fit_caption_until(&p, glyph.x + 16.0, caption, until);
    let w = text_width(&p, &shown);
    // `glyph` is English; the glyph and its caption (after it) are placed on screen.
    let english = glyph;
    let hit = rect(Rect::from_min_size(english, vec2(16.0 + w + 2.0, 13.0)));
    let glyph = at(english, 13.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(
        hit,
        widget_id(ui, ("check-light", glyph.x as i32, glyph.y as i32)),
        sense,
    );
    pass_focus_on(ui, &resp, enabled);
    let resp = enter_is_not_a_click(ui, resp);
    focus_on_press(&resp);
    show_focus(ui, &resp, hit.expand(1.0));
    if resp.clicked() {
        *checked = !*checked;
    }
    expose(
        &resp,
        egui::Role::CheckBox,
        caption,
        enabled,
        Some(*checked),
    );
    let rc = Rect::from_min_size(glyph, vec2(13.0, 13.0));
    check_box_glyph(&p, rc, *checked, State::of(&resp, enabled));
    let color = if enabled {
        LIGHT_TEXT()
    } else {
        LIGHT_DISABLED()
    };
    text(&p, english.x + 16.0, glyph.y + 10.0, &shown, color);
    if cut {
        resp.on_hover_text(caption)
    } else {
        resp
    }
}

/// An era theme's check box (or, `round`, radio button) in `rc` (13 × 13), `on` when checked:
/// Windows XP's, Windows 7's or Mac OS X's (`palette::CheckLook`). False, drawing nothing, for
/// the other looks.
pub(super) fn era_check(p: &Painter, rc: Rect, on: bool, state: State, round: bool) -> bool {
    use crate::derive::mix;
    use crate::palette::CheckLook;
    let look = crate::theme::current().controls.check;
    if !matches!(look, CheckLook::Luna | CheckLook::Aero | CheckLook::Aqua) {
        return false;
    }
    let c = crate::theme::colors();
    let m = crate::theme::metrics();
    let radius = if round { 6.5 } else { m.check_radius };
    let disabled = state == State::Disabled;
    let (fill_c, edge) = fluent_box(state);
    let fill_c = fill_c.unwrap_or(c.field_disabled);
    let stroke = |rc: Rect, r: f32, ink: Color32| {
        p.rect_stroke(rc, r, Stroke::new(m.border, ink), StrokeKind::Inside);
    };
    // The mark: a tick, or a radio's dot (shaded as the theme's gloss shades).
    let mark = |ink: Color32, width: f32| {
        if round {
            let dot = Rect::from_center_size(rc.center(), vec2(5.0, 5.0));
            fill_shaded(p, dot, 2.5, &[(0.0, mix(ink, c.gloss, 0.45)), (1.0, ink)]);
        } else {
            p.line(
                vec![
                    pos2(rc.left() + 3.0, rc.top() + 6.0),
                    pos2(rc.left() + 5.5, rc.top() + 8.5),
                    pos2(rc.left() + 10.0, rc.top() + 3.5),
                ],
                Stroke::new(width, ink),
            );
        }
    };
    match look {
        // Windows XP: a dark edge, the inside shaded from its top, an orange glow inside under
        // the pointer; a thick tick (a dot) in the checked colour.
        CheckLook::Luna => {
            let inner = rc.shrink(m.border);
            fill_shaded(
                p,
                inner,
                (radius - m.border).max(0.0),
                &[(0.0, mix(fill_c, edge, 0.18)), (1.0, fill_c)],
            );
            if state == State::Hot && c.button_glow.a() > 0 {
                stroke(inner, (radius - 1.0).max(0.0), c.button_glow);
                stroke(inner.shrink(1.0), (radius - 2.0).max(0.0), c.button_glow);
            }
            stroke(rc, radius, edge);
            if on {
                let ink = if disabled {
                    c.check_on_disabled
                } else {
                    fluent_accent(state)
                };
                mark(ink, 2.5);
            }
        }
        // Windows 7: a quiet edge (the accent's under the pointer), a white margin and a
        // shaded square inside; the tick in the mark's colour.
        CheckLook::Aero => {
            p.rect_filled(rc, radius, c.field);
            stroke(rc, radius, edge);
            if !round || !on {
                let inner = rc.shrink(3.0);
                fill_shaded(
                    p,
                    inner,
                    if round { 3.5 } else { 0.0 },
                    &[(0.0, mix(fill_c, edge, 0.35)), (1.0, fill_c)],
                );
            }
            if on {
                let ink = if disabled {
                    c.check_on_disabled
                } else if round {
                    fluent_accent(state)
                } else {
                    c.check_mark
                };
                mark(ink, 2.0);
            }
        }
        // Mac OS X: a glossy box, round corners; checked, the accent's gel and a dark mark.
        CheckLook::Aqua => {
            let face = if on && !disabled {
                fluent_accent(state)
            } else {
                fill_c
            };
            fill_shaded(p, rc, radius, &Shine::Aqua.stops(face, c.gloss));
            let edge = if on && !disabled {
                crate::derive::shade(face, -0.15)
            } else {
                edge
            };
            stroke(rc, radius, edge);
            if on {
                let ink = if disabled {
                    c.check_on_disabled
                } else {
                    c.check_mark
                };
                if round {
                    p.circle_filled(rc.center(), 2.5, ink);
                } else {
                    mark(ink, 2.0);
                }
            }
        }
        _ => {}
    }
    true
}

pub fn check_box_glyph(p: &Painter, rc: Rect, checked: bool, state: State) {
    if era_check(p, rc, checked, state, false) {
        return;
    }
    // KDE: the box's edge in the accent under the pointer; checked, the accent's edge around
    // its inside tinted with it, a tick in the text's colour.
    if crate::theme::current().controls.check == crate::palette::CheckLook::Breeze {
        let m = crate::theme::metrics();
        let (fill_c, border) = breeze_box(state, checked);
        if let Some(f) = fill_c {
            p.rect_filled(rc, m.check_radius, f);
        }
        p.rect_stroke(
            rc,
            m.check_radius,
            Stroke::new(m.border, border),
            StrokeKind::Inside,
        );
        if checked {
            p.line(
                vec![
                    pos2(rc.left() + 3.0, rc.top() + 6.5),
                    pos2(rc.left() + 5.5, rc.top() + 9.0),
                    pos2(rc.left() + 10.0, rc.top() + 4.0),
                ],
                Stroke::new(2.0, breeze_mark(state)),
            );
        }
        return;
    }
    // GNOME: a 2-point border at rest (its inside tinted under the pointer), the accent filled
    // with a light tick, 2 points wide, when checked.
    if crate::theme::current().controls.check == crate::palette::CheckLook::Adwaita {
        let m = crate::theme::metrics();
        if checked {
            p.rect_filled(rc, m.check_radius, fluent_accent(state));
            p.line(
                vec![
                    pos2(rc.left() + 3.0, rc.top() + 6.5),
                    pos2(rc.left() + 5.5, rc.top() + 9.0),
                    pos2(rc.left() + 10.0, rc.top() + 4.0),
                ],
                Stroke::new(2.0, fluent_mark()),
            );
        } else {
            let (fill_c, border) = fluent_box(state);
            if let Some(f) = fill_c {
                p.rect_filled(rc, m.check_radius, f);
            }
            p.rect_stroke(
                rc,
                m.check_radius,
                Stroke::new(2.0, border),
                StrokeKind::Inside,
            );
        }
        return;
    }
    // Windows 8: the box keeps its inside and border (the border in the accent under the
    // pointer), a tick in the accent.
    if crate::theme::current().controls.check == crate::palette::CheckLook::Square {
        let (fill_c, border) = fluent_box(state);
        let m = crate::theme::metrics();
        if let Some(f) = fill_c {
            p.rect_filled(rc, m.check_radius, f);
        }
        let border = if matches!(state, State::Hot | State::Pressed) {
            accent()
        } else {
            border
        };
        p.rect_stroke(
            rc,
            m.check_radius,
            Stroke::new(m.border, border),
            StrokeKind::Inside,
        );
        if checked {
            let ink = if state == State::Disabled {
                crate::theme::colors().check_on_disabled
            } else {
                fluent_accent(state)
            };
            p.line(
                vec![
                    pos2(rc.left() + 2.5, rc.top() + 6.5),
                    pos2(rc.left() + 5.5, rc.top() + 9.5),
                    pos2(rc.left() + 10.5, rc.top() + 3.0),
                ],
                Stroke::new(2.0, ink),
            );
        }
        return;
    }
    if checked {
        p.rect_filled(
            rc,
            crate::theme::metrics().check_radius,
            fluent_accent(state),
        );
        p.line(
            vec![
                pos2(rc.left() + 3.0, rc.top() + 6.5),
                pos2(rc.left() + 5.5, rc.top() + 9.0),
                pos2(rc.left() + 10.0, rc.top() + 3.5),
            ],
            Stroke::new(1.4, fluent_mark()),
        );
    } else {
        let (fill_c, border) = fluent_box(state);
        let m = crate::theme::metrics();
        if let Some(f) = fill_c {
            p.rect_filled(rc, m.check_radius, f);
        }
        p.rect_stroke(
            rc,
            m.check_radius,
            Stroke::new(m.border, border),
            StrokeKind::Inside,
        );
    }
}

/// A Windows 11 check box that is partly ticked (a folder with some of its files chosen): the
/// accent box with a dash, as Windows draws it (a square sat on the unticked box before).
pub fn check_box_mixed(p: &Painter, rc: Rect, state: State) {
    p.rect_filled(
        rc,
        crate::theme::metrics().check_radius,
        fluent_accent(state),
    );
    let y = rc.center().y.floor() + 0.5;
    p.line_segment(
        [pos2(rc.left() + 3.0, y), pos2(rc.right() - 3.0, y)],
        Stroke::new(1.4, fluent_mark()),
    );
}

/// A checked Windows 11 check box's or radio's accent in `state`: lighter under the pointer,
/// lighter still while pressed, grey when disabled.
pub(super) fn fluent_accent(state: State) -> Color32 {
    let c = crate::theme::colors();
    match state {
        State::Disabled => c.check_on_disabled,
        State::Hot => c.check_on_hot,
        State::Pressed => c.check_on_pressed,
        _ => c.check_on,
    }
}

/// The mark on a checked box or radio (white on the light accent's blue, dark on the dark's).
pub(super) fn fluent_mark() -> Color32 {
    crate::theme::colors().check_mark
}

/// A KDE check box's or radio's inside (none when disabled) and edge: the accent's edge under
/// the pointer or pressed, and when checked, around an inside tinted with the accent.
pub(super) fn breeze_box(state: State, checked: bool) -> (Option<Color32>, Color32) {
    let c = crate::theme::colors();
    let (fill_c, border) = fluent_box(state);
    let edge = match state {
        State::Disabled if checked => c.check_on_disabled,
        State::Disabled => border,
        _ if checked => fluent_accent(state),
        State::Hot | State::Pressed => c.check_on_hot,
        _ => border,
    };
    let inside = match (checked, fill_c) {
        (true, Some(f)) => Some(crate::derive::mix(f, fluent_accent(state), 0.3)),
        (_, f) => f,
    };
    (inside, edge)
}

/// The tick or dot on a checked KDE box or radio: the text's colour, greyed when disabled.
pub(super) fn breeze_mark(state: State) -> Color32 {
    let c = crate::theme::colors();
    if state == State::Disabled {
        c.check_on_disabled
    } else {
        c.check_mark
    }
}

/// An unchecked Windows 11 check box's or radio's fill (none when disabled) and border in
/// `state`: darker fill under the pointer, darker still and a fainter border while pressed.
pub(super) fn fluent_box(state: State) -> (Option<Color32>, Color32) {
    fluent_box_in(state, crate::theme::colors())
}

pub(super) fn fluent_box_in(
    state: State,
    c: &crate::palette::Palette,
) -> (Option<Color32>, Color32) {
    match state {
        State::Disabled => (None, c.check_border_disabled),
        State::Hot => (Some(c.check_hot), c.check_border_hot),
        State::Pressed => (Some(c.check_pressed), c.check_border_pressed),
        _ => (Some(c.check), c.check_border),
    }
}

/// Win11 light radio glyph (13 × 13) with its label at glyph.x + 16. The caller sets the
/// choice when the response is clicked.
pub fn radio_light(
    ui: &mut Ui,
    glyph: Pos2,
    selected: bool,
    caption: &str,
    enabled: bool,
) -> Response {
    radio_light_until(ui, glyph, selected, caption, enabled, None)
}

/// [`radio_light`] whose caption also stops before English x `until` (a control after it).
pub fn radio_light_until(
    ui: &mut Ui,
    glyph: Pos2,
    selected: bool,
    caption: &str,
    enabled: bool,
    until: Option<f32>,
) -> Response {
    let caption = crate::i18n::t(caption);
    let p = ui.painter().clone();
    // A caption longer than its room ends in "..." at the edge, whole on hover (UI rule).
    let (shown, cut) = fit_caption_until(&p, glyph.x + 16.0, caption, until);
    let w = text_width(&p, &shown);
    // `glyph` is English; the glyph and its caption (after it) are placed on screen.
    let english = glyph;
    let hit = rect(Rect::from_min_size(english, vec2(16.0 + w + 2.0, 13.0)));
    let glyph = at(english, 13.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(
        hit,
        widget_id(ui, ("radio-light", glyph.x as i32, glyph.y as i32)),
        sense,
    );
    pass_focus_on(ui, &resp, enabled);
    let resp = enter_is_not_a_click(ui, resp);
    focus_on_press(&resp);
    show_focus(ui, &resp, hit.expand(1.0));
    expose(
        &resp,
        egui::Role::RadioButton,
        caption,
        enabled,
        Some(selected),
    );
    let c = glyph + vec2(6.5, 6.5);
    let state = State::of(&resp, enabled);
    // The era themes: their own round radios.
    if era_check(
        &p,
        Rect::from_center_size(c, vec2(13.0, 13.0)),
        selected,
        state,
        true,
    ) {
        // Drawn.
    } else if crate::theme::current().controls.check == crate::palette::CheckLook::Breeze {
        // KDE: as its check box, round; the dot in the text's colour.
        let (fill_c, edge) = breeze_box(state, selected);
        if let Some(f) = fill_c {
            p.circle_filled(c, 6.0, f);
        }
        p.circle_stroke(c, 6.0, Stroke::new(1.0, edge));
        if selected {
            p.circle_filled(c, 2.5, breeze_mark(state));
        }
    } else if selected {
        p.circle_filled(c, 6.5, fluent_accent(state));
        // Windows 11's dot grows under the pointer and shrinks while pressed.
        let dot = match state {
            State::Hot => 3.1,
            State::Pressed => 2.1,
            _ => 2.6,
        };
        p.circle_filled(c, dot, fluent_mark());
    } else {
        let (fill_c, ring) = fluent_box(state);
        if let Some(f) = fill_c {
            p.circle_filled(c, 6.0, f);
        }
        // GNOME's ring is two points wide, as its check boxes' edge.
        let adwaita = crate::theme::current().controls.check == crate::palette::CheckLook::Adwaita;
        if adwaita {
            p.circle_stroke(c, 5.5, Stroke::new(2.0, ring));
        } else {
            p.circle_stroke(c, 6.0, Stroke::new(1.0, ring));
        }
        // Pressed, Windows 11 shows the dot to come.
        if state == State::Pressed && !adwaita {
            p.circle_filled(c, 2.1, ring);
        }
    }
    let color = if enabled {
        LIGHT_TEXT()
    } else {
        LIGHT_DISABLED()
    };
    text(&p, english.x + 16.0, glyph.y + 10.0, &shown, color);
    if cut {
        resp.on_hover_text(caption)
    } else {
        resp
    }
}
