//! Text boxes: the dark dialogs' and the light ones', read-only fields, number, password and
//! multiline boxes, their history keys and their right-click edit menu.

use super::*;

/// What a text box allows: typing, nothing (greyed), or reading and copying.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldLook {
    Editable,
    Disabled,
    ReadOnly,
}

#[derive(Clone, Copy)]
pub(super) enum EditCmd {
    Undo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
}

/// A text box's frame, in both themes Windows 11's: rounded as the buttons are, a quiet 1 px
/// border and, while it can be typed in, a stronger bottom line (the focus turns it into the
/// accent: [`focus_bottom`]). Disabled and read-only boxes are flat: a disabled one dims its
/// text, a read-only one keeps it (it can be read and copied) on a light grey. The frame paints
/// the box's inside too, so every kind of text box agrees (they had three insides between them,
/// and dark mode drew IDM's bright 2 px border round every box, disabled ones included).
pub fn field_frame(p: &Painter, rc: Rect, look: FieldLook) {
    let radius = control_radius();
    let c = crate::theme::colors();
    // Disabled and read-only boxes are flat: their bottom line is their border.
    let (inside, border, bottom) = match look {
        FieldLook::Editable => (c.field, c.field_border, c.field_bottom),
        FieldLook::Disabled => (
            c.field_disabled,
            c.field_disabled_border,
            c.field_disabled_border,
        ),
        FieldLook::ReadOnly => (
            c.field_readonly,
            c.field_readonly_border,
            c.field_readonly_border,
        ),
    };
    p.rect_filled(rc, radius, inside);
    p.rect_stroke(
        rc,
        radius,
        Stroke::new(crate::theme::metrics().border, border),
        StrokeKind::Inside,
    );
    hline(
        p,
        rc.left() + radius,
        rc.right() - radius - 1.0,
        rc.bottom() - 1.0,
        bottom,
    );
}

/// A box whose value is wrong (a number out of range, an address that isn't one, a schedule
/// that can't start): the error colour round it, on the control's own corners (it was drawn
/// three ways).
pub fn error_frame(p: &Painter, rc: Rect) {
    p.rect_stroke(
        rc,
        control_radius(),
        Stroke::new(crate::theme::metrics().border, ERROR()),
        StrokeKind::Inside,
    );
}

/// [`field_frame`] of a box that can be typed in, or not.
pub fn edit_frame(p: &Painter, rc: Rect, enabled: bool) {
    field_frame(
        p,
        rc,
        if enabled {
            FieldLook::Editable
        } else {
            FieldLook::Disabled
        },
    );
}

/// A single-line edit: frame + egui text editing positioned so text starts at x+5 with its
/// baseline at top+13.
pub fn edit(ui: &mut Ui, rc: Rect, value: &mut String, enabled: bool, password: bool) -> Response {
    edit_frame(ui.painter(), rc, enabled);
    let resp = edit_text(ui, rc, value, enabled, password, 5.0);
    edit_focus_line(ui, &resp, rc);
    resp
}

/// A text box's focus, as Windows 11 shows it: an accent line along its bottom edge.
pub fn edit_focus_line(ui: &Ui, resp: &Response, rc: Rect) {
    if resp.has_focus() {
        focus_bottom(ui.painter(), rc);
    }
}

/// The 2 px accent along a focused box's bottom edge. Light: the bottom of its rounded
/// frame, curving into the corners as Windows 11 draws it; dark: IDM's straight line.
pub fn focus_bottom(p: &Painter, rc: Rect) {
    let h = crate::theme::metrics().focus_line;
    // GNOME: a ring inside the box, the accent half seen through.
    if crate::theme::current().controls.field_focus == crate::palette::FieldFocus::Ring {
        p.rect_stroke(
            rc,
            control_radius(),
            Stroke::new(2.0, accent().gamma_multiply(0.5)),
            StrokeKind::Inside,
        );
        return;
    }
    // Windows 8 and XP: the whole border in the accent, not a line along the bottom.
    if crate::theme::current().controls.field_focus == crate::palette::FieldFocus::Border {
        p.rect_stroke(
            rc,
            control_radius(),
            Stroke::new(crate::theme::metrics().border, accent()),
            StrokeKind::Inside,
        );
        return;
    }
    if crate::theme::colors().focus_line_straight {
        fill(
            p,
            rc.left() + 1.0,
            rc.bottom() - h,
            rc.right() - 2.0,
            rc.bottom() - 1.0,
            accent(),
        );
    } else {
        let strip = Rect::from_min_max(pos2(rc.left(), rc.bottom() - h), rc.max);
        p.with_clip_rect(strip)
            .rect_filled(rc, control_radius(), accent());
    }
}

