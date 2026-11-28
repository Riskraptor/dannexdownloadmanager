//! Drop-down lists and spin boxes: the dark dialogs' combo boxes and the light ones', their
//! drop-down list and keys, and the spin boxes' arrows.

use super::*;

/// An editable or drop-list combo's arrow zone (inside its border, at the trailing edge).
pub(super) fn combo_zone(rc: Rect) -> Rect {
    if rtl() {
        Rect::from_min_max(
            rc.min + vec2(1.0, 1.0),
            pos2(rc.left() + 20.0, rc.bottom() - 1.0),
        )
    } else {
        Rect::from_min_max(
            pos2(rc.right() - 20.0, rc.top() + 1.0),
            rc.max - vec2(1.0, 1.0),
        )
    }
}

/// A combo's chevron: 8 × 6 "v" at the arrow zone's left + 6, top + 8.
pub(super) fn combo_chevron(p: &Painter, rc: Rect) {
    let arrow_left = if rtl() {
        rc.left() + 1.0
    } else {
        rc.right() - 1.0 - 19.0
    };
    let (cx, cy) = (arrow_left + 6.0, rc.top() + 8.0);
    p.line(
        vec![
            pos2(cx, cy + 0.5),
            pos2(cx + 4.0, cy + 4.5),
            pos2(cx + 8.0, cy + 0.5),
        ],
        Stroke::new(
            crate::theme::metrics().chevron_width,
            crate::theme::colors().combo_chevron,
        ),
    );
}

/// Combo frame. Dark: 1 px border (#8d8d8d, or #0078d4 when focused), 2 px white ring, dark
/// text area, white 19 px arrow zone with a grey chevron. Height 21.
pub fn combo_frame(p: &Painter, rc: Rect, focused: bool) {
    // A text box with the drop-down's chevron (an editable drop-down is one), in both themes.
    edit_frame(p, rc, true);
    if focused {
        focus_bottom(p, rc);
    }
    combo_chevron(p, rc);
}

/// Drop-list combo showing `value`; returns the click response.
pub fn combo(ui: &mut Ui, rc: Rect, value: &str, id: &str) -> Response {
    let resp = ui.interact(rc, widget_id(ui, ("combo", id)), Sense::click());
    let resp = opener_click(ui, resp);
    expose_value(&resp, egui::Role::ComboBox, value, true);
    // A drop-down list is drawn as Windows 11 draws them (and as combo_light does): a button's
    // face, its state across the whole of it, focus shown as a button's.
    dropdown_frame_light(ui.painter(), rc, State::of(&resp, true));
    combo_chevron(ui.painter(), rc);
    show_focus(ui, &resp, rc.shrink(2.0));
    // Short of the arrow zone: a longer value ends in "..." and shows whole on hover.
    let shown = ellipsize(ui.painter(), &tahoma(), value, rc.width() - 5.0 - 22.0);
    text_start(
        ui.painter(),
        tahoma(),
        lead(rc) + 5.0 * fwd(),
        rc.top() + 13.0,
        &shown,
        TEXT(),
    );
    if shown != value {
        resp.on_hover_text(value)
    } else {
        resp
    }
}

/// Editable combo (Add URL "Address", Save As): frame + edit child at +3,+3 .. right−20 (the
/// arrow zone at the trailing edge).
/// Its blue focus border while its box has the keyboard focus.
pub fn combo_edit(ui: &mut Ui, rc: Rect, value: &mut String) -> Response {
    // The frame goes under the box, so it is drawn first, with the box's focus as last frame
    // left it (a change repaints at once).
    let key = widget_id(ui, ("combo-edit-focus", rc.min.x as i32, rc.min.y as i32));
    let was = ui.data(|d| d.get_temp::<bool>(key).unwrap_or(false));
    combo_frame(ui.painter(), rc, was);
    let child = if rtl() {
        Rect::from_min_max(pos2(rc.left() + 20.0, rc.top()), rc.max - vec2(3.0, 0.0))
    } else {
        Rect::from_min_max(
            rc.min + vec2(3.0, 0.0),
            pos2(rc.right() - 20.0, rc.bottom()),
        )
    };
    let resp = edit_text(ui, child, value, true, false, 2.0);
    // Read before the data lock: has_focus asks the context too, and inside it hung the app
    // the moment the box took the focus (0.2.2: Add URL froze as it opened).
    let focused = resp.has_focus();
    if focused != was {
        ui.data_mut(|d| d.insert_temp(key, focused));
        ui.ctx().request_repaint();
    }
    resp
}

