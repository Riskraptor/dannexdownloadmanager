//! Options → Save to: each category's folder and extensions, the default folder and the temporary
//! directory.

use super::*;

impl OptionsDialog {
    pub(super) fn save_to(&mut self, ui: &mut Ui, o: Pos2, downloads: &std::path::Path) {
        let p = ui.painter().clone();
        header(ui, o, Icon::PageSaveTo, "Categories, file types, folders");
        // i18n: Options → Save to: group box: the categories, their file types and folders
        group(ui, o, 16.0, 94.0, 392.0, 201.0, "Categories");
        // i18n: noun: label before the list of categories
        text(
            &p,
            o.x + 30.0,
            o.y + 127.0,
            crate::i18n::t("Category"),
            LIGHT_TEXT(),
        );
        // IDM's, Torrents and the user's (the registry follows this dialog's edits).
        let cats = Category::all();
        self.category = self.category.min(cats.len() - 1);
        let names: Vec<String> = cats.iter().map(|c| c.label().into_owned()).collect();
        let labels: Vec<&str> = names.iter().map(String::as_str).collect();
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 30.0, 136.0, 212.0, 21.0),
            &labels,
            self.category,
            true,
            "opt-category",
        ) {
            // Keep the folder typed for the category being left.
            self.commit_cat_dir(downloads);
            self.category = i;
            self.cat_dir = display_dir(&self.s.category_dir(cats[i], downloads));
        }
        // New / Edit... (D-021): a category of the user's; a built-in one's folder. One width for
        // both, as wide as the longer label in this language (#192), into the room after the
        // list (it ends at 242).
        let (col_x, col_w) = winui::buttons_to_right(
            &p,
            392.0,
            0.0,
            &[(winui::widest_label(&p, &["New", "Edit..."]), 73.0)],
            142.0,
        )[0];
        if winui::button_light(
            ui,
            r(o, col_x, 105.0, col_w, 21.0),
            // i18n: button: create a new item
            "New",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.commit_cat_dir(downloads);
            self.cat_edit = Some(crate::categories::CategoryDialog::new());
        }
        if winui::button_light(
            ui,
            r(o, col_x, 137.0, col_w, 21.0),
            // i18n: verb: edit the item
            "Edit...",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.commit_cat_dir(downloads);
            let cat = cats[self.category];
            let folder = self.s.category_dir(cat, downloads);
            self.cat_edit = Some(crate::categories::CategoryDialog::edit(cat, &folder));
        }
        let cat = cats[self.category];
        let label = cat.shown();
        let with_category = |template: &str| crate::i18n::fmt(template, &[("category", &label)]);
        // i18n: Options → Save to: over the file types of the chosen category; {category} is its name
        let line = with_category(crate::i18n::t("File types that go in {category}:"));
        winui::text_in_window(ui, &p, o.x + 31.0, o.y + 178.0, &line, LIGHT_TEXT());
        let types = cat.types_text();
        readonly(ui, r(o, 31.0, 185.0, 362.0, 23.0), &types);
        // i18n: Options → Save to: over the chosen category's folder (and the folder picker's title); {category} is its name
        let dir_line = with_category(crate::i18n::t("Save {category} files to:"));
        winui::text_in_window(ui, &p, o.x + 30.0, o.y + 230.0, &dir_line, LIGHT_TEXT());
        // Browse as wide as its label in this language (#192); the box ends 7 px before it.
        let (browse_x, browse_w) =
            winui::buttons_to_right(&p, 392.0, 0.0, &[("Browse", 73.0)], 160.0)[0];
        winui::edit_light_on(
            ui,
            r(o, 31.0, 239.0, browse_x - 7.0 - 31.0, 23.0),
            &mut self.cat_dir,
            true,
            false,
            LIGHT_PAGE(),
        );
        if winui::button_light(
            ui,
            r(o, browse_x, 240.0, browse_w, 21.0),
            // i18n: verb: choose a folder
            "Browse",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            let (title, from) = (dir_line.clone(), self.cat_dir.clone());
            winui::start_pick(ui.ctx(), "options-category-folder", move || {
                rfd::FileDialog::new()
                    .set_title(title)
                    .set_directory(from)
                    .pick_folder()
            });
        }
        if let Some(dir) = winui::picked(ui.ctx(), "options-category-folder") {
            self.cat_dir = display_dir(&dir);
        }
        self.commit_cat_dir(downloads);
        // i18n: Options → Save to: checkbox; a folder chosen in Download File Info becomes this category's folder; {category} is its name
        let line = with_category(crate::i18n::t(
            "Remember the last folder chosen for {category}",
        ));
        winui::checkbox_light(
            ui,
            o + vec2(33.0, 271.0),
            &mut self.s.change_folder_on_last,
            &line,
            true,
        );
        winui::checkbox_light(
            ui,
            o + vec2(33.0, 306.0),
            &mut self.s.set_creation_date,
            // i18n: Options → Save to: checkbox; a finished file gets the date the server gives for it
            "Give files the server's date",
            true,
        );
        // i18n: Options → Save to: group box: the folder where unfinished downloads keep their part files
        group(ui, o, 16.0, 334.0, 392.0, 118.0, "Temporary folder");
        // Where downloads keep their part files until they are complete (naming::set_temp_dir);
        // empty, beside each file (D-004: no copying at the end, the default).
        let (browse_x, browse_w) =
            winui::buttons_to_right(&p, 390.0, 0.0, &[("Browse", 73.0)], 160.0)[0];
        winui::edit_light_on(
            ui,
            r(o, 30.0, 349.0, browse_x - 6.0 - 30.0, 23.0),
            &mut self.s.temp_dir,
            true,
            false,
            LIGHT_PAGE(),
        );
        if winui::button_light(
            ui,
            r(o, browse_x, 350.0, browse_w, 21.0),
            // i18n: verb: choose a folder
            "Browse",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            // i18n: title of the folder dialog of Options → Save to → Temporary directory → Browse
            let title = crate::i18n::t("Temporary directory").to_owned();
            let from = Some(self.s.temp_dir.trim().to_owned()).filter(|d| !d.is_empty());
            winui::start_pick(ui.ctx(), "options-temp-dir", move || {
                let dialog = rfd::FileDialog::new().set_title(title);
                match from {
                    Some(dir) => dialog.set_directory(dir),
                    None => dialog,
                }
                .pick_folder()
            });
        }
        if let Some(dir) = winui::picked(ui.ctx(), "options-temp-dir") {
            self.s.temp_dir = dir.display().to_string();
        }
        // Inside the group box (21..401): from its controls' left edge to their right one, Browse's.
        let note = if self.s.temp_dir.trim().is_empty() {
            crate::i18n::t(
                "Empty: each download writes into a part file next to its file, which\nbecomes the file when done. A folder here holds the part files instead.\nDDM removes leftover part files by itself (see the Downloads menu).",
            )
        } else {
            crate::i18n::t(
                "Part files stay here until their downloads complete. On another drive\nthan a download's folder, its file is then copied over. DDM removes\nleftovers by itself. Empty it to write next to each file again.",
            )
        };
        winui::paragraph(
            &p,
            o.x + 30.0,
            o.y + 387.0,
            note,
            LIGHT_TEXT(),
            o.x + 390.0,
            13.0,
        );
        // #93: the built-in categories by file type as shipped again, at the page's foot.
        let (restore_x, restore_w) = winui::button_row_from_left(
            &p,
            &[("Restore the built-in categories", 21.0, 200.0)],
            0.0,
            401.0,
            380.0,
        )[0];
        if winui::button_light(
            ui,
            r(o, restore_x, 460.0, restore_w, 21.0),
            "Restore the built-in categories",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.commit_cat_dir(downloads);
            let text = crate::i18n::fmt(
                crate::i18n::t(
                    "Bring back {names} as they were shipped: their names, file types and folders? Your own categories, General and Torrents stay as they are. Downloads already moved to General stay there.",
                ),
                &[("names", &crate::categories::restored_names())],
            );
            self.restore_ask = Some(crate::dialogs::Confirm::new(
                crate::i18n::mark("Restore the built-in categories"),
                text,
            ));
        }
    }

    /// The typed folder for the selected category becomes its own (or the default again).
    pub(super) fn commit_cat_dir(&mut self, downloads: &std::path::Path) {
        let cats = Category::all();
        let Some(&cat) = cats.get(self.category) else {
            return;
        };
        let base = if cat == Category::General {
            std::borrow::Cow::Borrowed(downloads)
        } else {
            crate::model::default_downloads(downloads, &self.s.category_dirs)
        };
        let default = display_dir(&cat.default_folder(&base));
        let typed = self.cat_dir.trim();
        let own = (!typed.is_empty()
            && typed.trim_end_matches(['\\', '/']) != default.trim_end_matches(['\\', '/']))
        .then(|| typed.to_owned());
        self.s.set_category_dir(cat, own);
    }
}

/// A folder as the dialogs show it: the full path, as IDM's Options shows it, ending in the
/// system's separator (a backslash on Windows).
pub(super) fn display_dir(dir: &std::path::Path) -> String {
    let s = dir.display().to_string();
    if s.ends_with(std::path::MAIN_SEPARATOR) {
        s
    } else {
        format!("{s}{}", std::path::MAIN_SEPARATOR)
    }
}