pub(super) fn edit_text(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    enabled: bool,
    password: bool,
    inset: f32,
) -> Response {
    let color = if enabled {
        TEXT()
    } else {
        EDIT_TEXT_DISABLED()
    };
    edit_text_with(
        ui,
        rc,
        value,
        enabled,
        password,
        inset,
        color,
        start_align(),
    )
}

/// Text aligned to its start: left, right in RTL.
pub fn start_align() -> egui::Align {
    if rtl() {
        egui::Align::RIGHT
    } else {
        egui::Align::LEFT
    }
}

/// Text aligned to its end (numbers in a box): right, left in RTL.
pub fn end_align() -> egui::Align {
    if rtl() {
        egui::Align::LEFT
    } else {
        egui::Align::RIGHT
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn edit_text_with(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    enabled: bool,
    password: bool,
    inset: f32,
    color: Color32,
    align: egui::Align,
) -> Response {
    // Tahoma −11: ascent 11, so the text cell top is baseline − 10 (top + 3 in a 21 px box),
    // the line centred in the box at any height (baseline_in).
    let base = baseline_in(rc.top(), rc.height());
    let inner = Rect::from_min_max(
        pos2(rc.left() + inset, base - 10.0),
        pos2(rc.right() - inset, base + 3.0),
    );
    // The whole drawn box takes clicks (the text itself fills only a strip inside it): first,
    // so the text on top still gets its own; a click elsewhere in the box focuses it.
    // Keyed by its window's opening too: a box shown again for another record (New login after
    // Edit login) starts without the last one's text in its undo.
    // (The key first: asking the context inside its own lock deadlocks.)
    let opening = crate::dialogs::opening_key(ui.ctx());
    let opened = ui.data(|d| d.get_temp::<u64>(opening)).unwrap_or(0);
    let edit_id = widget_id(ui, ("edit-box", rc.min.x as i32, rc.min.y as i32, opened));
    if enabled {
        let frame = ui.interact(rc, edit_id.with("frame"), pointer_only());
        if frame.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        if frame.clicked() {
            ui.memory_mut(|m| m.request_focus(edit_id));
        }
    }
    // A box that was empty and is then filled by its window, not typed in (a link from the
    // clipboard, read off the window's thread, comes a frame or two after the window opens:
    // #661), has its caret where it would be had it opened with the text (at its end), not left
    // at its start.
    let was_empty = edit_id.with("was-empty");
    if !value.is_empty() && ui.data(|d| d.get_temp::<bool>(was_empty)).is_some() {
        ui.data_mut(|d| d.remove::<bool>(was_empty));
        if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), edit_id) {
            state.cursor.set_char_range(None);
            state.store(ui.ctx(), edit_id);
        }
    }
    let te = egui::TextEdit::singleline(value)
        .id(edit_id)
        .align(egui::Align2([align, egui::Align::TOP]))
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .font(tahoma())
        .text_color(color)
        .password(password)
        // Enter keeps the focus in the box (the dialog's default button answers it; a box that
        // lost the keys on Enter took the next typing nowhere).
        .return_key(None)
        .desired_width(inner.width());
    if ui.memory(|m| m.has_focus(edit_id)) {
        ui.input_mut(|i| {
            let mut events = Vec::with_capacity(i.events.len());
            for e in i.events.drain(..) {
                match e {
                    // Ctrl+V: one clean line (see single_line).
                    egui::Event::Paste(t) => events.push(egui::Event::Paste(single_line(&t))),
                    // A password's word moves would show where its words end: whole-box
                    // moves instead (Ctrl+Left / Right to its ends, Ctrl+Backspace / Delete
                    // to them).
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        physical_key,
                        repeat,
                    } if password
                        && modifiers.command
                        && matches!(
                            key,
                            egui::Key::ArrowLeft
                                | egui::Key::ArrowRight
                                | egui::Key::Backspace
                                | egui::Key::Delete
                        ) =>
                    {
                        let plain = egui::Modifiers {
                            shift: modifiers.shift,
                            ..Default::default()
                        };
                        let select = egui::Modifiers::SHIFT;
                        let press = |key, modifiers| egui::Event::Key {
                            key,
                            physical_key,
                            pressed: true,
                            repeat,
                            modifiers,
                        };
                        match key {
                            egui::Key::ArrowLeft => events.push(press(egui::Key::Home, plain)),
                            egui::Key::ArrowRight => events.push(press(egui::Key::End, plain)),
                            egui::Key::Backspace => {
                                events.push(press(egui::Key::Home, select));
                                events.push(press(egui::Key::Backspace, Default::default()));
                            }
                            _ => {
                                events.push(press(egui::Key::End, select));
                                events.push(press(egui::Key::Delete, Default::default()));
                            }
                        }
                    }
                    other => events.push(other),
                }
            }
            i.events = events;
        });
    }
    let resp = ui.put(inner, |ui: &mut Ui| {
        // The caret in the box's own ink: egui's default (a pale blue in its dark style) was
        // all but invisible on a light box.
        ui.visuals_mut().text_cursor.stroke.color = color;
        ui.add_enabled_ui(enabled, |ui| ui.add(te)).inner
    });
    pass_focus_on(ui, &resp, enabled);
    // Named for screen readers by its caption.
    name_from_caption(&resp);
    // Tab into a box selects its text, as in Windows (a click places the caret instead).
    if resp.gained_focus() && !ui.input(|i| i.pointer.any_down()) {
        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), edit_id).unwrap_or_default();
        let n = value.chars().count();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(n),
            )));
        state.store(ui.ctx(), edit_id);
    }
    if value.is_empty() && ui.data(|d| d.get_temp::<bool>(was_empty)).is_none() {
        ui.data_mut(|d| d.insert_temp(was_empty, true));
    }
    if enabled {
        edit_menu(&resp, value, password, false);
    }
    resp
}

