//! Keys, focus and the pointer: mnemonics, Enter and Escape, focus rings and when they show, keys
//! kept by a control, typing to find, Tab and arrow keys between controls, auto-repeat.

use super::*;

/// Alt+`letter` was pressed (and is taken): Latin letters as egui keys, other scripts (Cyrillic,
/// Greek, Arabic…) as the text typed with Alt held.
pub fn mnemonic_pressed(ui: &Ui, letter: char) -> bool {
    let upper: String = letter.to_uppercase().collect();
    if let Some(key) = egui::Key::from_name(&upper)
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::ALT, key))
    {
        return true;
    }
    let lower: String = letter.to_lowercase().collect();
    ui.input_mut(|i| {
        let alt = i.modifiers.alt;
        let at = i
            .events
            .iter()
            .position(|e| matches!(e, egui::Event::Text(t) if alt && t.to_lowercase() == lower));
        at.map(|k| i.events.remove(k)).is_some()
    })
}

/// Enter, without a modifier, that no control took first: a dialog's default button. A control
/// that answers Enter itself (an open list, a multi-line box) takes it with [`take_key`] before
/// the dialog asks.
pub fn enter_pressed(ui: &Ui) -> bool {
    ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none())
}

/// Escape that no control took first (an open list or picker closes itself): a dialog's Cancel.
pub fn escape_pressed(ui: &Ui) -> bool {
    ui.input(|i| i.key_pressed(egui::Key::Escape))
}

/// Take `key` (without modifiers) for a control that answers it, so the dialog under it doesn't
/// too: Escape closes an open list, not the window; Enter in a multi-line box is a new line.
pub fn take_key(ui: &Ui, key: egui::Key) -> bool {
    ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key))
}

/// A part clicked by the pointer that never takes the keyboard focus (egui's `click()` is also
/// a Tab stop): spinner arrows, a combo's arrow, scroll bars, toolbar and menu-bar items, rows
/// of a list that handles its own keys. Tab then only reaches real controls.
pub fn pointer_only() -> Sense {
    Sense::CLICK
}

/// [`pointer_only`] that can also be dragged (a scroll bar's thumb).
pub fn pointer_drag() -> Sense {
    Sense::CLICK | Sense::DRAG
}

/// The ring of `resp`'s control around `rc` while it has the keyboard focus.
pub fn show_focus(ui: &Ui, resp: &Response, rc: Rect) {
    if resp.has_focus() {
        focus_ring(ui.painter(), rc);
    }
}

/// Is the keyboard in use in the current window? A navigation key (Tab, the arrows, Space,
/// Enter, Home / End, Page Up / Down) turns it on, a mouse press off: focus rings show only
/// for a user moving with the keys, as Windows shows them.
pub fn keyboard_in_use(ctx: &egui::Context) -> bool {
    use egui::Key as K;
    let key = egui::Id::unique(("keyboard-in-use", ctx.viewport_id()));
    let (keys, press) = ctx.input(|i| {
        let nav = i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key {
                    key: K::Tab
                        | K::ArrowUp
                        | K::ArrowDown
                        | K::ArrowLeft
                        | K::ArrowRight
                        | K::Space
                        | K::Enter
                        | K::Home
                        | K::End
                        | K::PageUp
                        | K::PageDown
                        | K::F6,
                    pressed: true,
                    ..
                }
            )
        });
        (nav, i.pointer.any_pressed())
    });
    let mut on = ctx.data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    if keys {
        on = true;
    } else if press {
        on = false;
    }
    ctx.data_mut(|d| d.insert_temp(key, on));
    on
}

/// The keyboard focus around `rc`: a dotted rectangle, every other pixel, in the theme's ink.
pub fn focus_ring(p: &Painter, rc: Rect) {
    focus_ring_round(p, rc, control_radius());
}

