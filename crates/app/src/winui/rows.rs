//! Row lists: a list of rows that scrolls, is chosen from by keys and pointer and read by screen
//! readers, its rows' check boxes and the tree's expanders.

use super::*;

// ---- row list -------------------------------------------------------------------------------

/// Where a [`row_list`] is: its first row shown and its selected row (kept by the caller).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowList {
    pub first: usize,
    pub selected: Option<usize>,
    /// What a screen reader calls the list ("Quality", "Files"), translated: set by the window
    /// that owns it. Empty: unnamed (a caption near the list is often a column title).
    pub name: &'static str,
    /// The selection was set from outside (typing, the caller): scrolled into view next draw.
    pub(super) pending: bool,
    /// How far past the top of row `first` the view is (px), while the wheel has left it
    /// between rows; 0 once the keys, the caller or the scroll bar move it (by whole rows).
    pub(super) px: f32,
    /// The wheel's glide ([`WheelGlide`]).
    pub(super) glide: WheelGlide,
}

impl RowList {
    /// Typing selects the row whose `label` starts with what was typed ([`typed_match`]);
    /// call after [`row_list`] while it has the keys (`RowEvents::focused`).
    pub fn type_to(
        &mut self,
        ui: &Ui,
        list: &str,
        n: usize,
        label: impl Fn(usize) -> String,
    ) -> bool {
        let key = widget_id(ui, ("row-list-typed", list));
        match typed_match(ui, key, n, self.selected, label) {
            Some(i) => {
                self.selected = Some(i);
                self.pending = true;
                true
            }
            None => false,
        }
    }

    /// Scroll so row `i` shows (whole), `page` rows being visible.
    pub fn show_row(&mut self, i: usize, page: usize) {
        if i < self.first || (i == self.first && self.px > 0.0) {
            self.first = i;
            self.px = 0.0;
        } else if i >= self.first + page {
            self.first = i + 1 - page;
            self.px = 0.0;
        }
    }
}

/// What happened in a [`row_list`] this frame (row indexes).
#[derive(Default, Debug)]
pub struct RowEvents {
    /// The selection moved to this row (a click, the keys).
    pub picked: Option<usize>,
    pub double_clicked: Option<usize>,
    /// Enter on the selected row (not taken: [`take_key`] it to keep it from the dialog).
    pub entered: Option<usize>,
    /// Space on the selected row (a list of check boxes ticks it).
    pub space: Option<usize>,
    /// Delete on the selected row.
    pub delete: Option<usize>,
    /// Right-click on a row (selected first), at this screen point.
    pub context: Option<(usize, Pos2)>,
    /// The list has the keys (a tree adds Left / Right).
    pub focused: bool,
}

/// How [`row_list`] drew a row's background: the row's text goes in `ink`.
pub struct RowLook {
    pub selected: bool,
    pub ink: Color32,
    /// What a screen reader says for the row: the row's painter sets it (its text, e.g. a
    /// file's name and size). Left empty, the row is read without a name.
    pub name: String,
}

/// How many rows of a list a screen reader reaches at most ([`reachable_rows`]).
pub const A11Y_ROWS: usize = 300;

/// Rows of a virtual list a screen reader reaches (#111): every row of a list up to
/// `A11Y_ROWS`, else that many around the view (`first`, `page` rows), so a list of thousands
/// costs no more than a short one. Further rows come within reach as the list scrolls (the keys
/// scroll it, as a list view's own).
pub fn reachable_rows(n: usize, first: usize, page: usize) -> std::ops::Range<usize> {
    if n <= A11Y_ROWS {
        return 0..n;
    }
    let before = A11Y_ROWS.saturating_sub(page) / 2;
    let start = first.saturating_sub(before).min(n - A11Y_ROWS);
    start..start + A11Y_ROWS
}

/// The row of `rows` (widget ids `id_of(i)`) a screen reader asked to scroll into view, or to
/// select (AccessKit's click), this frame.
pub fn row_asked_for(
    ui: &Ui,
    rows: std::ops::Range<usize>,
    id_of: impl Fn(usize) -> egui::Id,
) -> Option<usize> {
    use egui::accesskit::Action;
    let targets: Vec<egui::accesskit::NodeId> = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::AccessKitActionRequest(r)
                    if matches!(r.action, Action::ScrollIntoView | Action::Click) =>
                {
                    Some(r.target_node)
                }
                _ => None,
            })
            .collect()
    });
    if targets.is_empty() {
        return None;
    }
    rows.into_iter()
        .find(|i| targets.contains(&id_of(*i).accesskit_id()))
}