/// A text box's right-click menu: Cut, Copy, Paste, Delete and Select all, on the box's own
/// selection, as every Windows, macOS and Linux text box has (Cut and Copy not for passwords). A
/// menu window of its own (`chrome::popup`), so short dialogs never cut it off.
/// A right-click (or Shift+F10) menu with Copy for read-only text `text` shown by `resp` (a
/// value, a version, an address): the whole text goes to the clipboard. Ctrl+C does the same
/// while `resp` has the focus.
pub fn copy_menu(resp: &Response, text: &str) {
    use crate::chrome::{MenuEntry, PopupResult};
    let ctx = resp.ctx.clone();
    let key = resp.id.with("copy-menu");
    if resp.has_focus() && ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy))) {
        crate::copy_text(&ctx, text.to_owned());
    }
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        ctx.data_mut(|d| d.insert_temp(key, p));
    }
    if resp.has_focus() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10))
    {
        ctx.data_mut(|d| d.insert_temp(key, resp.rect.left_bottom()));
    }
    let Some(at) = ctx.data(|d| d.get_temp::<Pos2>(key)) else {
        return;
    };
    let entries = [MenuEntry::Item("&Copy", (), !text.is_empty(), false)];
    match crate::chrome::popup(
        &ctx,
        crate::chrome::MenuKind::Context,
        &format!("copy-{:?}", resp.id),
        at,
        &entries,
        &|_| None,
    ) {
        PopupResult::Open | PopupResult::Neighbor(_) => {}
        PopupResult::Chosen(()) => {
            crate::copy_text(&ctx, text.to_owned());
            ctx.data_mut(|d| d.remove::<Pos2>(key));
        }
        PopupResult::Dismissed => ctx.data_mut(|d| d.remove::<Pos2>(key)),
    }
}