/// The arrow zone of an editable combo (`combo_edit`, at the trailing edge): a click on it
/// opens the combo's drop list (`drop_list`); it shows hot, pressed, and pressed while `open`.
pub fn combo_arrow(ui: &mut Ui, rc: Rect, id: &str, open: bool) -> Response {
    let zone = combo_zone(rc);
    let resp = ui.interact(zone, widget_id(ui, ("combo-arrow", id)), pointer_only());
    let resp = opener_click(ui, resp);
    let state = State::of(&resp, true).open_if(open);
    let c = crate::theme::colors();
    let tint = match state {
        State::Hot => Some(c.combo_arrow_hot),
        State::Pressed | State::Open => Some(c.combo_arrow_pressed),
        _ => None,
    };
    tint_part(ui.painter(), zone, tint, |p| combo_chevron(p, rc));
    resp
}

pub fn combo_keys(ui: &mut Ui, edit: &Response) -> bool {
    edit.has_focus()
        && ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowDown)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::F4)
        })
}

/// An editable combo's drop list under `anchor`: `items` newest first, or `empty` greyed when
/// there are none. A window of its own, as the menus are (`chrome::popup`), so a list longer
/// than the dialog is never cut off, and the arrow keys, Enter and Escape work in it. Returns the
/// index picked; `open` turns false on a pick, on Escape or on a click anywhere else.
pub fn drop_list(
    ui: &mut Ui,
    id: &str,
    anchor: Rect,
    items: &[String],
    empty: &str,
    open: &mut bool,
) -> Option<usize> {
    use crate::chrome::{MenuEntry, PopupResult, label};
    // A menu label: long addresses shortened in the middle, "&" kept as text (not an access key).
    let shown = |t: &str| {
        let n = t.chars().count();
        let short: String = if n > 72 {
            t.chars()
                .take(36)
                .chain(std::iter::once('\u{2026}'))
                .chain(t.chars().skip(n - 35))
                .collect()
        } else {
            t.to_owned()
        };
        label(&short.replace('&', "&&"))
    };
    let entries: Vec<MenuEntry<usize>> = if items.is_empty() {
        vec![MenuEntry::Item(label(empty), usize::MAX, false, false)]
    } else {
        items
            .iter()
            .take(crate::settings::HISTORY)
            .enumerate()
            .map(|(i, t)| MenuEntry::Item(shown(t), i, true, false))
            .collect()
    };
    // Under the box's leading edge (its right one in a right-to-left interface, where menus
    // open leftwards from their top-right).
    let at = if rtl() {
        anchor.right_bottom()
    } else {
        anchor.left_bottom()
    };
    let menu = crate::chrome::popup(
        ui.ctx(),
        crate::chrome::MenuKind::List,
        &format!("drop-{id}"),
        at,
        &entries,
        &|_| None,
    );
    match menu {
        PopupResult::Chosen(i) => {
            *open = false;
            Some(i)
        }
        PopupResult::Dismissed => {
            *open = false;
            None
        }
        PopupResult::Open | PopupResult::Neighbor(_) => None,
    }
}

/// How wide a [`combo_light`] must be for each of its translated `items` to show whole before
/// its chevron (the widest decides, so the box keeps its size as the choice changes): at least
/// `min` (its English width: English items fit it, so English never changes), at most `max`
/// (past it the choice ends in "..." and shows whole on hover, as before).
pub fn combo_fit_width(p: &Painter, items: &[&str], min: f32, max: f32) -> f32 {
    let widest = items
        .iter()
        .map(|item| text_width_font(p, tahoma(), crate::i18n::t(item)))
        .fold(0.0, f32::max);
    // combo_light's room for its label: 5 px before it, 19 px for the chevron after it.
    (widest + 5.0 + 19.0).ceil().clamp(min, max.max(min))
}

/// A spin box (Windows 11's number box): a whole number in `min..=max` with its up and down
/// arrows inside the box's trailing end, one control (they were two buttons beside a box).
/// Typing, the keys and the range as [`number_field`]; the arrows step by 1, held they repeat.
#[allow(clippy::too_many_arguments)]
pub fn spin_box(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    min: u64,
    max: u64,
    enabled: bool,
    id: &str,
) -> Response {
    let (resp, step) = spin_frame(ui, rc, enabled, id, |ui, text| {
        number_text(ui, text, value, min, max, enabled)
    });
    if step != 0 {
        let n = value.parse::<i64>().unwrap_or(min as i64) + step as i64;
        *value = n.clamp(min as i64, max as i64).to_string();
    }
    let ok = value.parse::<u64>().is_ok_and(|n| (min..=max).contains(&n));
    if enabled && !ok {
        error_frame(ui.painter(), rc);
    }
    resp
}