/// [`focus_ring`] whose ring or frame (where the theme draws one) has corners of `radius`: a
/// control rounded otherwise than the others (Mac OS X's push buttons, round at the ends).
pub fn focus_ring_round(p: &Painter, rc: Rect, radius: f32) {
    // Only once the keyboard is in use in this window, as Windows shows focus rectangles.
    if !keyboard_in_use(p.ctx()) {
        return;
    }
    let ink = crate::theme::colors().focus_ring;
    // GNOME: a rounded ring, as wide as two points, the controls' corners.
    match crate::theme::current().controls.focus {
        crate::palette::FocusLook::Ring => {
            p.rect_stroke(rc, radius, Stroke::new(2.0, ink), StrokeKind::Inside);
            return;
        }
        // KDE: a frame one point wide.
        crate::palette::FocusLook::Frame => {
            p.rect_stroke(rc, radius, Stroke::new(1.0, ink), StrokeKind::Inside);
            return;
        }
        crate::palette::FocusLook::Dotted => {}
    }
    let (l, t, r, b) = (rc.left(), rc.top(), rc.right() - 1.0, rc.bottom() - 1.0);
    let mut x = l;
    while x <= r {
        px(p, x, t, ink);
        px(p, x, b, ink);
        x += 2.0;
    }
    let mut y = t;
    while y <= b {
        px(p, l, y, ink);
        px(p, r, y, ink);
        y += 2.0;
    }
}

/// A held button that repeats, as Windows' spinners and scroll-bar arrows do: true when it is
/// pressed, then after 0.4 s every 50 ms while it stays held (redraws are asked for to keep
/// time).
pub fn auto_repeat(ui: &Ui, resp: &Response) -> bool {
    let key = resp.id.with("repeat-at");
    if !resp.is_pointer_button_down_on() {
        ui.data_mut(|d| d.remove::<f64>(key));
        return false;
    }
    // Paused while the pointer is off it (Windows' arrows wait for it to come back).
    if !resp.contains_pointer() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
        return false;
    }
    let now = ui.input(|i| i.time);
    let due = ui.data(|d| d.get_temp::<f64>(key));
    let (fire, next) = match due {
        None => (true, now + 0.4),
        Some(t) if now >= t => (true, t + 0.05),
        Some(t) => (false, t),
    };
    ui.data_mut(|d| d.insert_temp(key, next));
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f64((next - now).max(0.01)));
    fire
}

/// The click of a control that opens a popup, except the one ending the press that just closed
/// that popup (it went down on the control): a click on an open list's arrow closes it, not
/// closes and opens it again.
pub fn opener_click(ui: &Ui, mut resp: Response) -> Response {
    let key = crate::chrome::dismissed_at_key(ui.ctx());
    let Some(at) = ui.data(|d| d.get_temp::<Pos2>(key)) else {
        return resp;
    };
    if resp.clicked() && resp.rect.contains(at) {
        resp.flags.remove(egui::response::Flags::CLICKED);
        ui.data_mut(|d| d.remove::<Pos2>(key));
    } else if !ui.input(|i| i.pointer.any_down() || i.pointer.any_released()) {
        // Between presses: that press is over.
        ui.data_mut(|d| d.remove::<Pos2>(key));
    }
    resp
}

/// A control greyed while it has the keys hands them to the next one, as in Windows: egui let
/// them go to nowhere, and Tab then started over from the window's first control (a button
/// that greys itself when pressed, a box greyed by a choice made from the keyboard). Called
/// where the control is laid out, so "next" is the one after it.
pub fn pass_focus_on(ui: &Ui, resp: &Response, enabled: bool) {
    let id = resp.id;
    if !enabled
        && ui
            .ctx()
            .memory(|m| m.focused().is_none() && m.had_focus_last_frame(id))
    {
        ui.ctx()
            .memory_mut(|m| m.move_focus(egui::FocusDirection::Next));
    }
}

/// A press gives a control the keys, as in Windows (egui focuses only text boxes on a click):
/// the arrows work in a radio group clicked, Ctrl+C in a field clicked. Its ring shows only
/// once the keyboard is used ([`keyboard_in_use`]).
pub fn focus_on_press(resp: &Response) {
    if resp.is_pointer_button_down_on() && !resp.has_focus() && resp.sense.senses_click() {
        resp.request_focus();
    }
}

/// egui clicks a focused control on Enter as on Space. A check box, radio button or tab answers
/// Space only, as in Windows: Enter is the dialog's default button (it ticked the box and
/// pressed OK). Links do open on Enter.
pub fn enter_is_not_a_click(ui: &Ui, mut resp: Response) -> Response {
    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        resp.flags.remove(egui::response::Flags::CLICKED);
    }
    resp
}