/// A read-only value in a box (a path, an address): cut to fit with "..." and whole on hover,
/// on `fill`; it takes the focus (Tab), and Ctrl+C or its right-click menu copies the whole
/// value ([`copy_menu`]). `ltr`: the value reads left to right in every language (paths,
/// addresses keep their "..." at their end in a right-to-left interface).
pub fn readonly_field(
    ui: &mut Ui,
    rc: Rect,
    value: &str,
    fill: Color32,
    ltr: bool,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> Response {
    // `fill`: the surface it sits on (kept for its callers; the read-only look has its own).
    let _ = fill;
    readonly_with_icon(ui, rc, value, ltr, id, None)
}

/// [`readonly_field`] holding a file name, after its type's icon (the shared file icons).
pub fn readonly_file(
    ui: &mut Ui,
    rc: Rect,
    name: &str,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> Response {
    let icon = (!name.is_empty()).then_some(crate::fileicons::Kind::File(name));
    readonly_with_icon(ui, rc, name, true, id, icon)
}

pub(super) fn readonly_with_icon(
    ui: &mut Ui,
    rc: Rect,
    value: &str,
    ltr: bool,
    id: impl std::hash::Hash + std::fmt::Debug,
    icon: Option<crate::fileicons::Kind>,
) -> Response {
    field_frame(ui.painter(), rc, FieldLook::ReadOnly);
    let p = ui.painter().clone();
    // From the field's leading edge (an English pen in its English rect).
    let mut e = rect(rc);
    if let Some(kind) = icon {
        let top = e.top() + ((e.height() - 16.0) / 2.0).floor();
        crate::fileicons::paint_at(ui, &p, kind, pos2(e.left() + 3.0, top), 16.0);
        e.min.x += crate::fileicons::INDENT - 2.0;
    }
    let shown = ellipsize(&p, &tahoma(), value, e.width() - 9.0);
    let cut = shown != value;
    let shown = if ltr && rtl() {
        crate::i18n::ltr(&shown)
    } else {
        shown
    };
    text(
        &p,
        e.left() + 5.0,
        baseline_in(e.top(), e.height()),
        &shown,
        LIGHT_TEXT(),
    );
    let resp = ui.interact(rc, widget_id(ui, ("readonly", id)), Sense::click());
    focus_on_press(&resp);
    // A value, not a label: its "&" (an address's query, a file name's) is no mnemonic mark,
    // which `expose` takes out (screen readers read "?seed=32rate=512K" for "?seed=32&rate=512K").
    expose(
        &resp,
        egui::Role::TextInput,
        &value.replace('&', "&&"),
        true,
        None,
    );
    // Focused as every text box shows it: the accent along its bottom.
    if resp.has_focus() {
        focus_bottom(&p, rc);
    }
    copy_menu(&resp, value);
    if cut { resp.on_hover_text(value) } else { resp }
}

pub fn edit_menu(resp: &Response, value: &mut String, password: bool, multiline: bool) {
    use crate::chrome::{MenuEntry, PopupResult};
    use egui::text::{CCursor, CCursorRange};
    use egui::text_edit::TextEditState;
    let ctx = resp.ctx.clone();
    let id = resp.id;
    let key = id.with("edit-menu");
    // Paste is offered when the clipboard has text: asked as the menu opens, read off the
    // window's thread (clipboard.rs; read here at every frame, a clipboard owner that hung froze
    // the window, #661).
    let clip_key = key.with("clipboard");
    let open_at = |p: Pos2| {
        let ask = crate::clipboard::ask();
        ctx.data_mut(|d| {
            d.insert_temp(key, p);
            d.insert_temp(clip_key, ask);
        });
    };
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        open_at(p);
    }
    // Shift+F10 (the keyboard's way to a right-click menu) opens it under the box.
    if resp.has_focus() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10))
    {
        open_at(resp.rect.left_bottom());
    }
    let Some(at) = ctx.data(|d| d.get_temp::<Pos2>(key)) else {
        return;
    };
    let mut state = TextEditState::load(&ctx, id).unwrap_or_default();
    let n = value.chars().count();
    let (a, b) = state
        .cursor
        .char_range()
        .map(|r| {
            let (x, y) = (r.primary.index.0.min(n), r.secondary.index.0.min(n));
            (x.min(y), x.max(y))
        })
        .unwrap_or((n, n));
    let has = a < b;
    // Until the answer comes (a frame or two; never, from an owner that hung), what the
    // clipboard last held.
    let clip = ctx
        .data(|d| d.get_temp::<crate::clipboard::Ask>(clip_key))
        .and_then(crate::clipboard::answer)
        .unwrap_or_else(crate::clipboard::latest)
        .filter(|t| !t.is_empty());
    // egui's own undo history (Ctrl+Z uses it too).
    let mut undoer = state.undoer();
    let now = (state.cursor.char_range().unwrap_or_default(), value.clone());
    let entries = vec![
        MenuEntry::Item("&Undo", EditCmd::Undo, undoer.has_undo(&now), false),
        MenuEntry::Sep,
        MenuEntry::Item("Cu&t", EditCmd::Cut, has && !password, false),
        MenuEntry::Item("&Copy", EditCmd::Copy, has && !password, false),
        MenuEntry::Item("&Paste", EditCmd::Paste, clip.is_some(), false),
        MenuEntry::Item("&Delete", EditCmd::Delete, has, false),
        MenuEntry::Sep,
        MenuEntry::Item("Select &all", EditCmd::SelectAll, n > 0, false),
    ];
    let cmd = match crate::chrome::popup(
        &ctx,
        crate::chrome::MenuKind::Context,
        &format!("edit-{id:?}"),
        at,
        &entries,
        &|_| None,
    ) {
        PopupResult::Open | PopupResult::Neighbor(_) => return,
        PopupResult::Dismissed => None,
        PopupResult::Chosen(c) => Some(c),
    };
    ctx.data_mut(|d| {
        d.remove::<Pos2>(key);
        d.remove::<crate::clipboard::Ask>(clip_key);
    });
    let Some(cmd) = cmd else { return };
    let byte = |c: usize| value.char_indices().nth(c).map_or(value.len(), |(i, _)| i);
    let (ba, bb) = (byte(a), byte(b));
    let caret = match cmd {
        EditCmd::Undo => {
            let Some((range, text)) = undoer.undo(&now).cloned() else {
                return;
            };
            *value = text;
            state.set_undoer(undoer);
            state.cursor.set_char_range(Some(range));
            state.store(&ctx, id);
            ctx.memory_mut(|m| m.request_focus(id));
            return;
        }
        // Each change an undo step of its own (Undo after the menu's Paste did nothing).
        EditCmd::Cut => {
            crate::copy_text(&ctx, value[ba..bb].to_owned());
            undoer.add_undo(&now);
            value.replace_range(ba..bb, "");
            (a, a)
        }
        EditCmd::Copy => {
            crate::copy_text(&ctx, value[ba..bb].to_owned());
            (a, b)
        }
        EditCmd::Paste => {
            // A single-line box takes one line; a list box keeps the lines.
            let text = clip.unwrap_or_default();
            let text = if multiline {
                text.replace("\r\n", "\n")
            } else {
                single_line(&text)
            };
            undoer.add_undo(&now);
            value.replace_range(ba..bb, &text);
            let end = a + text.chars().count();
            (end, end)
        }
        EditCmd::Delete => {
            undoer.add_undo(&now);
            value.replace_range(ba..bb, "");
            (a, a)
        }
        EditCmd::SelectAll => (0, value.chars().count()),
    };
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(caret.0),
        CCursor::new(caret.1),
    )));
    state.set_undoer(undoer);
    state.store(&ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
}

