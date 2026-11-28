//! Screen readers (AccessKit): what each hand-painted control is (role, name, state), and the
//! choices and values an assistive technology sets.

use super::*;

/// What a control is for screen readers (Narrator, NVDA through AccessKit): its role, its name
/// (the full translated label, "&" taken out), whether it can be used, and for a check box,
/// radio button, tab or list row whether it is on. Our controls are painted by hand, so egui
/// can't tell otherwise. Call right after the control's `interact`, before any tooltip.
/// A line of text a window paints (a status, an error) made readable too: a screen reader and UI
/// Automation find it as static text at `rc`, by `salt`, with `text` as it reads (no mnemonic).
pub fn readable(ui: &Ui, rc: Rect, salt: impl std::hash::Hash + std::fmt::Debug, text: &str) {
    let resp = ui.interact(rc, widget_id(ui, ("readable", salt)), Sense::hover());
    let text = text.to_owned();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &text));
}

pub fn expose(resp: &Response, role: egui::Role, label: &str, enabled: bool, on: Option<bool>) {
    let name = label
        .replace("&&", "\u{1}")
        .replace('&', "")
        .replace('\u{1}', "&");
    resp.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(role, enabled, &name);
        info.selected = on;
        info
    });
}

/// A drop-down list for screen readers and UI Automation, whose list (the shared popup) they
/// can't reach: Set value picks the item of that text (as shown, or as written), Expand and
/// Collapse open and close the list. Returns the item picked this frame.
pub fn assistive_choice(resp: &Response, items: &[&str], open: &mut bool) -> Option<usize> {
    use egui::accesskit::{Action, ActionData};
    let (value, expand, collapse) = resp.ctx.input(|i| {
        let value = i
            .accesskit_action_requests(resp.id, Action::SetValue)
            .find_map(|r| match &r.data {
                Some(ActionData::Value(v)) => Some(v.to_string()),
                _ => None,
            });
        (
            value,
            i.has_accesskit_action_request(resp.id, Action::Expand),
            i.has_accesskit_action_request(resp.id, Action::Collapse),
        )
    });
    if expand {
        *open = true;
    }
    if collapse {
        *open = false;
    }
    let expanded = *open;
    resp.ctx.accesskit_node_builder(resp.id, |n| {
        n.add_action(Action::SetValue);
        n.add_action(if expanded {
            Action::Collapse
        } else {
            Action::Expand
        });
        n.set_expanded(expanded);
    });
    let value = value?;
    let value = value.trim();
    items
        .iter()
        .position(|s| crate::i18n::t(s) == value || *s == value)
}

/// Whether a screen reader or UI Automation set this control's value this frame (AccessKit's
/// Set value), focused or not: such a value is meant at once, where a typed one waits for Enter
/// or the focus to leave.
pub fn value_set_by_assistive(resp: &Response) -> bool {
    resp.ctx
        .input(|i| i.has_accesskit_action_request(resp.id, egui::accesskit::Action::SetValue))
}

/// [`expose`] for a control showing a value (a drop-down): named by its caption, the value read
/// after it.
pub fn expose_value(resp: &Response, role: egui::Role, value: &str, enabled: bool) {
    let name = caption_for(&resp.ctx, resp.rect).unwrap_or_default();
    let value = value.to_owned();
    resp.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(role, enabled, &name);
        info.current_text_value = Some(value.clone());
        info
    });
}