/// A focused control that answers arrow keys (or Escape) itself keeps them: egui otherwise also
/// moves the focus to the next control on an arrow and drops it on Escape. Call each frame
/// with the control's response.
pub fn keep_keys(ui: &Ui, resp: &Response, horizontal: bool, vertical: bool, escape: bool) {
    if resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                resp.id,
                egui::EventFilter {
                    horizontal_arrows: horizontal,
                    vertical_arrows: vertical,
                    escape,
                    ..Default::default()
                },
            )
        });
    }
}

/// Type-ahead, as Windows lists and drop-downs have it: letters typed while it has the keys
/// pick the next of its `n` items whose `label` starts with them (typed within a second of
/// each other; one letter again and again goes through the items starting with it). `key`
/// keeps what was typed. Returns the item to choose.
pub fn typed_match(
    ui: &Ui,
    key: egui::Id,
    n: usize,
    current: Option<usize>,
    label: impl Fn(usize) -> String,
) -> Option<usize> {
    let typed: String = ui.input(|i| {
        if i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command {
            return String::new();
        }
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<String>()
    });
    let typed: String = typed.chars().filter(|c| !c.is_control()).collect();
    // Space alone is a key of its own (it ticks a row), not a name's first letter.
    if typed.trim().is_empty() || n == 0 {
        return None;
    }
    let now = ui.input(|i| i.time);
    let (mut buf, at) = ui
        .data(|d| d.get_temp::<(String, f64)>(key))
        .unwrap_or_default();
    if now - at > 1.0 {
        buf.clear();
    }
    buf.push_str(&typed.to_lowercase());
    ui.data_mut(|d| d.insert_temp(key, (buf.clone(), now)));
    let first = buf.chars().next().unwrap_or_default();
    let repeated = buf.chars().all(|c| c == first);
    // The same letter again: the next item with it; else the first from here with the prefix.
    let (needle, start) = if repeated {
        (first.to_string(), current.map_or(0, |c| c + 1))
    } else {
        (buf, current.unwrap_or(0))
    };
    (0..n)
        .map(|k| (start + k) % n)
        .find(|&i| label(i).to_lowercase().starts_with(&needle))
}

/// A tab control's keys, as Windows has them: Ctrl+Tab / Ctrl+Page Down the next of its `n`
/// tabs, Ctrl+Shift+Tab / Ctrl+Page Up the previous one, wrapping. For a window's main tabs
/// (one tab control a window answers them).
pub fn tab_keys(ui: &Ui, n: usize, current: usize) -> Option<usize> {
    if n == 0 {
        return None;
    }
    let (next, prev) = ui.input_mut(|i| {
        let ctrl = egui::Modifiers::COMMAND;
        let ctrl_shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        (
            i.consume_key(ctrl, egui::Key::Tab) || i.consume_key(ctrl, egui::Key::PageDown),
            i.consume_key(ctrl_shift, egui::Key::Tab) || i.consume_key(ctrl, egui::Key::PageUp),
        )
    });
    match (next, prev) {
        (true, _) => Some((current + 1) % n),
        (_, true) => Some((current + n - 1) % n),
        _ => None,
    }
}

/// A radio group's keys, as Windows has them: while one of the group's buttons (`group`, in
/// order) has the focus, Up / Left and Down / Right move the focus to the previous / next
/// enabled one, wrapping, and choose it. Returns the one chosen.
pub fn radio_keys(ui: &Ui, group: &[Response]) -> Option<usize> {
    let at = group.iter().position(Response::has_focus)?;
    keep_keys(ui, &group[at], true, true, false);
    let step: isize = if take_key(ui, egui::Key::ArrowDown) || take_key(ui, egui::Key::ArrowRight) {
        1
    } else if take_key(ui, egui::Key::ArrowUp) || take_key(ui, egui::Key::ArrowLeft) {
        -1
    } else {
        return None;
    };
    let n = group.len() as isize;
    let next = (1..n)
        .map(|k| (at as isize + step * k).rem_euclid(n) as usize)
        .find(|&j| group[j].sense.senses_click())?;
    group[next].request_focus();
    Some(next)
}