/// A spin box's frame and arrows round `content` (its text, drawn in the room left of the
/// arrows: given that rect): the one look of every box with arrows (numbers, a letter, a
/// time). Returns the content's response and the arrows' step (+1, −1, 0).
pub fn spin_frame(
    ui: &mut Ui,
    rc: Rect,
    enabled: bool,
    id: &str,
    content: impl FnOnce(&mut Ui, Rect) -> Response,
) -> (Response, i32) {
    edit_frame(ui.painter(), rc, enabled);
    let zone = spin_zone(rc);
    let text = if rtl() {
        Rect::from_min_max(pos2(zone.right(), rc.top()), rc.max)
    } else {
        Rect::from_min_max(rc.min, pos2(zone.left(), rc.bottom()))
    };
    let resp = content(ui, text);
    let step = spin_arrows(ui, zone, enabled, id);
    edit_focus_line(ui, &resp, rc);
    (resp, step)
}

/// The arrows' place inside a spin box (on screen): its trailing 18 px within the border (the
/// leading ones in a right-to-left interface).
pub fn spin_zone(rc: Rect) -> Rect {
    if rtl() {
        Rect::from_min_max(
            pos2(rc.left() + 1.0, rc.top() + 1.0),
            pos2(rc.left() + 19.0, rc.bottom() - 1.0),
        )
    } else {
        Rect::from_min_max(
            pos2(rc.right() - 19.0, rc.top() + 1.0),
            pos2(rc.right() - 1.0, rc.bottom() - 1.0),
        )
    }
}

/// The 8 × 5 "v" of drop-downs, its top-left at `at`.
pub fn chevron_light(p: &Painter, at: Pos2, enabled: bool) {
    let c = if enabled {
        CHEVRON()
    } else {
        CHEVRON_DISABLED()
    };
    p.line(
        vec![
            at + vec2(0.5, 0.5),
            at + vec2(4.0, 4.0),
            at + vec2(7.5, 0.5),
        ],
        Stroke::new(1.2, c),
    );
}

pub fn dropdown_frame_light(p: &Painter, rc: Rect, state: State) {
    // Drawn as a button (Windows 11 draws its drop-downs so): the same faces and states, in
    // the current theme; open, the accent border and no bottom edge (its list is out).
    let open = state == State::Open;
    let kind = if state == State::Disabled {
        ButtonKind::Disabled
    } else {
        ButtonKind::Normal
    };
    let face = button_colors(
        kind,
        if open { State::Hot } else { state },
        crate::theme::colors(),
    );
    // Open: the accent all round (its bottom too: the list is out).
    let face = if open {
        Face {
            border: accent(),
            bottom: accent(),
            ..face
        }
    } else {
        face
    };
    paint_button(p, rc, kind, state, &face, Shape::DropDown);
}

