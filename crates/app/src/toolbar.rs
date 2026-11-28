//! View → Toolbar → Customize: which of the toolbar's buttons show, and in what order. The
//! buttons themselves (their icons, labels and ids) are listed here; chrome.rs draws them, in
//! the look View → Toolbar chooses (`chrome::ToolbarLook`).

use egui::{Pos2, pos2, vec2};

use crate::dialogs::{Outcome, window};
use crate::icons::Icon;
use crate::winui::{self, ButtonKind, LIGHT_BG, LIGHT_TEXT, r};

/// Every toolbar button in its first order: its icon, its label, and the id the settings keep it
/// by (`Settings::toolbar_buttons`; never changed, a saved list names them).
pub const BUTTONS: [(Icon, &str, &str); 12] = [
    (Icon::AddUrl, "Add URL", "add-url"),
    (Icon::AddTorrent, "Add Torrent", "add-torrent"),
    // i18n: verb: continue the selected downloads
    (Icon::Resume, "Resume", "resume"),
    // i18n: verb: stop the selected downloads
    (Icon::Stop, "Stop", "stop"),
    (Icon::StopAll, "Stop All", "stop-all"),
    // i18n: verb: delete the selected downloads
    (Icon::Delete, "Delete", "delete"),
    (
        Icon::DeleteCompleted,
        "Delete Completed",
        "delete-completed",
    ),
    // i18n: plural noun: opens the Options window (settings)
    (Icon::Options, "Options", "options"),
    // i18n: noun: the tool that starts and stops queues on time
    (Icon::Scheduler, "Scheduler", "scheduler"),
    (Icon::StartQueue, "Start Queue", "start-queue"),
    (Icon::StopQueue, "Stop Queue", "stop-queue"),
    // i18n: noun: opens the site grabber (Tasks → Run site grabber), which explores a web site and downloads its files
    (Icon::Grabber, "Grabber", "grabber"),
];

/// A button's label in English (translated where it is drawn, as `tc("toolbar", …)`).
pub fn label(icon: Icon) -> &'static str {
    BUTTONS
        .iter()
        .find(|(i, ..)| *i == icon)
        .map_or("", |(_, label, _)| *label)
}

/// The buttons as the user arranged them (`saved`, by id): those it names in its order (each
/// once; an id DDM doesn't know is left out), then any it doesn't name, shown, in their first
/// order (a button added since, or none arranged: every button).
pub fn arrange(saved: &[(String, bool)]) -> Vec<(Icon, bool)> {
    let mut out: Vec<(Icon, bool)> = Vec::with_capacity(BUTTONS.len());
    for (id, shown) in saved {
        if let Some((icon, ..)) = BUTTONS.iter().find(|(.., i)| i == id)
            && !out.iter().any(|(o, _)| o == icon)
        {
            out.push((*icon, *shown));
        }
    }
    for (icon, ..) in BUTTONS {
        if !out.iter().any(|(o, _)| *o == icon) {
            out.push((icon, true));
        }
    }
    out
}

/// The buttons the toolbar shows, in their order.
pub fn shown(saved: &[(String, bool)]) -> Vec<Icon> {
    arrange(saved)
        .into_iter()
        .filter_map(|(icon, on)| on.then_some(icon))
        .collect()
}

/// `list` as the settings keep it: empty when it is the first order with every button shown (so
/// a button added later joins it), else every button by id.
pub fn to_saved(list: &[(Icon, bool)]) -> Vec<(String, bool)> {
    if list == arrange(&[]).as_slice() {
        return Vec::new();
    }
    list.iter()
        .filter_map(|(icon, on)| {
            BUTTONS
                .iter()
                .find(|(i, ..)| i == icon)
                .map(|(.., id)| ((*id).to_owned(), *on))
        })
        .collect()
}

/// Moves row `i` of `list` one place up (or down); its new place, or None where it can't move.
pub fn move_row(list: &mut [(Icon, bool)], i: usize, up: bool) -> Option<usize> {
    let to = if up { i.checked_sub(1)? } else { i + 1 };
    if i >= list.len() || to >= list.len() {
        return None;
    }
    list.swap(i, to);
    Some(to)
}

/// Rows of the list, in points.
const ROW_H: f32 = 20.0;
/// The list's height: a row for every button, none hidden behind a scroll bar.
const LIST_H: f32 = ROW_H * BUTTONS.len() as f32 + 4.0;
/// OK and Cancel's top, 10 under the list (the window is as tall as it takes: a button added
/// to [`BUTTONS`] makes it taller).
const OK_Y: f32 = 30.0 + LIST_H + 10.0;

/// View → Toolbar → Customize...: each button ticked to show, in an order Move up and Move down
/// change; Reset puts back every button in its first order. OK gives the list as the settings
/// keep it ([`to_saved`]).
pub struct Customize {
    list: Vec<(Icon, bool)>,
    rows: winui::RowList,
}