/// A dark edit with right-aligned text (numbers, e.g. the Speed Limiter value).
pub fn edit_right(ui: &mut Ui, rc: Rect, value: &mut String, enabled: bool) -> Response {
    edit_frame(ui.painter(), rc, enabled);
    let color = if enabled {
        TEXT()
    } else {
        EDIT_TEXT_DISABLED()
    };
    let resp = edit_text_with(ui, rc, value, enabled, false, 5.0, color, end_align());
    // Its focus shows as every text box's does.
    edit_focus_line(ui, &resp, rc);
    resp
}

/// The keys that open an editable combo's drop list while its box has the focus: Alt+Down or
/// F4, as Windows' combo boxes.
/// Up / Down in an editable drop-down's box (the list closed): the next / previous of its
/// `n` items, as Windows' editable combos step through theirs. `current`: the one the box
/// shows, if any. Returns the item to put in the box.
pub fn history_keys(ui: &Ui, edit: &Response, n: usize, current: Option<usize>) -> Option<usize> {
    if !edit.has_focus() || n == 0 {
        return None;
    }
    keep_keys(ui, edit, false, true, false);
    if take_key(ui, egui::Key::ArrowDown) {
        Some(current.map_or(0, |c| (c + 1).min(n - 1)))
    } else if take_key(ui, egui::Key::ArrowUp) {
        Some(current.map_or(0, |c| c.saturating_sub(1)))
    } else {
        None
    }
}

