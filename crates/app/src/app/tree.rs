//! The Categories tree: its rows (categories, queues, Site grabber projects), what each node holds,
//! the keys that move along it and its right-click menu.

use super::*;

/// Tree rows carry what they select and which branch they toggle.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Branch {
    Leaf,
    All,
    Unfinished,
    Finished,
    Queues,
    Grabber,
}

/// What Left or Right does in the Categories tree ([`tree_side_step`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TreeStep {
    /// Open or close the node the keys are on.
    Toggle,
    /// Go to this row (its parent, its first child).
    Select(usize),
}

impl App {
    /// Does `view` (a node of the Categories tree) hold this download?
    pub(super) fn in_view(&self, view: View, i: &Item) -> bool {
        // Not in the list yet (started early behind Download File Info): in no view, no count.
        if !i.listed() {
            return false;
        }
        // A grabber project's that keeps its downloads apart: under it and its queue only.
        if !matches!(view, View::Queue(_) | View::Grabber(_)) && self.grab_kept_apart(i.id) {
            return false;
        }
        let in_cat = |c: Option<Category>| c.is_none_or(|c| i.category == c);
        match view {
            View::All => true,
            View::Cat(c) => i.category == c,
            View::Unfinished(c) => i.status != Status::Complete && in_cat(c),
            View::Finished(c) => i.status == Status::Complete && in_cat(c),
            View::Queue(q) => self.queues.get(q).is_some_and(|q| q.items.contains(&i.id)),
            View::Grabber(p) => self.grab_in_view(p, i),
        }
    }

    /// Categories right-click: categories (Browse, Properties, Add category, Delete; greyed
    /// on the built-in nodes) and queues (Start now, Stop, Edit queue, Schedule, Delete,
    /// Create new queue).
    pub(super) fn node_entries(&self, node: Option<View>) -> Vec<MenuEntry<Cmd>> {
        if let Some(entries) = self.grab_node_entries(node) {
            return entries;
        }
        use MenuEntry::{Item as I, Sep};
        let off = |label| I(label, Cmd::None, false, false);
        let queue = match node {
            Some(View::Queue(q)) => Some(q),
            None => None,
            Some(v) => {
                let cat = match v {
                    View::Cat(c) | View::Unfinished(Some(c)) | View::Finished(Some(c)) => Some(c),
                    _ => None,
                };
                return vec![
                    match cat {
                        // i18n: verb: opens this category's download folder
                        Some(c) => I("Browse", Cmd::Browse(c), true, false),
                        // i18n: verb: opens this category's download folder
                        None => off("Browse"),
                    },
                    match cat {
                        // i18n: noun: opens the category's properties
                        Some(c) => I("Properties", Cmd::CategoryProps(c), true, false),
                        // i18n: noun: opens the category's properties
                        None => off("Properties"),
                    },
                    Sep,
                    I("Add category", Cmd::AddCategory, true, false),
                    match cat {
                        // i18n: verb: deletes the category
                        Some(c) if c.editable() => I("Delete", Cmd::DeleteCategory(c), true, false),
                        // i18n: verb: deletes the category
                        _ => off("Delete"),
                    },
                ];
            }
        };
        let running = queue.is_some_and(|q| self.queues[q].running);
        vec![
            match queue {
                Some(q) => I("Start now", Cmd::StartQueue(q), !running, false),
                None => off("Start now"),
            },
            match queue {
                // i18n: verb: stops the queue
                Some(q) => I("Stop", Cmd::StopQueue(q), running, false),
                // i18n: verb: stops the queue
                None => off("Stop"),
            },
            Sep,
            match queue {
                Some(q) => I("Edit queue", Cmd::EditQueue(q), true, false),
                None => off("Edit queue"),
            },
            match queue {
                // i18n: command: opens this queue's schedule (when it starts and stops)
                Some(q) => I("Schedule", Cmd::ScheduleQueue(q), true, false),
                // i18n: command: opens this queue's schedule (when it starts and stops)
                None => off("Schedule"),
            },
            Sep,
            // The two built-in queues cannot be deleted.
            match queue {
                // i18n: verb: deletes the queue
                Some(q) if q >= 2 => I("Delete", Cmd::DeleteQueue(q), !running, false),
                // i18n: verb: deletes the queue
                _ => off("Delete"),
            },
            Sep,
            I("Create new queue", Cmd::NewQueue, true, false),
        ]
    }