/// The one list of whole rows in a box (a dialog's list, a tree drawn as rows): its frame,
/// rows glided by the wheel ([`WheelGlide`]; whole rows by the keys) and moved by a scroll bar
/// when they don't all fit, the row under the pointer tinted, the selected row in the
/// selection colour (grey while the list hasn't the keys), and the keys while it has them:
/// Up, Down, Home, End, Page Up, Page Down move the selection; Enter, Space and Delete are
/// reported. `rc` is where the box is on screen ([`r`]); `paint` draws row `i`'s contents in
/// its English rect, after its background, and may put controls in it (they take their own
/// clicks).
pub fn row_list(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    rc: Rect,
    n: usize,
    row_h: f32,
    st: &mut RowList,
    mut paint: impl FnMut(&mut Ui, &Painter, usize, Rect, &mut RowLook),
) -> RowEvents {
    let mut ev = RowEvents::default();
    let id = widget_id(ui, ("row-list", id));
    let p = ui.painter().clone();
    solid(&p, rc, FIELD_EDGE());
    p.rect_filled(rc.shrink(1.0), 0.0, FIELD());
    // Laid out in English, made where it is on screen.
    let inner = rect(rc).shrink(1.0);
    let page = (((inner.height() - 2.0) / row_h).floor() as usize).max(1);
    let max_first = n.saturating_sub(page);
    let bar = n > page;
    let right = inner.right() - if bar { crate::scrollbar::THICK } else { 0.0 };
    let area = Rect::from_min_max(inner.min, pos2(right, inner.bottom()));
    let resp = ui.interact(rect(inner), id, Sense::click());
    // Named by its owner (#95), not by a caption found near it: that is often a column title
    // ("Size"), and a wrong name is worse than none.
    expose(&resp, egui::Role::ListBox, st.name, true, None);
    // Its arrows (a tree's Left / Right too) stay with it.
    keep_keys(ui, &resp, true, true, false);
    let focused = resp.has_focus();
    // The wheel glides the rows to where its notches take them, a pixel at a time (a
    // touchpad's own travel at once); the list may rest part-way between rows then.
    let (glide, direct) = if ui.rect_contains_pointer(rect(inner)) {
        wheel_travel(ui, row_h, page as f32)
    } else {
        (0.0, 0.0)
    };
    let mut pos = st.first.min(max_first) as f32 * row_h + st.px + direct * row_h;
    st.glide
        .step(ui.ctx(), &mut pos, glide * row_h, max_first as f32 * row_h);
    st.first = (pos / row_h).floor() as usize;
    st.px = pos - st.first as f32 * row_h;
    let glided = st.first;
    if focused && n > 0 {
        let at = st.selected.map_or(-1, |s| s as isize);
        let last = n as isize - 1;
        let step = page as isize;
        let keys = [
            (egui::Key::ArrowDown, at + 1),
            (egui::Key::ArrowUp, (at - 1).max(0)),
            (egui::Key::Home, 0),
            (egui::Key::End, last),
            // As Explorer: first to the last (first) row in view, then a page on.
            (
                egui::Key::PageDown,
                if at < (st.first + page) as isize - 1 {
                    (st.first + page) as isize - 1
                } else {
                    at + step
                },
            ),
            (
                egui::Key::PageUp,
                if at > st.first as isize {
                    st.first as isize
                } else {
                    at - step
                },
            ),
        ];
        for (key, to) in keys {
            if take_key(ui, key) {
                let to = to.clamp(0, last) as usize;
                st.selected = Some(to);
                st.show_row(to, page);
                ev.picked = Some(to);
            }
        }
        if let Some(s) = st.selected {
            // Reported, not taken: a list that ignores it leaves Enter to the dialog's
            // default button (a caller that acts on it takes it).
            if ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none()) {
                ev.entered = Some(s);
            }
            if take_key(ui, egui::Key::Space) {
                ev.space = Some(s);
            }
            if take_key(ui, egui::Key::Delete) {
                ev.delete = Some(s);
            }
        }
    }
    if std::mem::take(&mut st.pending)
        && let Some(s) = st.selected
    {
        st.show_row(s, page);
    }
    // Rows out of view are there for screen readers too (#111): those `reachable_rows` gives
    // (every row of a list up to a few hundred, else that many around the view) while a
    // screen reader is on. A row brought into view by one (Scroll into view, or Select, which
    // AccessKit sends as a click) shows from this frame on.
    // The number of rows is the list's (UI Automation reads a row's "of M" from its list).
    let a11y = ui
        .ctx()
        .accesskit_node_builder(resp.id, |node| node.set_size_of_set(n))
        .is_some();
    // A row more shows, cut, while the view rests between rows.
    let part = usize::from(st.px > 0.0);
    let reach = if a11y {
        reachable_rows(n, st.first, page)
    } else {
        st.first..n.min(st.first + page + part)
    };
    if a11y && let Some(i) = row_asked_for(ui, reach.clone(), |i| id.with(i)) {
        st.show_row(i, page);
    }
    st.first = st.first.min(max_first);
    if st.first == max_first {
        st.px = 0.0;
    }
    // Moved by the keys or a screen reader: whole rows, and the glide is over.
    if st.first != glided {
        st.px = 0.0;
        st.glide.stop(st.first as f32 * row_h);
    }
    ev.focused = focused;
    let clip = p.with_clip_rect(rect(area));
    // The rows (and what their painters put in them: a tree's check boxes) are the list's
    // children in the accessibility tree, not its siblings (#102): a screen reader then says
    // "4 of 12", and UI Automation's Selection finds them under the list.
    let mut rows_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect(inner))
            .accessibility_parent(resp.id),
    );
    // Rows out of view lie above or below it, clipped: nothing of them shows or takes a click.
    rows_ui.set_clip_rect(rect(area).intersect(ui.clip_rect()));
    let part = usize::from(st.px > 0.0);
    let shown = st.first..n.min(st.first + page + part);
    // Drawn to the screen's pixel, so text stays sharp part-way between rows.
    let ppp = ui.ctx().pixels_per_point();
    let px = (st.px * ppp).round() / ppp;
    for i in reach {
        let in_view = shown.contains(&i);
        // On a whole screen pixel, each row, so their grid lines are alike ([`on_pixel`]).
        let top = on_pixel(
            ppp,
            inner.top() + 1.0 + row_h * (i as f32 - st.first as f32) - px,
        );
        let row = Rect::from_min_max(pos2(inner.left(), top), pos2(right, top + row_h));
        let r = rows_ui.interact(rect(row), id.with(i), pointer_only());
        let selected = st.selected == Some(i);
        let fill = if !in_view {
            None
        } else if selected && focused {
            Some(ROW_SELECTED())
        } else if selected {
            Some(SELECTED_INACTIVE())
        } else {
            // Hot, and pressed lighter still on a dark field (subtle_fill's pressed grey was the
            // dark field's own).
            let c = crate::theme::colors();
            match State::of(&r, true) {
                State::Pressed => Some(c.row_pressed),
                state => subtle_fill(state, c),
            }
        };
        if let Some(f) = fill {
            clip.rect_filled(rect(row), 0.0, f);
        }
        if r.clicked() || r.secondary_clicked() || r.double_clicked() {
            resp.request_focus();
            if st.selected != Some(i) {
                st.selected = Some(i);
                ev.picked = Some(i);
                // Selected out of view (a screen reader's Select): brought into view.
                st.pending |= !in_view;
                // This frame drew the old selection; draw (and expose) the new one now, not on
                // the next mouse move: a screen reader's click moves no mouse.
                ui.ctx().request_repaint();
            }
        }
        if r.double_clicked() {
            ev.double_clicked = Some(i);
        }
        if r.secondary_clicked()
            && let Some(at) = r.interact_pointer_pos()
        {
            ev.context = Some((i, at));
        }
        let mut look = RowLook {
            selected,
            ink: if selected && focused {
                SELECTION_TEXT()
            } else {
                TEXT()
            },
            name: String::new(),
        };
        // What the painter puts in the row (a tree's check box) belongs to the row: the row's
        // node made first (named below, once its painter has said what it shows), so those
        // controls find it as their parent.
        ui.ctx().accesskit_node_builder(r.id, |_| {});
        let mut row_ui = rows_ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect(row))
                .accessibility_parent(r.id),
        );
        paint(&mut row_ui, &clip, i, row, &mut look);
        // Named by what its painter drew (#95: rows were read as blank items, and alike rows
        // couldn't be told apart by UI Automation).
        expose(&r, egui::Role::ListBoxOption, &look.name, true, None);
        // Selected as a list's item (UI Automation's SelectionItem), as the download list does;
        // expose's `on` would make it "toggled".
        // Its place among all the rows, not only those drawn (only the rows in view exist).
        ui.ctx().accesskit_node_builder(r.id, |node| {
            node.set_selected(selected);
            // AccessKit counts from 0 (UI Automation and AT-SPI say i + 1).
            node.set_position_in_set(i);
            node.set_size_of_set(n);
            node.add_action(egui::accesskit::Action::ScrollIntoView);
        });
        if selected && focused && in_view {
            focus_ring(&clip, rect(row));
        }
    }
    if focused
        && st
            .selected
            .is_none_or(|s| s < st.first || s >= st.first + page)
    {
        focus_ring(&p, rect(area));
    }
    if bar {
        let track = Rect::from_min_max(pos2(right, inner.top()), inner.max);
        let mut pos = st.first as f32 + st.px / row_h;
        if crate::scrollbar::bar(
            ui,
            id.with("bar"),
            rect(track),
            true,
            n as f32,
            page as f32,
            1.0,
            &mut pos,
        ) {
            st.first = (pos.round().max(0.0) as usize).min(max_first);
            st.px = 0.0;
            st.glide.stop(st.first as f32 * row_h);
        }
    }
    ev
}