/// Text pasted into a one-line box: a trailing line break dropped, line breaks and tabs inside
/// it one space each, other control characters out ("url\r\n" was "url" and two spaces).
pub fn single_line(text: &str) -> String {
    let t = text.trim_end_matches(['\r', '\n']);
    let t = t.replace("\r\n", " ").replace(['\r', '\n', '\t'], " ");
    t.chars().filter(|c| !c.is_control()).collect()
}

/// Light edit: the same Win11 border, white fill, black text.
pub fn edit_light(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    enabled: bool,
    align_right: bool,
) -> Response {
    edit_light_on(ui, rc, value, enabled, align_right, LIGHT_BG())
}

/// A whole number in `min..=max` typed in a box (the one number box of the dialogs): digits
/// only, no longer than `max` has; Up / Down step it by 1, Page Up / Down by 10; out of range,
/// its frame is red while typed and it comes back into range, visibly, when the box is left.
/// [`number_in`] reads it.
#[allow(clippy::too_many_arguments)]
pub fn number_field(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    min: u64,
    max: u64,
    enabled: bool,
    parent: Color32,
) -> Response {
    let _ = parent;
    edit_frame(ui.painter(), rc, enabled);
    let resp = number_text(ui, rc, value, min, max, enabled);
    let ok = value.parse::<u64>().is_ok_and(|n| (min..=max).contains(&n));
    if enabled && !ok {
        error_frame(ui.painter(), rc);
    }
    edit_focus_line(ui, &resp, rc);
    resp
}

/// A plain line of text in a box drawn by its caller (a spin box's letter): the text boxes'
/// typing, caret and menu without a frame of its own.
pub fn field_text(ui: &mut Ui, rc: Rect, value: &mut String, enabled: bool) -> Response {
    let color = if enabled {
        LIGHT_TEXT()
    } else {
        LIGHT_DISABLED()
    };
    edit_text_with(ui, rc, value, enabled, false, 5.0, color, start_align())
}

/// A number box's text and keys: digits only, no longer than `max` has; Up / Down step it by 1,
/// Page Up / Down by 10; left, it comes back into `min..=max`.
pub(super) fn number_text(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    min: u64,
    max: u64,
    enabled: bool,
) -> Response {
    let resp = field_text(ui, rc, value, enabled);
    value.retain(|c| c.is_ascii_digit());
    value.truncate(max.to_string().len());
    if resp.has_focus() {
        keep_keys(ui, &resp, false, true, false);
        let n = value.parse::<u64>().unwrap_or(min) as i64;
        let step = if take_key(ui, egui::Key::ArrowUp) {
            1
        } else if take_key(ui, egui::Key::ArrowDown) {
            -1
        } else if take_key(ui, egui::Key::PageUp) {
            10
        } else if take_key(ui, egui::Key::PageDown) {
            -10
        } else {
            0
        };
        if step != 0 {
            *value = (n + step).clamp(min as i64, max as i64).to_string();
        }
    }
    if resp.lost_focus() {
        *value = number_in(value, min, max).to_string();
    }
    resp
}

/// The number in a [`number_field`], brought into `min..=max` (empty: `min`).
pub fn number_in(value: &str, min: u64, max: u64) -> u64 {
    value.parse::<u64>().unwrap_or(min).clamp(min, max)
}

/// [`edit_light`] on a surface `parent` (a disabled edit shows it: `#f9f9f9` on a tab page).
pub fn edit_light_on(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    enabled: bool,
    align_right: bool,
    parent: Color32,
) -> Response {
    edit_light_with(ui, rc, value, enabled, align_right, false, parent)
}