    // ---- painting --------------------------------------------------------------------------

    /// The Categories tree: each row, the view it selects (None: just toggles) and its branch.
    pub(super) fn tree_rows(&self) -> Vec<(TreeRow<'static>, Option<View>, Branch)> {
        let cur = self.view;
        let row = |level, label: &'static str, expander, icon, badge, view: Option<View>| TreeRow {
            level,
            label: label.into(),
            expander,
            selected: view == Some(cur),
            icon,
            badge,
            count: None,
        };
        let mut rows = Vec::new();
        // Silk icons in IDM's roles: folder, folder + green arrow, folder + tick, page stacks.
        let mut branch =
            |label, open: bool, icon, badge, view: View, b: Branch, child: fn(Category) -> View| {
                rows.push((
                    row(0, label, Some(open), icon, badge, Some(view)),
                    Some(view),
                    b,
                ));
                if open {
                    for c in Category::named() {
                        let v = child(c);
                        rows.push((
                            TreeRow {
                                level: 1,
                                label: c.label(),
                                expander: None,
                                selected: v == cur,
                                icon: category_icon(c),
                                badge: None,
                                count: None,
                            },
                            Some(v),
                            Branch::Leaf,
                        ));
                    }
                }
            };
        branch(
            "All Downloads",
            self.all_open,
            IconKey::Themed("cat-all", "folder"),
            None,
            View::All,
            Branch::All,
            View::Cat,
        );
        branch(
            // i18n: adjective: the downloads that are not complete yet
            "Unfinished",
            self.unfinished_open,
            IconKey::Themed("cat-unfinished", "folder_go"),
            None,
            View::Unfinished(None),
            Branch::Unfinished,
            |c| View::Unfinished(Some(c)),
        );
        branch(
            // i18n: adjective: the downloads that are complete
            "Finished",
            self.finished_open,
            IconKey::Themed("cat-finished", "folder"),
            Some("tick"),
            View::Finished(None),
            Branch::Finished,
            |c| View::Finished(Some(c)),
        );
        let stack = IconKey::Silk("page_white_stack");
        rows.push((
            row(
                0,
                // i18n: plural noun: the download queues
                "Queues",
                Some(self.queues_open),
                IconKey::Themed("cat-queues", "folder"),
                Some("page_white_stack"),
                None,
            ),
            None,
            Branch::Queues,
        ));
        if self.queues_open {
            // Every queue, the user's too (IDM lists them all here).
            for (q, queue) in self.queues.iter().enumerate() {
                let view = Some(View::Queue(q));
                rows.push((
                    TreeRow {
                        level: 1,
                        label: queue.name.clone().into(),
                        expander: None,
                        selected: view == Some(cur),
                        icon: stack.clone(),
                        badge: queue.sync.then_some("tick"),
                        count: None,
                    },
                    view,
                    Branch::Leaf,
                ));
            }
        }
        self.grab_tree_rows(&mut rows);
        // Each top node's downloads, as its list would show them (before column filters):
        // the pane is narrow, its children's names fill it already.
        for (row, view, _) in &mut rows {
            if row.level == 0
                && let Some(v) = view
            {
                let n = self.items.iter().filter(|i| self.in_view(*v, i)).count();
                row.count = (n > 0).then_some(n);
            }
        }
        rows
    }
}

/// Left (`right` false) or Right on row `at` of the Categories tree, whose rows are `(level, open
/// or shut or no children, has a view of its own)`, as a Windows tree view has them: Right opens
/// a shut branch, then goes to its first child; Left closes an open branch, else goes to the
/// parent. Only rows with a view of their own are gone to (a header such as "Queues" isn't).
pub(super) fn tree_side_step(
    rows: &[(usize, Option<bool>, bool)],
    at: usize,
    right: bool,
) -> Option<TreeStep> {
    let &(level, open, _) = rows.get(at)?;
    match (right, open) {
        (true, Some(false)) | (false, Some(true)) => Some(TreeStep::Toggle),
        (true, Some(true)) => rows[at + 1..]
            .iter()
            .take_while(|r| r.0 > level)
            .position(|r| r.2)
            .map(|k| TreeStep::Select(at + 1 + k)),
        (true, None) => None,
        (false, _) => rows[..at]
            .iter()
            .rposition(|r| r.0 < level)
            .filter(|&k| rows[k].2)
            .map(TreeStep::Select),
    }
}
