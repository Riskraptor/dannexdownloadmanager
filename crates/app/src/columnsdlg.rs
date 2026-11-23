//! View → Choose columns: the download list's columns in one window, a check box for each
//! (shown or not) in their order, Move up / Move down to reorder them and Reset to put them back
//! as they first were. It edits the column state the header's own ways do (dragging a title,
//! its right-click column chooser, Reset columns: list.rs), applied on OK and saved with the
//! list's layout as those are. File Name always shows.

use egui::{Pos2, Rect, pos2, vec2};

use crate::dialogs::{Outcome, grow_extra, grow_to, window};
use crate::list::{self, COLS, ListState};
use crate::winui::{self, ButtonKind, LIGHT_BG, LIGHT_TEXT, r};

/// The columns as the window shows them: every data slot in display order, each shown or not.
#[derive(Clone, Debug, PartialEq)]
pub struct Columns {
    /// (data slot, shown), left to right in the list.
    rows: Vec<(usize, bool)>,
    /// Reset was pressed: the widths go back to theirs too (as the header's Reset columns).
    reset_widths: bool,
    /// The Torrents category's view is shown: its torrent columns show by default.
    torrent_view: bool,
}

impl Columns {
    /// The list's columns as they are now.
    pub fn of(st: &ListState) -> Self {
        Columns {
            rows: st.order.iter().map(|c| (*c, st.is_shown(*c))).collect(),
            reset_widths: false,
            torrent_view: st.torrent_view,
        }
    }

    /// How many columns (rows of the window's list).
    pub fn count(&self) -> usize {
        self.rows.len()
    }

    /// Row `i`'s data slot and whether it shows.
    pub fn get(&self, i: usize) -> (usize, bool) {
        self.rows[i]
    }

    /// Whether row `i`'s check box can change: File Name's can't (it always shows).
    pub fn can_hide(&self, i: usize) -> bool {
        self.rows.get(i).is_some_and(|(c, _)| *c != 0)
    }

    /// Show row `i`'s column or hide it (not File Name).
    pub fn toggle(&mut self, i: usize) {
        if self.can_hide(i) {
            self.rows[i].1 = !self.rows[i].1;
        }
    }

    /// Show or hide row `i`'s column (File Name stays shown).
    pub fn set_shown(&mut self, i: usize, on: bool) {
        if self.can_hide(i) {
            self.rows[i].1 = on;
        }
    }

    /// Move row `i` one place towards the list's start (`up`) or its end; where it is now.
    pub fn move_row(&mut self, i: usize, up: bool) -> usize {
        let to = if up {
            i.checked_sub(1)
        } else {
            Some(i + 1).filter(|t| *t < self.rows.len())
        };
        match to {
            Some(to) if i < self.rows.len() => {
                self.rows.swap(i, to);
                to
            }
            _ => i,
        }
    }

    /// The columns as they first were: their order, the default ones shown (the torrent columns
    /// following the view again), and, on OK, their widths.
    pub fn reset(&mut self) {
        let mut fresh = ListState::default();
        fresh.torrent_view = self.torrent_view;
        self.rows = list::DEFAULT_ORDER
            .iter()
            .map(|c| (*c, fresh.is_shown(*c)))
            .collect();
        self.reset_widths = true;
    }

    /// Into the list: its order and which columns show (its widths too after Reset, and the torrent
    /// columns following the view again unless one was changed after it).
    pub fn apply(&self, st: &mut ListState) {
        if self.reset_widths {
            st.reset_columns();
        }
        let order: Vec<usize> = self.rows.iter().map(|(c, _)| *c).collect();
        let mut visible = [true; COLS];
        for (c, on) in &self.rows {
            if *c < COLS {
                visible[*c] = *on;
            }
        }
        st.set_columns(&order, visible, self.reset_widths);
    }
}

/// A column's name as the header writes it (its own words), and the narrow Q by its whole name,
/// as the header's column chooser names it.
fn column_name(col: usize) -> &'static str {
    if list::TITLES[col] == "Q" {
        crate::i18n::tc("column", "Queue")
    } else {
        crate::i18n::tc("column", list::TITLES[col])
    }
}

/// The window's id (`dialogs::dialog_viewport`), its width, and the list's rows (every column
/// shows without scrolling).
pub const ID: &str = "columns";
const W: f32 = 380.0;
const ROW_H: f32 = 17.0;
/// The list's box: its rows, a pixel of frame and of padding above and below them.
const LIST_H: f32 = COLS as f32 * ROW_H + 5.0;

pub struct CustomizeColumns {
    cols: Columns,
    rows: winui::RowList,
}

impl CustomizeColumns {
    /// Opened on the list's columns as they are, the first one selected.
    pub fn new(st: &ListState) -> Self {
        let mut rows = winui::RowList::default();
        rows.selected = Some(0);
        CustomizeColumns {
            cols: Columns::of(st),
            rows,
        }
    }