pub(super) fn edit_light_with(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    enabled: bool,
    align_right: bool,
    password: bool,
    parent: Color32,
) -> Response {
    // `parent`: the surface a box sits on (kept for its callers; the frame paints the inside).
    let _ = parent;
    edit_frame(ui.painter(), rc, enabled);
    let color = if enabled {
        LIGHT_TEXT()
    } else {
        LIGHT_DISABLED()
    };
    // A number keeps to the end (the left in RTL), text to the start.
    let align = if align_right {
        end_align()
    } else {
        start_align()
    };
    let resp = edit_text_with(ui, rc, value, enabled, password, 5.0, color, align);
    edit_focus_line(ui, &resp, rc);
    resp
}

/// A light password box: dots, and no Cut or Copy in its menu.
pub fn password_light(ui: &mut Ui, rc: Rect, value: &mut String, enabled: bool) -> Response {
    password_light_on(ui, rc, value, enabled, LIGHT_BG())
}

/// [`password_light`] on a surface `parent` (shown inside it while disabled).
pub fn password_light_on(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    enabled: bool,
    parent: Color32,
) -> Response {
    edit_light_with(ui, rc, value, enabled, false, true, parent)
}

/// A multi-line text box (lists of sites, notes) that wraps its text and scrolls, with the
/// shared frame, caret, focus line and edit menu. `id` keeps its scroll place.
/// `enabled`: off, it is greyed and can't be typed in (its switch is off).
pub fn multiline_light(
    ui: &mut Ui,
    rc: Rect,
    value: &mut String,
    id: &str,
    enabled: bool,
) -> Response {
    edit_frame(ui.painter(), rc, enabled);
    let ink = if enabled {
        LIGHT_TEXT()
    } else {
        LIGHT_DISABLED()
    };
    // The text left of the shared scroll bar (egui's own was a thin overlay unlike every other
    // list's), which sits inside the frame when the text is taller than the box.
    let track = Rect::from_min_max(
        pos2(rc.right() - 1.0 - crate::scrollbar::THICK, rc.top() + 1.0),
        pos2(rc.right() - 1.0, rc.bottom() - 1.0),
    );
    let inner = Rect::from_min_max(
        rc.min + vec2(4.0, 2.0),
        pos2(track.left() - 2.0, rc.bottom() - 3.0),
    );
    let off_key = egui::Id::unique(("multiline-scroll", id));
    let mut off: f32 = ui.data(|d| d.get_temp(off_key)).unwrap_or(0.0);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).id_salt(id));
    // Inside the frame only (the last line showed under the box, over the control below).
    child.set_clip_rect(rc.shrink(2.0));
    child.visuals_mut().text_cursor.stroke.color = LIGHT_TEXT();
    if !enabled {
        child.disable();
    }
    let out = egui::ScrollArea::vertical()
        .id_salt((id, "scroll"))
        .max_height(inner.height())
        // egui's least height (64) is taller than most boxes.
        .min_scrolled_height(inner.height())
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .scroll_source(egui::scroll_area::ScrollSource {
            scroll_bar: false,
            mouse_wheel: false,
            ..Default::default()
        })
        .vertical_scroll_offset(off)
        .show(&mut child, |ui| {
            let resp = ui.add(
                egui::TextEdit::multiline(value)
                    .font(tahoma())
                    .text_color(ink)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .desired_width(inner.width())
                    .desired_rows(3),
            );
            edit_menu(&resp, value, false, true);
            // Enter here is a new line, not the dialog's OK.
            if resp.has_focus() {
                take_key(ui, egui::Key::Enter);
            }
            resp
        });
    let resp = out.inner;
    // Where the caret took it (typing past the bottom), then the wheel and the bar.
    off = out.state.clamped_offset().y;
    let (total, page) = (out.content_size.y, inner.height());
    let line = ui.fonts_mut(|f| f.row_height(&tahoma()));
    if ui.rect_contains_pointer(rect(rc)) {
        off += wheel_rows(ui, line, (page / line).floor().max(1.0)) * line;
    }
    if total > page && enabled {
        crate::scrollbar::bar(
            ui,
            off_key.with("bar"),
            track,
            true,
            total,
            page,
            line,
            &mut off,
        );
    }
    let off_now = off.clamp(0.0, (total - page).max(0.0));
    if off_now != out.state.clamped_offset().y {
        ui.ctx().request_repaint();
    }
    ui.data_mut(|d| d.insert_temp(off_key, off_now));
    edit_focus_line(ui, &resp, rc);
    resp
}
