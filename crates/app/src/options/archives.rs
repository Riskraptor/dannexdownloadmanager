//! Options → Archives (ours, #37): extracting archives when their download completes, and what
//! becomes of them.

use super::*;

impl OptionsDialog {
    /// Archives (ours, #37): extracting archives when their download completes, what becomes
    /// of them, and Link grabber's ways.
    pub(super) fn archives(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        // i18n: Options page title: extracting downloaded archives, and Link grabber
        let title = crate::i18n::mark("Archives and Link grabber");
        header(ui, o, Icon::PageArchives, title);
        let s = &mut self.s;
        // i18n: Options → Archives: group box title
        group(ui, o, 16.0, 92.0, 392.0, 196.0, "Archives");
        winui::checkbox_light(
            ui,
            o + vec2(30.0, 110.0),
            &mut s.extract_auto,
            // i18n: Options → Archives check box (a Link grabber package has its own "Extract when done")
            "Extract archives when their download completes",
            true,
        );
        winui::checkbox_light(
            ui,
            o + vec2(30.0, 134.0),
            &mut s.extract_delete,
            if cfg!(windows) {
                // i18n: Options → Archives check box
                "Then move the archives to the Recycle Bin"
            } else {
                // i18n: Options → Archives check box (Linux and macOS)
                "Then move the archives to the Trash"
            },
            true,
        );
        text(
            &p,
            o.x + 30.0,
            o.y + 174.0,
            // i18n: Options → Archives: a label before the list of where extracted files go
            crate::i18n::t("What an archive holds goes:"),
            LIGHT_TEXT(),
        );
        const INTO: [&str; 3] = [
            // i18n: Options → Archives: where extracted files go
            crate::i18n::mark("Into a folder named after it, when it holds several things"),
            // i18n: Options → Archives: where extracted files go
            crate::i18n::mark("Always into a folder named after it"),
            // i18n: Options → Archives: where extracted files go
            crate::i18n::mark("Straight into the archive's folder"),
        ];
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 30.0, 181.0, 364.0, 21.0),
            &INTO,
            s.extract_subfolder.min(2) as usize,
            true,
            "opt-extract-into",
        ) {
            s.extract_subfolder = i as u8;
        }
        winui::paragraph(
            &p,
            o.x + 30.0,
            o.y + 226.0,
            crate::i18n::t(
                "ZIP, 7Z, RAR, TAR, GZ and XZ; a split archive (.part1.rar, .7z.001) once\nevery volume is in. A file there already is kept: the new one is numbered.",
            ),
            LIGHT_TEXT(),
            o.x + 400.0,
            13.0,
        );
        let (rar, ink) = match &self.seven_zip {
            Some(path) => (
                crate::i18n::fmt(
                    // i18n: Options → Archives: {path} is where 7-Zip's program is
                    crate::i18n::t("7z and RAR archives are extracted by 7-Zip ({path})."),
                    &[("path", &crate::i18n::ltr(&path.display().to_string()))],
                ),
                LIGHT_TEXT(),
            ),
            None => (
                // i18n: Options → Archives, when 7-Zip isn't installed
                crate::i18n::t(
                    "7z and RAR archives need 7-Zip (7-zip.org), which isn't installed.",
                )
                .to_owned(),
                winui::ERROR(),
            ),
        };
        winui::fit(
            ui,
            &p,
            o.x + 30.0,
            o.y + 272.0,
            &rar,
            ink,
            o.x + 400.0,
            "opt-rar-tool",
        );
        // i18n: Options → Archives: group box title
        group(ui, o, 16.0, 300.0, 392.0, 100.0, "Link grabber");
        winui::checkbox_light(
            ui,
            o + vec2(30.0, 318.0),
            &mut s.grabber_check,
            // i18n: Options → Archives → Link grabber check box (as the browser's Download all links hands them over: each is looked up)
            "Look links up when they arrive (name, size, online)",
            true,
        );
        winui::checkbox_light(
            ui,
            o + vec2(30.0, 342.0),
            &mut s.grabber_hide_pages,
            "Hide web pages",
            true,
        );
        winui::paragraph(
            &p,
            o.x + 30.0,
            o.y + 380.0,
            crate::i18n::t(
                "Tasks → Link grabber, or the browser's \"Download all links\": a split\narchive's volumes are one package, the other links of a page another.",
            ),
            LIGHT_DISABLED(),
            o.x + 400.0,
            13.0,
        );
    }
}