/// Light drop-down list (CBS_DROPDOWNLIST): the chosen item and a chevron; a click opens the
/// list under it. Returns the item chosen this frame.
pub fn combo_light(
    ui: &mut Ui,
    rc: Rect,
    items: &[&str],
    selected: usize,
    enabled: bool,
    id: &str,
) -> Option<usize> {
    let key = widget_id(ui, ("combo-light", id));
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(rc, key.with("button"), sense);
    pass_focus_on(ui, &resp, enabled);
    let resp = opener_click(ui, resp);
    focus_on_press(&resp);
    keep_keys(ui, &resp, false, true, false);
    let mut open = ui.data(|d| d.get_temp::<bool>(key).unwrap_or(false)) && enabled;
    let popup_id = format!("combo-light-{}", key.short_debug_format());
    let mut chosen = None;
    let last = items.len().saturating_sub(1);
    // egui clicks a focused control on Enter too: on a closed one Enter is the dialog's default
    // button, not an opener.
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    if resp.clicked() && !enter {
        open = !open;
    }
    // The keys, as a Windows drop-down list has them (an open list's are the popup's).
    if resp.has_focus() && enabled && !items.is_empty() && !open {
        let opener = ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowDown)
                || i.consume_key(egui::Modifiers::NONE, egui::Key::F4)
        });
        if opener {
            open = true;
            crate::chrome::start_at(ui.ctx(), &popup_id, selected.min(last));
        }
        let to = if take_key(ui, egui::Key::ArrowDown) {
            Some((selected + 1).min(last))
        } else if take_key(ui, egui::Key::ArrowUp) {
            Some(selected.saturating_sub(1))
        } else if take_key(ui, egui::Key::Home) {
            Some(0)
        } else if take_key(ui, egui::Key::End) {
            Some(last)
        } else {
            None
        };
        // Typing picks the item starting with it (the arrows change the choice too).
        let typed = typed_match(ui, key.with("typed"), items.len(), Some(selected), |i| {
            crate::i18n::t(items[i]).to_owned()
        });
        // Closed: the arrows change the choice itself.
        if let Some(i) = to.or(typed).filter(|i| *i != selected) {
            chosen = Some(i);
        }
    }
    let p = ui.painter().clone();
    dropdown_frame_light(&p, rc, State::of(&resp, enabled).open_if(open));
    let color = if enabled {
        LIGHT_TEXT()
    } else {
        LIGHT_DISABLED()
    };
    // Items are shown in the interface language (data such as numbers passes through as is).
    let full = crate::i18n::t(items.get(selected).copied().unwrap_or(""));
    expose_value(&resp, egui::Role::ComboBox, full, enabled);
    if enabled && let Some(i) = assistive_choice(&resp, items, &mut open) {
        chosen = Some(i).filter(|i| *i != selected);
    }
    // Cut with "..." before the chevron when too long (a translation), whole on hover.
    let label = ellipsize(&p, &tahoma(), full, rc.width() - 5.0 - 19.0);
    let drawn = text_start(
        &p,
        tahoma(),
        lead(rc) + 5.0 * fwd(),
        rc.top() + 13.0,
        &label,
        color,
    );
    if label != full && !open {
        full_on_hover(ui, drawn, ("combo-light-label", id), full);
    }
    // The chevron at the trailing edge.
    let chevron_x = if rtl() {
        rc.left() + 5.0
    } else {
        rc.right() - 13.0
    };
    chevron_light(&p, pos2(chevron_x, rc.top() + 8.0), enabled);
    // Over the face (drawn before it, the face hid it).
    show_focus(ui, &resp, rc.shrink(3.0));
    if open {
        // The list: the shared popup (a window of its own, never clipped by the dialog, kept on
        // the screen; the keys, the pointer), the choice marked.
        use crate::chrome::{MenuEntry, PopupResult, label as menu_label};
        let entries: Vec<MenuEntry<usize>> = items
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let shown = crate::i18n::t(s).replace('&', "&&");
                MenuEntry::Radio(menu_label(&shown), i, true, i == selected)
            })
            .collect();
        let at = if rtl() {
            rc.right_bottom()
        } else {
            rc.left_bottom()
        };
        match crate::chrome::popup(
            ui.ctx(),
            crate::chrome::MenuKind::List,
            &popup_id,
            at,
            &entries,
            &|_| None,
        ) {
            PopupResult::Chosen(i) => {
                open = false;
                chosen = Some(i).filter(|i| *i != selected);
                resp.request_focus();
            }
            PopupResult::Dismissed => {
                open = false;
                resp.request_focus();
            }
            PopupResult::Open | PopupResult::Neighbor(_) => {}
        }
        // The keys went to the list: the dialog's default button doesn't take Enter too.
        take_key(ui, egui::Key::Enter);
    }
    ui.data_mut(|d| d.insert_temp(key, open));
    chosen
}

/// A spin box's arrows in `zone` ([`spin_zone`]): up and down halves inside the box, without
/// borders of their own, tinted under the pointer and pressed as the box's other parts are.
/// Returns +1 / −1 for a click (held: repeats), 0 otherwise.
pub fn spin_arrows(ui: &mut Ui, rc: Rect, enabled: bool, id: &str) -> i32 {
    let p = ui.painter().clone();
    let half = (rc.height() / 2.0).floor();
    let up = Rect::from_min_size(rc.min, vec2(rc.width(), half));
    let down = Rect::from_min_max(pos2(rc.left(), rc.top() + half), rc.max);
    let mut step = 0;
    for (i, part, dir) in [(0, up, 1), (1, down, -1)] {
        let sense = if enabled {
            pointer_only()
        } else {
            Sense::hover()
        };
        let r = ui.interact(part, widget_id(ui, ("updown-light", id, i)), sense);
        pass_focus_on(ui, &r, enabled);
        let name = if dir > 0 {
            // i18n: a spin box's arrow, for screen readers: one more
            crate::i18n::t("Increase")
        } else {
            // i18n: a spin box's arrow, for screen readers: one less
            crate::i18n::t("Decrease")
        };
        expose(&r, egui::Role::Button, name, enabled, None);
        let state = if !enabled {
            State::Disabled
        } else if r.is_pointer_button_down_on() {
            State::Pressed
        } else if r.hovered() {
            State::Hot
        } else {
            State::Normal
        };
        if let Some(tint) = subtle_fill(state, crate::theme::colors()) {
            p.rect_filled(part.shrink(1.0), crate::theme::metrics().part_radius, tint);
        }
        let c = part.center();
        let arrow = if enabled {
            LIGHT_TEXT()
        } else {
            CHEVRON_DISABLED()
        };
        let pts = if dir > 0 {
            vec![c + vec2(-3.0, 1.5), c + vec2(3.0, 1.5), c + vec2(0.0, -1.5)]
        } else {
            vec![
                c + vec2(-3.0, -1.5),
                c + vec2(3.0, -1.5),
                c + vec2(0.0, 1.5),
            ]
        };
        p.add(egui::Shape::convex_polygon(pts, arrow, Stroke::NONE));
        // Held, it keeps counting.
        if auto_repeat(ui, &r) {
            step = dir;
        }
    }
    step
}