impl Customize {
    pub fn new(saved: &[(String, bool)]) -> Self {
        let mut rows = winui::RowList::default();
        rows.selected = Some(0);
        Customize {
            list: arrange(saved),
            rows,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<Vec<(String, bool)>> {
        window(
            ctx,
            "toolbar-customize",
            // i18n: title of the window that chooses the toolbar's buttons and their order (View → Toolbar → Customize...)
            "Customize toolbar",
            [380.0, OK_Y + 32.0],
            false,
            LIGHT_BG(),
            |ui, o| self.body(ui, o),
        )
    }

    fn body(&mut self, ui: &mut egui::Ui, o: Pos2) -> Outcome<Vec<(String, bool)>> {
        let p = ui.painter().clone();
        winui::fit(
            ui,
            &p,
            o.x + 11.0,
            o.y + 20.0,
            // i18n: in Customize toolbar, above the list of buttons (ticked: shown on the toolbar)
            crate::i18n::t("Buttons on the toolbar, in their order:"),
            LIGHT_TEXT(),
            o.x + 369.0,
            "toolbar-caption",
        );
        // i18n: what a screen reader calls Customize toolbar's list of buttons
        self.rows.name = crate::i18n::t("Toolbar buttons");
        let n = self.list.len();
        let list = &self.list;
        let mut toggle = None;
        let ev = winui::row_list(
            ui,
            "toolbar-buttons",
            r(o, 11.0, 30.0, 230.0, LIST_H),
            n,
            ROW_H,
            &mut self.rows,
            |ui, clip, i, row, look| {
                let (icon, mut on) = list[i];
                let name = crate::i18n::tc("toolbar", label(icon));
                look.name = name.to_owned();
                let glyph = pos2(row.left() + 5.0, row.top() + 3.0);
                if winui::row_check(ui, clip, glyph, &mut on, name, ("toolbar-check", i)) {
                    toggle = Some(i);
                }
                // The icon as the toolbar's small buttons draw it (the theme's own, if any).
                let side = 16.0;
                let at = egui::Rect::from_min_size(
                    pos2(glyph.x + 19.0, row.top() + 2.0),
                    vec2(side, side),
                );
                icon.image_px(true, side * ui.ctx().pixels_per_point())
                    .paint_at(ui, winui::rect(at));
                winui::fit(
                    ui,
                    clip,
                    at.right() + 6.0,
                    row.top() + 14.0,
                    name,
                    look.ink,
                    row.right() - 4.0,
                    ("toolbar-name", i),
                );
            },
        );
        if let Some(i) = ev.space.or(ev.double_clicked) {
            toggle = Some(i);
        }
        if let Some(i) = toggle {
            self.list[i].1 = !self.list[i].1;
        }
        let sel = self.rows.selected.filter(|s| *s < n);
        // Move up, Move down and Reset beside the list.
        let up = winui::button_light(
            ui,
            r(o, 252.0, 30.0, 117.0, 23.0),
            // i18n: button in Customize toolbar: the selected button one place earlier on the toolbar
            "Move up",
            if sel.is_some_and(|s| s > 0) {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            },
            false,
        )
        .clicked();
        let down = winui::button_light(
            ui,
            r(o, 252.0, 59.0, 117.0, 23.0),
            // i18n: button in Customize toolbar: the selected button one place later on the toolbar
            "Move down",
            if sel.is_some_and(|s| s + 1 < n) {
                ButtonKind::Normal
            } else {
                ButtonKind::Disabled
            },
            false,
        )
        .clicked();
        let reset = winui::button_light(
            ui,
            r(o, 252.0, 98.0, 117.0, 23.0),
            // i18n: button in Customize toolbar: every button shown again, in its first order
            "Reset",
            ButtonKind::Normal,
            false,
        )
        .clicked();
        if let Some(s) = sel
            && (up || down)
            && let Some(to) = move_row(&mut self.list, s, up)
        {
            self.rows.selected = Some(to);
        }
        if reset {
            self.list = arrange(&[]);
        }
        // At least one button: an empty toolbar would be a blank band.
        let any = self.list.iter().any(|(_, on)| *on);
        let ok = winui::button_light(
            ui,
            r(o, 215.0, OK_Y, 73.0, 21.0),
            "OK",
            if any {
                ButtonKind::Default
            } else {
                ButtonKind::Disabled
            },
            true,
        );
        let ok = winui::why_disabled(
            ok,
            !any,
            // i18n: tooltip of Customize toolbar's greyed OK button
            "Tick at least one button to show",
        )
        .clicked();
        let cancel = winui::button_light(
            ui,
            r(o, 296.0, OK_Y, 73.0, 21.0),
            "Cancel",
            ButtonKind::Normal,
            false,
        )
        .clicked();
        if cancel || winui::escape_pressed(ui) {
            Outcome::Closed
        } else if any && (ok || winui::enter_pressed(ui)) {
            Outcome::Done(to_saved(&self.list))
        } else {
            Outcome::Open
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(ids: &[(&str, bool)]) -> Vec<(String, bool)> {
        ids.iter().map(|(i, on)| ((*i).to_owned(), *on)).collect()
    }

    /// Nothing arranged: every button, in the toolbar's first order (as before Customize).
    #[test]
    fn nothing_saved_is_every_button_in_its_first_order() {
        let all = arrange(&[]);
        assert_eq!(all.len(), BUTTONS.len());
        assert!(all.iter().all(|(_, on)| *on));
        let icons: Vec<Icon> = all.iter().map(|(i, _)| *i).collect();
        let first: Vec<Icon> = BUTTONS.iter().map(|(i, ..)| *i).collect();
        assert_eq!(icons, first);
        assert_eq!(shown(&[]), first);
        // And it is kept as nothing, so a button added later joins it.
        assert!(to_saved(&all).is_empty());
    }

    /// A saved list keeps its order and what it hides; unknown ids and repeats are left out; a
    /// button it doesn't name (added since) comes after it, shown.
    #[test]
    fn a_saved_order_is_kept_and_new_buttons_join_it() {
        let list = arrange(&saved(&[
            ("options", true),
            ("add-url", false),
            ("no-such-button", true),
            ("options", false),
            ("stop", true),
        ]));
        assert_eq!(list.len(), BUTTONS.len());
        assert_eq!(
            &list[..3],
            &[
                (Icon::Options, true),
                (Icon::AddUrl, false),
                (Icon::Stop, true)
            ]
        );
        // The rest in their first order, shown.
        assert_eq!(list[3], (Icon::AddTorrent, true));
        assert!(list[3..].iter().all(|(_, on)| *on));
        let shown = shown(&to_saved(&list));
        assert_eq!(shown[..2], [Icon::Options, Icon::Stop]);
        assert!(!shown.contains(&Icon::AddUrl));
        // Saved and read back: the same list.
        assert_eq!(arrange(&to_saved(&list)), list);
    }

    /// Grabber's place: after Stop Queue in the first order; at the end of a list saved before
    /// it was a button (DDM 0.6.0's eleven, arranged), shown, the rest as they were saved.
    #[test]
    fn grabber_joins_a_list_saved_before_it_at_the_end_shown() {
        let first = arrange(&[]);
        let at = first.iter().position(|(i, _)| *i == Icon::Grabber).unwrap();
        assert_eq!(first[at - 1].0, Icon::StopQueue);
        assert_eq!(label(Icon::Grabber), "Grabber");
        let old = saved(&[
            ("stop-queue", true),
            ("start-queue", true),
            ("add-url", true),
            ("add-torrent", false),
            ("resume", true),
            ("stop", true),
            ("stop-all", false),
            ("delete", true),
            ("delete-completed", false),
            ("options", true),
            ("scheduler", true),
        ]);
        assert!(!old.iter().any(|(id, _)| id == "grabber"));
        let list = arrange(&old);
        assert_eq!(list.len(), BUTTONS.len());
        assert_eq!(list.last(), Some(&(Icon::Grabber, true)));
        let kept: Vec<(String, bool)> = to_saved(&list[..old.len()]);
        assert_eq!(kept, old);
        assert_eq!(shown(&old).last(), Some(&Icon::Grabber));
        assert!(!shown(&old).contains(&Icon::AddTorrent));
        // Saved again, the list names it, so it stays where it is now.
        let again = to_saved(&list);
        assert_eq!(again.last(), Some(&("grabber".to_owned(), true)));
        assert_eq!(arrange(&again), list);
        // Hidden by the user, it stays hidden.
        let mut hidden = again.clone();
        hidden.last_mut().unwrap().1 = false;
        assert!(!shown(&hidden).contains(&Icon::Grabber));
    }

    #[test]
    fn rows_move_one_place_within_the_list() {
        let mut list = arrange(&[]);
        assert_eq!(move_row(&mut list, 0, true), None);
        let last = list.len() - 1;
        assert_eq!(move_row(&mut list, last, false), None);
        assert_eq!(move_row(&mut list, 99, true), None);
        assert_eq!(move_row(&mut list, 2, true), Some(1));
        assert_eq!(list[1].0, Icon::Resume);
        assert_eq!(list[2].0, Icon::AddTorrent);
        assert_eq!(move_row(&mut list, 1, false), Some(2));
        assert_eq!(list, arrange(&[]));
    }

    /// Every button has its own id and label (the settings and the dialog tell them apart).
    #[test]
    fn ids_and_labels_are_unique() {
        for (k, (icon, l, id)) in BUTTONS.iter().enumerate() {
            assert!(!l.is_empty() && !id.is_empty());
            assert_eq!(label(*icon), *l);
            for (other, ol, oid) in &BUTTONS[k + 1..] {
                assert!(icon != other && l != ol && id != oid);
            }
        }
    }
}