/// A Windows 11 check box's 13 × 13 box in `rc`, ticked or not, in `state`: the one look of
/// check boxes and of the check lists (Options → Browsers, Sounds).
/// A check box in a list's row ([`row_list`]): its 13 × 13 box at English `at`, ticked by a
/// click (hot and pressed as any check box), not a Tab stop of its own (the list is: Space
/// ticks the selected row). `salt` names the item, so the box stays its own while the list
/// scrolls. True when clicked.
pub fn row_check(
    ui: &Ui,
    p: &Painter,
    at: Pos2,
    on: &mut bool,
    label: &str,
    salt: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    let rc = rect(Rect::from_min_size(at, vec2(13.0, 13.0)));
    let resp = ui.interact(
        rc.expand(2.0),
        widget_id(ui, ("row-check", salt)),
        pointer_only(),
    );
    if resp.clicked() {
        *on = !*on;
    }
    // Named by its row's item, ticked or not (#95).
    expose(&resp, egui::Role::CheckBox, label, true, Some(*on));
    check_box_glyph(p, rc, *on, State::of(&resp, true));
    resp.clicked()
}

/// A row's check box ([`row_check`] at English `glyph`) shown partly ticked: a folder or a
/// package with only some of its files chosen.
pub fn row_check_mixed(p: &Painter, glyph: Pos2) {
    check_box_mixed(
        p,
        rect(Rect::from_min_size(glyph, vec2(13.0, 13.0))),
        State::Normal,
    );
}

/// A tree row's expander at English point `at`: ▸ when closed (pointing into the row: ◂ in
/// RTL), ▾ when open (Add torrent's folders, Link grabber's packages).
pub fn tree_expander(p: &Painter, at: Pos2, open: bool) {
    let c = MUTED();
    let pts = if open {
        vec![at, pos2(at.x + 8.0, at.y), pos2(at.x + 4.0, at.y + 5.0)]
    } else {
        vec![
            pos2(at.x + 2.0, at.y - 1.0),
            pos2(at.x + 7.0, at.y + 3.5),
            pos2(at.x + 2.0, at.y + 8.0),
        ]
    };
    let pts = pts.into_iter().map(|q| pos2(mx(q.x), q.y)).collect();
    p.add(egui::Shape::convex_polygon(pts, c, Stroke::NONE));
}
