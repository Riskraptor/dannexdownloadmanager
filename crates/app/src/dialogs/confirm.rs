//! The questions: Delete's confirmation, a duplicate download link, the queue to add to, a plain
//! OK / Cancel, and deleting a category.

use super::*;

// ---- Confirm deletion of downloads (client 374 × 120, light) ----------------------------------

/// "Confirm deletion of downloads": Delete / Remove / Delete All Completed. Asks whether the
/// files go too (user, 2026-10-29: IDM's checkbox; for an unfinished download its data so far),
/// for one download or several. `Done((files, dont_show_again))` on Yes.
pub struct ConfirmDelete {
    pub ids: Vec<u64>,
    /// The file name when there is one download.
    pub(super) name: String,
    /// One download, not complete (IDM's wording).
    pub(super) unfinished: bool,
    pub(super) files: bool,
    pub(super) dont_show: bool,
}

impl ConfirmDelete {
    /// `files`: the checkbox's first state, ticked only by Shift+Delete: never remembered
    /// (user, 2026-11-25: deleting files must always be asked for anew).
    pub fn new(ids: Vec<u64>, name: String, unfinished: bool, files: bool) -> Self {
        ConfirmDelete {
            ids,
            name,
            unfinished,
            files,
            dont_show: false,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<(bool, bool)> {
        let n = self.ids.len() as u64;
        let extra = grow_extra(ctx, "confirm-delete");
        question(
            ctx,
            "confirm-delete",
            "Confirm deletion of downloads",
            [374.0, 146.0 + extra],
            false,
            winui::LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let black = winui::LIGHT_TEXT();
                let text = if n == 1 && self.unfinished {
                    crate::i18n::t(
                        "The selected download is not complete. Are you sure you want to delete
it from DDM list of downloads?",
                    )
                    .to_owned()
                } else if n == 1 {
                    // The whole name (two similar names must stay apart): the box grows.
                    let name = self.name.clone();
                    crate::i18n::fmt(
                        crate::i18n::t(
                            "Are you sure you want to delete \"{name}\" from DDM list of downloads?",
                        ),
                        // A file name keeps its own direction (an Arabic one reads right to left).
                        &[("name", &crate::i18n::isolate(&name))],
                    )
                } else {
                    // Two or more (one download has its own sentences above); the singular form
                    // is for languages whose "one" form also counts 21, 31…
                    crate::i18n::fmt(
                        // i18n: never shown for a single download
                        &crate::i18n::tn(
                            n,
                            "Are you sure you want to delete {n} download from DDM list of downloads?",
                            "Are you sure you want to delete these {n} downloads from DDM list of downloads?",
                        ),
                        &[("n", &crate::i18n::integer(n))],
                    )
                };
                let lines =
                    winui::paragraph(&p, o.x + 11.0, o.y + 21.0, &text, black, o.x + 362.0, 13.0);
                grow_to(ui.ctx(), "confirm-delete", lines, 3);
                let o = o + vec2(0.0, extra);
                let files = if n == 1 {
                    crate::i18n::t("Also delete the file from disk").to_owned()
                } else {
                    crate::i18n::fmt(
                        // i18n: never shown for a single file
                        &crate::i18n::tn(
                            n,
                            "Also delete the {n} file from disk",
                            "Also delete the {n} files from disk",
                        ),
                        &[("n", &crate::i18n::integer(n))],
                    )
                };
                winui::checkbox_light(ui, o + vec2(24.0, 60.0), &mut self.files, &files, true);
                let yes = winui::button_light(
                    ui,
                    r(o, 94.0, 86.0, 73.0, 21.0),
                    "Yes",
                    ButtonKind::Default,
                    false,
                )
                .clicked();
                let no = winui::button_light(
                    ui,
                    r(o, 207.0, 86.0, 73.0, 21.0),
                    "No",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                winui::checkbox_light(
                    ui,
                    o + vec2(24.0, 120.0),
                    &mut self.dont_show,
                    "Don't show this again",
                    true,
                );
                if no || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if yes || winui::enter_pressed(ui) {
                    Outcome::Done((self.files, self.dont_show))
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

/// "Duplicate download link": the address is already in the list (Options → Downloads → "If a
/// duplicate download link is added: Show a dialog and ask what to do"). `Done` with the
/// choice, numbered as Options lists them (1 numbered name, 2 overwrite, 3 open or resume).
pub struct Duplicate {
    pub(super) name: String,
    pub(super) choice: usize,
}

impl Duplicate {
    pub fn new(name: String) -> Self {
        Duplicate { name, choice: 1 }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<usize> {
        question(
            ctx,
            "duplicate",
            "Duplicate download link",
            [460.0, 156.0],
            false,
            winui::LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let ink = winui::LIGHT_TEXT();
                winui::text(
                    &p,
                    o.x + 11.0,
                    o.y + 21.0,
                    crate::i18n::t("This address is already in the list of downloads:"),
                    ink,
                );
                let name = crate::i18n::isolate(&self.name);
                let (rect, cut) =
                    winui::text_fit(&p, o.x + 11.0, o.y + 39.0, &name, ink, o.x + 449.0);
                if cut {
                    winui::full_on_hover(ui, rect, "duplicate-name", &self.name);
                }
                let mut group = Vec::new();
                for (k, label) in crate::options::DUPLICATE.iter().enumerate().skip(1) {
                    let y = 52.0 + 20.0 * (k - 1) as f32;
                    let resp =
                        winui::radio_light(ui, o + vec2(14.0, y), self.choice == k, label, true);
                    if resp.clicked() {
                        self.choice = k;
                    }
                    group.push(resp);
                }
                if let Some(j) = winui::radio_keys(ui, &group) {
                    self.choice = j + 1;
                }
                let ok = winui::button_light(
                    ui,
                    r(o, 295.0, 124.0, 73.0, 21.0),
                    "OK",
                    ButtonKind::Default,
                    false,
                )
                .clicked();
                let cancel = winui::button_light(
                    ui,
                    r(o, 376.0, 124.0, 73.0, 21.0),
                    "Cancel",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                if cancel || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if ok || winui::enter_pressed(ui) {
                    Outcome::Done(self.choice)
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

/// The queue selection panel (Options → Downloads → "Show queue selection panel on pressing
/// Download Later" / "on closing batch download dialogs"): which queue the downloads join, and
/// whether it starts now. `Done((queue, start))`; Cancel leaves them in the list, in no queue.
pub struct QueuePick {
    pub(super) names: Vec<String>,
    pub(super) chosen: usize,
    pub(super) start: bool,
}

impl QueuePick {
    pub fn new(names: Vec<String>) -> Self {
        QueuePick {
            names,
            chosen: 0,
            start: false,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<(usize, bool)> {
        window(
            ctx,
            "queue-pick",
            "Download later",
            [340.0, 110.0],
            false,
            winui::LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                winui::text_right(
                    &p,
                    o.x + 92.0,
                    o.y + 26.0,
                    crate::i18n::t("Add to queue:"),
                    winui::LIGHT_TEXT(),
                );
                let names: Vec<&str> = self.names.iter().map(String::as_str).collect();
                if let Some(i) = winui::combo_light(
                    ui,
                    r(o, 100.0, 12.0, 228.0, 21.0),
                    &names,
                    self.chosen,
                    true,
                    "queue-pick",
                ) {
                    self.chosen = i;
                }
                winui::checkbox_light(
                    ui,
                    o + vec2(100.0, 44.0),
                    &mut self.start,
                    "Start the queue now",
                    true,
                );
                let ok = winui::button_light(
                    ui,
                    r(o, 174.0, 78.0, 73.0, 21.0),
                    "OK",
                    ButtonKind::Default,
                    false,
                )
                .clicked();
                let cancel = winui::button_light(
                    ui,
                    r(o, 255.0, 78.0, 73.0, 21.0),
                    "Cancel",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                if cancel || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if ok || winui::enter_pressed(ui) {
                    Outcome::Done((self.chosen, self.start))
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

/// A question with Yes / No (deleting a queue…). `Done` on Yes.
pub struct Confirm {
    pub(super) title: &'static str,
    pub(super) text: String,
}

impl Confirm {
    pub fn new(title: &'static str, text: String) -> Self {
        Confirm { title, text }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let text = self.text.clone();
        let key = format!("confirm-{}", self.title);
        let extra = grow_extra(ctx, &key);
        question(
            ctx,
            &key,
            self.title,
            [400.0, 96.0 + extra],
            false,
            winui::BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let lines = winui::paragraph(
                    &p,
                    o.x + 11.0,
                    o.y + 22.0,
                    &text,
                    winui::TEXT(),
                    o.x + 389.0,
                    13.0,
                );
                grow_to(ui.ctx(), &key, lines, 3);
                let o = o + vec2(0.0, extra);
                let yes = winui::button(
                    ui,
                    r(o, 234.0, 64.0, 73.0, 21.0),
                    "Yes",
                    ButtonKind::Default,
                    false,
                    None,
                )
                .clicked();
                let no = winui::button(
                    ui,
                    r(o, 316.0, 64.0, 73.0, 21.0),
                    "No",
                    ButtonKind::Normal,
                    false,
                    None,
                )
                .clicked();
                if no || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if yes || winui::enter_pressed(ui) {
                    Outcome::Done(())
                } else {
                    Outcome::Open
                }
            },
        )
    }
}

/// Delete category (#93): what happens to its downloads is asked each time. Moving them to
/// General is the default; or they leave the list (a running one can't, so it moves). Files on
/// disk stay either way.
pub struct DeleteCategory {
    pub category: crate::model::Category,
    /// Remove its downloads from the list (else: move them to General).
    pub(super) remove: bool,
}

impl DeleteCategory {
    pub fn new(category: crate::model::Category) -> Self {
        DeleteCategory {
            category,
            remove: false,
        }
    }

    /// Opened with the second choice made (`/show category-remove:<name>`, the checks').
    pub fn removing(mut self) -> Self {
        self.remove = true;
        self
    }

    /// Done(remove): Delete was pressed; `remove` is the second choice.
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<bool> {
        let key = "delete-category";
        let extra = grow_extra(ctx, key);
        let text = crate::i18n::fmt(
            crate::i18n::t("Delete the category \"{category}\"? Its files stay where they are."),
            &[("category", &crate::i18n::isolate(&self.category.shown()))],
        );
        question(
            ctx,
            key,
            "Delete category",
            [500.0, 150.0 + extra],
            false,
            winui::LIGHT_BG(),
            |ui, o| {
                let p = ui.painter().clone();
                let lines = winui::paragraph(
                    &p,
                    o.x + 11.0,
                    o.y + 22.0,
                    &text,
                    winui::LIGHT_TEXT(),
                    o.x + 489.0,
                    13.0,
                );
                grow_to(ui.ctx(), key, lines, 2);
                let o = o + vec2(0.0, extra);
                // i18n: choice in Delete category: what happens to the category's downloads
                let to_general = crate::i18n::mark("Move its downloads to General");
                let off_list = crate::i18n::mark(
                    // i18n: choice in Delete category: its downloads leave the list (not their files; running ones move to General)
                    "Remove its downloads from the list (running ones move to General)",
                );
                if winui::radio_light(ui, o + vec2(14.0, 52.0), !self.remove, to_general, true)
                    .clicked()
                {
                    self.remove = false;
                }
                if winui::radio_light(ui, o + vec2(14.0, 75.0), self.remove, off_list, true)
                    .clicked()
                {
                    self.remove = true;
                }
                // As wide as their labels in this language (#192).
                let [delete_at, cancel_at] = winui::buttons_to_right(
                    &p,
                    488.0,
                    8.0,
                    &[("Delete", 73.0), ("Cancel", 73.0)],
                    140.0,
                )[..] else {
                    unreachable!("two buttons")
                };
                // i18n: button: deletes the category
                let delete = winui::button_light(
                    ui,
                    r(o, delete_at.0, 118.0, delete_at.1, 21.0),
                    "Delete",
                    ButtonKind::Default,
                    false,
                )
                .clicked();
                let cancel = winui::button_light(
                    ui,
                    r(o, cancel_at.0, 118.0, cancel_at.1, 21.0),
                    "Cancel",
                    ButtonKind::Normal,
                    false,
                )
                .clicked();
                if cancel || winui::escape_pressed(ui) {
                    Outcome::Closed
                } else if delete || winui::enter_pressed(ui) {
                    Outcome::Done(self.remove)
                } else {
                    Outcome::Open
                }
            },
        )
    }
}