    /// `Done` with the columns on OK (the caller applies them); Cancel changes nothing.
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<Columns> {
        // A translation's longer sentence takes more lines: what is under it moves down.
        let extra = grow_extra(ctx, ID);
        let list_bottom = 31.0 + extra + LIST_H;
        let height = list_bottom + 13.0 + 21.0 + 11.0;
        let CustomizeColumns { cols, rows } = self;
        window(
            ctx,
            ID,
            // i18n: window title (View → Choose columns): which columns the download list shows, and in what order
            "Choose columns",
            [W, height],
            false,
            LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                // i18n: Choose columns: the line above the download list's columns, each with a check box
                let intro = crate::i18n::t("Tick the columns to show, and put them in order:");
                let lines = winui::paragraph(
                    &p,
                    o.x + 11.0,
                    o.y + 21.0,
                    intro,
                    LIGHT_TEXT(),
                    o.x + W - 11.0,
                    13.0,
                );
                grow_to(ui.ctx(), ID, lines, 1);
                let top = 31.0 + extra;
                // The buttons beside the list, as wide as their longest label in this language.
                let side_w = ["Move up", "Move down", "Reset"]
                    .iter()
                    .map(|l| winui::button_width(&p, l, 80.0))
                    .fold(0.0, f32::max)
                    .min(140.0);
                let side_x = W - 11.0 - side_w;
                let list_w = side_x - 8.0 - 11.0;
                // i18n: Choose columns: what a screen reader calls its list of the download list's columns
                rows.name = crate::i18n::t("Columns");
                let ev = winui::row_list(
                    ui,
                    "columns",
                    r(o, 11.0, top, list_w, LIST_H),
                    cols.count(),
                    ROW_H,
                    rows,
                    |ui, clip, i, row, look| {
                        let (col, mut on) = cols.get(i);
                        let name = column_name(col);
                        look.name = name.to_owned();
                        let at = pos2(row.left() + 5.0, row.top() + 2.0);
                        if cols.can_hide(i) {
                            if winui::row_check(ui, clip, at, &mut on, name, ("column", col)) {
                                cols.set_shown(i, on);
                            }
                        } else {
                            file_name_check(ui, clip, at, name);
                        }
                        // Its name in the row's ink (white on the selection).
                        winui::text(clip, row.left() + 23.0, row.top() + 12.0, name, look.ink);
                    },
                );
                if let Some(i) = ev.space {
                    cols.toggle(i);
                }
                if ev.focused {
                    rows.type_to(ui, "columns", cols.count(), |i| {
                        column_name(cols.get(i).0).to_owned()
                    });
                }
                let sel = rows.selected.filter(|s| *s < cols.count());
                let kind = |on: bool| {
                    if on {
                        ButtonKind::Normal
                    } else {
                        ButtonKind::Disabled
                    }
                };
                let up = winui::button_light(
                    ui,
                    r(o, side_x, top, side_w, 21.0),
                    // i18n: button: moves the selected column one place up the list (towards the list's start)
                    "Move up",
                    kind(sel.is_some_and(|s| s > 0)),
                    false,
                )
                .clicked();
                let down = winui::button_light(
                    ui,
                    r(o, side_x, top + 29.0, side_w, 21.0),
                    // i18n: button: moves the selected column one place down the list (towards the list's end)
                    "Move down",
                    kind(sel.is_some_and(|s| s + 1 < cols.count())),
                    false,
                )
                .clicked();
                let reset = winui::button_light(
                    ui,
                    r(o, side_x, list_bottom - 21.0, side_w, 21.0),
                    // i18n: button: puts the columns back as they first were (their order, all shown, their widths)
                    "Reset",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                if let Some(s) = sel
                    && (up || down)
                {
                    rows.selected = Some(cols.move_row(s, up));
                }
                if reset {
                    cols.reset();
                }
                let buttons = winui::buttons_light_right(
                    ui,
                    o,
                    W - 11.0,
                    list_bottom + 13.0,
                    &[("OK", ButtonKind::Default), ("Cancel", ButtonKind::Normal)],
                );
                let (ok, cancel) = (buttons[0].clicked(), buttons[1].clicked());
                if cancel || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if ok || winui::enter_pressed(ui) {
                    Outcome::Done(cols.clone())
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

/// File Name's check box: ticked and greyed (the column always shows), saying so on hover.
fn file_name_check(ui: &egui::Ui, p: &egui::Painter, at: Pos2, name: &str) {
    let rc = winui::rect(Rect::from_min_size(at, vec2(13.0, 13.0)));
    let resp = ui.interact(
        rc.expand(2.0),
        egui::Id::unique(("columns-file-name-check", ui.ctx().viewport_id())),
        egui::Sense::hover(),
    );
    winui::expose(&resp, egui::Role::CheckBox, name, false, Some(true));
    winui::check_box_glyph(p, rc, true, winui::State::Disabled);
    // i18n: tooltip of File Name's greyed check box in Choose columns: that column can't be hidden
    resp.on_hover_text(crate::i18n::t("This column is always shown"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(c: &Columns) -> Vec<usize> {
        c.rows.iter().map(|(c, _)| *c).collect()
    }

    #[test]
    fn opens_on_the_lists_columns_in_their_order() {
        let mut st = ListState::default();
        st.set_visible(2, false);
        let c = Columns::of(&st);
        assert_eq!(order(&c), list::DEFAULT_ORDER.to_vec());
        assert_eq!(c.count(), COLS);
        let size = order(&c).iter().position(|s| *s == 2).unwrap();
        assert_eq!(c.get(size), (2, false));
        // Each as it shows now: IDM's set, Added and Source off, the torrent ones out of the
        // Torrents view.
        assert!(c.rows.iter().all(|(s, on)| *on == st.is_shown(*s)));
        assert!(!c.rows.iter().any(|(s, on)| *s == list::PEERS && *on));
    }

    #[test]
    fn moving_up_and_down_stops_at_the_ends() {
        let mut c = Columns::of(&ListState::default());
        let first = order(&c);
        // The first row can't go up, the last can't go down.
        assert_eq!(c.move_row(0, true), 0);
        assert_eq!(c.move_row(COLS - 1, false), COLS - 1);
        assert_eq!(order(&c), first, "nothing moved");
        // File Name (second by default) to the top, then down two places.
        assert_eq!(c.move_row(1, true), 0);
        assert_eq!(order(&c)[..3], [0, list::SOURCE, 1]);
        assert_eq!(c.move_row(0, false), 1);
        assert_eq!(c.move_row(1, false), 2);
        assert_eq!(order(&c)[..3], [list::SOURCE, 1, 0]);
        // A row past the end stays where it is.
        assert_eq!(c.move_row(COLS + 3, true), COLS + 3);
        assert_eq!(order(&c).len(), COLS);
    }

    #[test]
    fn file_name_always_shows() {
        let mut c = Columns::of(&ListState::default());
        let at = order(&c).iter().position(|s| *s == 0).unwrap();
        assert!(!c.can_hide(at));
        c.toggle(at);
        c.set_shown(at, false);
        assert_eq!(c.get(at), (0, true));
        // Another column hides and shows again.
        let q = order(&c).iter().position(|s| *s == 1).unwrap();
        c.toggle(q);
        assert_eq!(c.get(q), (1, false));
        c.toggle(q);
        assert_eq!(c.get(q), (1, true));
    }

    #[test]
    fn ok_applies_order_and_visibility_and_keeps_the_widths() {
        let mut st = ListState::default();
        st.widths[0] = 300.0;
        let mut c = Columns::of(&st);
        // Description to the very start, hidden; Size hidden.
        let mut at = order(&c).iter().position(|s| *s == 7).unwrap();
        while at > 0 {
            at = c.move_row(at, true);
        }
        c.toggle(0);
        let size = order(&c).iter().position(|s| *s == 2).unwrap();
        c.toggle(size);
        c.apply(&mut st);
        assert_eq!(st.order[0], 7);
        assert_eq!(st.order.len(), COLS);
        assert!(!st.visible[7] && !st.visible[2]);
        assert!(st.visible[0]);
        // Untouched in the window: as they were (Source off, the torrent columns following
        // the view).
        assert!(!st.is_shown(list::SOURCE));
        assert!(st.auto[list::PEERS] && st.auto[list::RATIO] && st.auto[list::UPLOAD]);
        assert_eq!(st.widths[0], 300.0, "the widths are the header's business");
    }

    #[test]
    fn reset_puts_back_the_first_columns_and_widths() {
        let mut st = ListState::default();
        st.order.swap(0, 5);
        st.visible[3] = false;
        st.widths[3] = 400.0;
        st.hscroll = 50.0;
        let mut c = Columns::of(&st);
        c.reset();
        assert_eq!(order(&c), list::DEFAULT_ORDER.to_vec());
        let fresh = ListState::default();
        assert!(c.rows.iter().all(|(s, on)| *on == fresh.is_shown(*s)));
        // Nothing changes before OK.
        assert_eq!(st.widths[3], 400.0);
        c.apply(&mut st);
        assert_eq!(st.order, fresh.order);
        assert_eq!(st.visible, fresh.visible);
        assert_eq!(st.auto, fresh.auto);
        assert_eq!(st.widths, fresh.widths);
        assert_eq!(st.hscroll, 0.0);
    }

    #[test]
    fn a_broken_order_changes_nothing() {
        let mut st = ListState::default();
        let before = st.order.clone();
        let mut visible = [true; COLS];
        visible[0] = false;
        // A column twice (another missing): ignored.
        let mut twice = before.clone();
        twice[1] = twice[0];
        st.set_columns(&twice, visible, true);
        assert_eq!(st.order, before);
        // A whole order applies, File Name shown whatever it is told.
        let mut turned = before.clone();
        turned.reverse();
        st.set_columns(&turned, visible, false);
        assert_eq!(st.order, turned);
        assert!(st.visible[0]);
    }
}
