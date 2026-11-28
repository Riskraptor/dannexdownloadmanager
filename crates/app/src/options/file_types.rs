//! Options → File types: the extensions DDM takes from the browser, and the sites and addresses it
//! leaves to it.

use super::*;

impl OptionsDialog {
    pub(super) fn file_types(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        // i18n: Options → File types: the page's title: which downloads DDM takes from the browsers
        header(ui, o, Icon::PageFileTypes, "Downloads DDM takes");
        text(
            &p,
            o.x + 21.0,
            o.y + 103.0,
            // i18n: Options → File types: over the list of file extensions DDM takes from the browsers
            crate::i18n::t("Take these file types from the browsers:"),
            LIGHT_TEXT(),
        );
        multiline(
            ui,
            r(o, 22.0, 110.0, 380.0, 59.0),
            &mut self.s.auto_types,
            "auto-types",
            true,
        );
        // Default as wide as its label in this language (#192), growing to the left.
        let default_at = winui::buttons_to_right(&p, 389.0, 0.0, &[("Default", 73.0)], 200.0)[0];
        if winui::button_light(
            ui,
            r(o, default_at.0, 175.0, default_at.1, 21.0),
            // i18n: button: put back the default list
            "Default",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.s.auto_types = settings::DEFAULT_AUTO_TYPES.into();
        }
        text(
            &p,
            o.x + 21.0,
            o.y + 222.0,
            // i18n: Options → File types: over the list of sites whose downloads stay with the browser
            crate::i18n::t("Leave downloads from these sites to the browser:"),
            LIGHT_TEXT(),
        );
        multiline(
            ui,
            r(o, 22.0, 229.0, 380.0, 49.0),
            &mut self.s.excluded_sites,
            "excluded-sites",
            true,
        );
        let spaces = text(
            &p,
            o.x + 21.0,
            o.y + 293.0,
            // i18n: Options: a note under a list of names (sites, file types): how to write several
            crate::i18n::t("(names separated by spaces)"),
            LIGHT_TEXT(),
        );
        // Short of the note before it.
        let default_at = winui::button_row(
            &p,
            &[("Default", 316.0, 73.0)],
            0.0,
            winui::rect(spaces).right() - o.x + 8.0,
            200.0,
        )[0];
        if winui::button_light(
            ui,
            r(o, default_at.0, 284.0, default_at.1, 21.0),
            // i18n: button: put back the default list
            "Default",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.s.excluded_sites = settings::DEFAULT_EXCLUDED_SITES.into();
        }
        text(
            &p,
            o.x + 21.0,
            o.y + 327.0,
            // i18n: Options → File types: over Edit list...: addresses (with * patterns) whose downloads stay with the browser
            crate::i18n::t("Leave these addresses to the browser:"),
            LIGHT_TEXT(),
        );
        let (list_x, list_w) =
            winui::button_row_from_left(&p, &[("Edit list...", 22.0, 135.0)], 0.0, 389.0, 260.0)[0];
        if winui::button_light(
            ui,
            r(o, list_x, 334.0, list_w, 21.0),
            "Edit list...",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.sub = Some(Sub::Addresses(self.s.excluded_addresses.join("\n")));
        }
        // DDM's: a receipt or an icon isn't worth a download window.
        let label = crate::i18n::t("Don't take files smaller than (KB, 0 = any size):");
        // Before its box: a longer translation is cut with "..." and shown whole on hover.
        let (rect, cut) = winui::label_into(
            &p,
            o.x + 21.0,
            o.y + 383.0,
            label,
            LIGHT_TEXT(),
            o.x + 316.0,
            o.x + 21.0,
        );
        if cut {
            winui::full_on_hover(ui, rect, ("types-min", 383), label);
        }
        winui::number_field(
            ui,
            r(o, 316.0, 370.0, 73.0, 21.0),
            &mut self.min_kb,
            0,
            10_000_000,
            true,
            LIGHT_PAGE(),
        );
    }
}
