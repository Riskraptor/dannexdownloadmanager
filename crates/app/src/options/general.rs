//! Options → General: browser integration (the browsers, the keys that hold a click, the download
//! panels over videos), the DDM menu in the browser, and the window's own choices.

use super::*;

// i18n: Options → General: group box over the list of web browsers whose downloads DDM takes
pub(super) const BROWSERS_CAPTION: &str = crate::i18n::mark("Take downloads from these browsers:");

/// General → Download panels → Edit...: a copy of the button's settings, kept on OK.
pub(super) struct PanelsEdit {
    pub(super) on: bool,
    pub(super) video: bool,
    pub(super) audio: bool,
    /// "Not over videos or sounds shorter than", seconds as typed.
    pub(super) min_s: String,
    /// Its first corner: an index into `mdm_ipc::capture::PANEL_CORNERS`.
    pub(super) corner: usize,
    pub(super) sites: String,
    /// What Export... or Import... came to.
    pub(super) said: String,
}

impl PanelsEdit {
    pub(super) fn from(s: &Settings) -> Self {
        let corner = mdm_ipc::capture::PANEL_CORNERS
            .iter()
            .position(|c| *c == s.video_panel_corner)
            .unwrap_or(0);
        PanelsEdit {
            on: s.video_panel,
            video: s.video_panel_video,
            audio: s.video_panel_audio,
            min_s: s.video_panel_min_s.to_string(),
            corner,
            sites: s.video_panel_sites.clone(),
            said: String::new(),
        }
    }

    pub(super) fn save(self, s: &mut Settings) {
        s.video_panel = self.on;
        s.video_panel_video = self.video;
        s.video_panel_audio = self.audio;
        s.video_panel_min_s = self
            .min_s
            .trim()
            .parse::<u32>()
            .unwrap_or(0)
            .min(PANEL_MIN_S_MAX);
        s.video_panel_corner = mdm_ipc::capture::PANEL_CORNERS[self.corner].into();
        s.video_panel_sites = self.sites.split_whitespace().collect::<Vec<_>>().join(" ");
    }
}

/// The longest "Not over videos or sounds shorter than" (seconds): an hour.
pub(super) const PANEL_MIN_S_MAX: u32 = 3600;

/// Download panels → "Where it shows first": `mdm_ipc::capture::PANEL_CORNERS`' corners, in
/// its order.
pub(super) const CORNER_LABELS: [&str; 4] = [
    // i18n: Options → General → Download panels: the corner of the video the download button shows in first
    crate::i18n::mark("Top right"),
    // i18n: Options → General → Download panels: the corner of the video the download button shows in first
    crate::i18n::mark("Top left"),
    // i18n: Options → General → Download panels: the corner of the video the download button shows in first
    crate::i18n::mark("Bottom left"),
    // i18n: Options → General → Download panels: the corner of the video the download button shows in first
    crate::i18n::mark("Bottom right"),
];

/// General → Keys...: the two choices (indexes into `mdm_ipc::capture::KEY_CHOICES`) and the
/// files the browser would open.
pub(super) struct KeysEdit {
    pub(super) prevent: usize,
    pub(super) force: usize,
    pub(super) opened: bool,
}

impl KeysEdit {
    pub(super) fn from(s: &Settings) -> Self {
        let at = |saved: &str| {
            let choice = mdm_ipc::capture::key_choice(saved);
            mdm_ipc::capture::KEY_CHOICES
                .iter()
                .position(|c| *c == choice)
                .unwrap_or(0)
        };
        KeysEdit {
            prevent: at(&s.key_prevent),
            force: at(&s.key_force),
            opened: s.capture_opened,
        }
    }

    /// The two differ, or one of them is None.
    pub(super) fn valid(&self) -> bool {
        self.prevent != self.force || self.prevent == 0
    }

    pub(super) fn save(self, s: &mut Settings) {
        s.key_prevent = mdm_ipc::capture::KEY_CHOICES[self.prevent].into();
        s.key_force = mdm_ipc::capture::KEY_CHOICES[self.force].into();
        s.capture_opened = self.opened;
    }
}

/// Keys...: the combinations as its lists show them (`mdm_ipc::capture::KEY_CHOICES`' order).
pub(super) const KEY_LABELS: [&str; 8] = [
    // i18n: Options → General → Keys...: no key: the choice is off
    crate::i18n::mark("None"),
    // i18n: Options → General → Keys...: a key name; translate key names as the keyboard shows them (e.g. Strg for Ctrl)
    crate::i18n::mark("Alt"),
    // i18n: Options → General → Keys...: a key name; translate key names as the keyboard shows them (e.g. Strg for Ctrl)
    crate::i18n::mark("Ctrl"),
    // i18n: Options → General → Keys...: a key name; translate key names as the keyboard shows them (e.g. Strg for Ctrl)
    crate::i18n::mark("Shift"),
    // i18n: Options → General → Keys...: keys held together; translate key names as the keyboard shows them (e.g. Strg for Ctrl)
    crate::i18n::mark("Ctrl+Alt"),
    // i18n: Options → General → Keys...: keys held together; translate key names as the keyboard shows them (e.g. Strg for Ctrl)
    crate::i18n::mark("Alt+Shift"),
    // i18n: Options → General → Keys...: keys held together; translate key names as the keyboard shows them (e.g. Strg for Ctrl)
    crate::i18n::mark("Ctrl+Shift"),
    // i18n: Options → General → Keys...: keys held together; translate key names as the keyboard shows them (e.g. Strg for Ctrl)
    crate::i18n::mark("Ctrl+Alt+Shift"),
];

/// Customize DDM menu items...: the browsers' right-click items (`mdm_ipc::capture::MENU_ITEMS`'
/// order), named as the extension names them.
pub(super) const MENU_LABELS: [&str; 5] = [
    // i18n: Options → General → Customize DDM menu items: the browser's right-click command on a link, picture, video or sound (as the browser extension names it)
    crate::i18n::mark("Download with DDM"),
    // i18n: Options → General → Customize DDM menu items: the browser's right-click command on selected text (as the browser extension names it)
    crate::i18n::mark("Download selected links with DDM"),
    // i18n: Options → General → Customize DDM menu items: the browser's right-click command on a page (as the browser extension names it)
    crate::i18n::mark("Download all links with DDM"),
    // i18n: Options → General → Customize DDM menu items: the browser's right-click command on a page (as the browser extension names it)
    crate::i18n::mark("Download all images with DDM"),
    // i18n: Options → General → Customize DDM menu items: the browser's right-click command on a page (as the browser extension names it)
    crate::i18n::mark("Download all videos and audio with DDM"),
];

impl OptionsDialog {
    // ---- pages --------------------------------------------------------------------------

    pub(super) fn general(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        // Bold as every page's title, ending where IDM's (regular, from 260) ends.
        // i18n: Options → General: the page's title
        header(ui, o, Icon::PageGeneral, "Browsers and the system");
        let portable = mdm_ipc::portable_dir().is_some();
        let s = &mut self.s;
        if portable {
            s.launch_on_startup = false;
        }
        // IDM's "Extend the availability of Manifest 2 Chrome extensions" is not here: a Chrome
        // policy of IDM's time with no meaning for DDM's extension (Manifest 3).
        let checks: [(f32, &mut bool, &str); 6] = [
            (
                94.0,
                &mut s.launch_on_startup,
                // i18n: Options → General: checkbox; DDM starts when the computer starts (the user signs in)
                crate::i18n::mark("Start Dannex Download Manager with the computer"),
            ),
            (
                113.0,
                &mut s.clipboard_autostart,
                crate::i18n::mark("Offer to download links copied to the clipboard"),
            ),
            (
                133.0,
                &mut s.advanced_integration,
                // i18n: Options → General: checkbox; downloads started in web browsers are handed to DDM (off: the browsers keep them all)
                crate::i18n::mark("Take downloads from web browsers"),
            ),
            // DDM's: turns back on the delete confirmation that "Don't show this again"
            // turned off.
            (
                152.0,
                &mut s.confirm_delete,
                crate::i18n::mark("Confirm before deleting downloads"),
            ),
            // DDM's (#63): Minimize hides the main window and progress windows in the tray too.
            (
                172.0,
                &mut s.minimize_to_tray,
                // i18n: checkbox: the Minimize button hides DDM's windows to the notification area (system tray) instead of the taskbar
                crate::i18n::mark("Minimize to the notification area"),
            ),
            // DDM's: the × hides the window in the notification area, or quits. The browsers'
            // group below starts after it, its list scrolling.
            (
                191.0,
                &mut s.close_to_tray,
                // i18n: checkbox: closing keeps DDM running, in the notification area when available or else the taskbar, instead of quitting
                crate::i18n::mark("Keep running when closing the window"),
            ),
        ];
        // Greyed where no tray shows DDM's icon (some Linux desktops): nothing could bring the
        // window back from there.
        let tray = crate::tray::available();
        for (i, (y, v, label)) in checks.into_iter().enumerate() {
            // Portable: nothing is written to the registry, so no start with Windows.
            let enabled = !(portable && i == 0) && (i < 4 || tray);
            let check = winui::checkbox_light(ui, o + vec2(30.0, y), v, label, enabled);
            // Where a start with the session leaves the window (#226).
            if i == 0 && enabled && check.hovered() {
                check.on_hover_text(if crate::can_start_in_tray() {
                    // i18n: tooltip of "Start Dannex Download Manager with the computer": started with the computer, DDM's window stays hidden, its icon in the notification area (system tray)
                    crate::i18n::t("Started with the computer, it waits in the notification area")
                } else {
                    // i18n: the same tooltip where no notification area (system tray) shows DDM's icon (some Linux desktops): its window starts minimized instead
                    crate::i18n::t(
                        "Started with the computer, it starts minimized: no notification area shows its icon here",
                    )
                });
            } else if i == 5 && check.hovered() {
                // i18n: tooltip of the close-setting: checked keeps DDM running when the window closes, in the notification area when available or else the taskbar; downloads continue; unchecked closing quits
                check.on_hover_text(crate::i18n::t(
                    "Checked, closing the window keeps DDM running: it hides in the notification area when available, or minimizes to the taskbar otherwise, and downloads continue; unchecked, closing quits DDM",
                ));
            }
        }
        group(ui, o, 16.0, 215.0, 392.0, 179.0, BROWSERS_CAPTION);
        // Check list: one row per browser in the shared row list (Space ticks the selected
        // one; the box and its name tick it too).
        let browsers = &mut s.browsers;
        // Named by its group's caption, which says what the list is (#102).
        self.browser_rows.name = crate::i18n::t(BROWSERS_CAPTION);
        let ev = winui::row_list(
            ui,
            "browsers",
            r(o, 30.0, 225.0, 362.0, 97.0),
            browsers.len(),
            17.0,
            &mut self.browser_rows,
            |ui, clip, i, row, look| {
                let (name, on) = &mut browsers[i];
                look.name = name.clone();
                winui::row_check(
                    ui,
                    clip,
                    pos2(row.left() + 5.0, row.top() + 2.0),
                    on,
                    name,
                    ("browser", i),
                );
                // Its name in the row's ink (white on the selection).
                text(clip, row.left() + 23.0, row.top() + 12.0, name, look.ink);
            },
        );
        if let Some(i) = ev.space {
            browsers[i].1 = !browsers[i].1;
        }
        if ev.focused {
            self.browser_rows
                .type_to(ui, "browsers", browsers.len(), |i| browsers[i].0.clone());
        }
        if let Some(i) = ev.delete {
            remove_custom_browser(s, i);
        }
        // Add browser...: a browser of the user's, by its program (the native host knows it by
        // the program that started it: mdm_ipc::capture::CaptureRules::custom_name). With one of
        // those selected, the button takes it away again.
        let custom = self
            .browser_rows
            .selected
            .filter(|i| is_custom_browser(&self.s, *i));
        let add_label = if custom.is_some() {
            // i18n: Options → General: button that takes the selected browser the user added off the list
            crate::i18n::mark("Remove browser")
        } else {
            // i18n: Options → General: button that adds a browser (its program) to the browsers DDM takes downloads from
            crate::i18n::mark("Add browser...")
        };
        // Both as wide as their labels in this language (#192), within the group box.
        let [test_at, add_at] = winui::button_row(
            &p,
            &[
                ("Test browser integration...", 30.0, 180.0),
                (add_label, 286.0, 106.0),
            ],
            8.0,
            30.0,
            260.0,
        )[..] else {
            unreachable!("two buttons")
        };
        if winui::button_light(
            ui,
            r(o, add_at.0, 328.0, add_at.1, 21.0),
            add_label,
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            match custom {
                Some(i) => remove_custom_browser(&mut self.s, i),
                None => {
                    // i18n: title of the file dialog of Options → General → Add browser...
                    let title = crate::i18n::t("Choose the browser's program").to_owned();
                    #[cfg(windows)]
                    let kind = crate::i18n::t("Programs").to_owned();
                    winui::start_pick(ui.ctx(), "options-add-browser", move || {
                        let dialog = rfd::FileDialog::new().set_title(title);
                        // Windows: its .exe, from where programs are installed.
                        #[cfg(windows)]
                        let dialog = dialog.add_filter(kind, &["exe"]).set_directory(
                            std::env::var_os("ProgramFiles").unwrap_or_else(|| "C:\\".into()),
                        );
                        // macOS: its .app, from Applications.
                        #[cfg(target_os = "macos")]
                        let dialog = dialog.set_directory("/Applications");
                        dialog.pick_file()
                    });
                }
            }
        }
        if let Some(program) = winui::picked(ui.ctx(), "options-add-browser") {
            self.sub = Some(Sub::AddBrowser(
                browser_name_of(&program),
                program.display().to_string(),
            ));
        }
        // DDM's: what stands between a browser and DDM, checked in order.
        if winui::button_light(
            ui,
            r(o, test_at.0, 328.0, test_at.1, 21.0),
            "Test browser integration...",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.sub = Some(Sub::Test(crate::extension::checks(&self.s)));
        }
        winui::etched_h(&p, o.x + 34.0, o.y + 356.0, 357.0);
        // The buttons at the right as wide as their labels in this language (#192), their
        // captions ending 6 px before them.
        let fit_right =
            |label: &str| winui::buttons_to_right(&p, 392.0, 0.0, &[(label, 73.0)], 160.0)[0];
        let keys_at = fit_right("Keys...");
        // i18n: Options → General: label before Keys...: the keys held while clicking a link to leave it to the browser, or to have DDM download it
        let label = crate::i18n::t("Keys that leave a link to the browser, or give it to DDM");
        // Inside the browsers' group box: it starts no further out than the group's frame.
        let (rect, cut) = winui::text_right_within(
            &p,
            o.x + keys_at.0 - 6.0,
            o.y + 379.0,
            label,
            LIGHT_TEXT(),
            o.x + 22.0,
        );
        if cut {
            winui::full_on_hover(ui, rect, ("general-right", 379), label);
        }
        if winui::button_light(
            ui,
            r(o, keys_at.0, 365.0, keys_at.1, 21.0),
            "Keys...",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.sub = Some(Sub::Keys(KeysEdit::from(&self.s)));
        }
        // The browsers' right-click items (the extension builds its menu from them, Sub::Menu).
        // i18n: Options → General: label before Edit...: which of DDM's commands the browsers' right-click menu shows
        let label = crate::i18n::t("DDM's commands in the browsers' right-click menu");
        let menu_at = fit_right("Edit...");
        let (rect, cut) =
            winui::text_right(&p, o.x + menu_at.0 - 6.0, o.y + 419.0, label, LIGHT_TEXT());
        if cut {
            winui::full_on_hover(ui, rect, ("general-right", 419), label);
        }
        if winui::button_light(
            ui,
            r(o, menu_at.0, 406.0, menu_at.1, 21.0),
            // i18n: verb: edit the item
            "Edit...",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.sub = Some(Sub::Menu(menu_shown(&self.s)));
        }
        // The download button over videos in the browsers (our own panel, Sub::Panels).
        // i18n: Options → General: label before Edit...: DDM's download button over videos and sound players in web pages
        let label = crate::i18n::t("DDM's download button over videos in browsers");
        let panels_at = fit_right("Edit...");
        let (rect, cut) = winui::text_right(
            &p,
            o.x + panels_at.0 - 6.0,
            o.y + 452.0,
            label,
            LIGHT_TEXT(),
        );
        if cut {
            winui::full_on_hover(ui, rect, ("general-right", 452), label);
        }
        if winui::button_light(
            ui,
            r(o, panels_at.0, 438.0, panels_at.1, 21.0),
            // i18n: verb: edit the item
            "Edit...",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.sub = Some(Sub::Panels(Box::new(PanelsEdit::from(&self.s))));
        }
    }
}

/// Whether the browsers list's row `i` is one the user added (Add browser...).
pub(super) fn is_custom_browser(s: &Settings, i: usize) -> bool {
    s.browsers
        .get(i)
        .is_some_and(|(name, _)| s.custom_browsers.iter().any(|(n, _)| n == name))
}

/// Take the browsers list's row `i` off, when it is one the user added (the others stay).
pub(super) fn remove_custom_browser(s: &mut Settings, i: usize) {
    if is_custom_browser(s, i) {
        let (name, _) = s.browsers.remove(i);
        s.custom_browsers.retain(|(n, _)| *n != name);
    }
}

/// The name a browser's program suggests: what it says it is (Windows: its file description),
/// else its file's name ("vivaldi.exe" → "Vivaldi", "Arc.app" → "Arc").
pub(super) fn browser_name_of(program: &std::path::Path) -> String {
    #[cfg(windows)]
    if let Some(name) = crate::win32::program_description(program) {
        return name;
    }
    let stem = program
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    stem.split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Keys...'s lists, as this system names the keys: translated; macOS's own symbols there, as
/// the shortcuts list writes them (chrome::key_label), where the extension reads Ctrl as
/// Command (a Control-click is the secondary click on a Mac).
pub(super) fn key_labels() -> Vec<String> {
    KEY_LABELS
        .iter()
        .enumerate()
        .map(|(i, label)| {
            if cfg!(target_os = "macos") && i > 0 {
                label
                    .replace("Ctrl", "\u{2318}")
                    .replace("Alt", "\u{2325}")
                    .replace("Shift", "\u{21e7}")
                    .replace('+', "")
            } else {
                crate::i18n::t(label).to_owned()
            }
        })
        .collect()
}

/// The right-click items shown, from the settings (`MENU_LABELS`' order).
pub(super) fn menu_shown(s: &Settings) -> [bool; 5] {
    let mut shown = [true; 5];
    for (i, item) in mdm_ipc::capture::MENU_ITEMS.iter().enumerate() {
        shown[i] = !s.browser_menu_hidden.iter().any(|h| h == item);
    }
    shown
}

/// General → Download panels → Edit... (beyond IDM's): the button over videos on or off, over
/// videos and over sound players, not on media shorter than so many seconds, the corner it shows
/// in first, and the sites it never shows on (the button's "Never on <site>" adds to them), as a
/// file too (#424).
pub(super) fn panels_dialog(ui: &mut Ui, o: Pos2, e: &mut PanelsEdit) -> Outcome<()> {
    let p = ui.painter().clone();
    winui::checkbox_light(
        ui,
        o + vec2(14.0, 16.0),
        &mut e.on,
        "Show a download button over videos in browsers",
        true,
    );
    let on = e.on;
    let ink = if on { LIGHT_TEXT() } else { LIGHT_DISABLED() };
    winui::checkbox_light(
        ui,
        o + vec2(32.0, 40.0),
        &mut e.video,
        // i18n: Options → General → Download panels: the button shows over videos (<video> players)
        "Over videos",
        on,
    );
    winui::checkbox_light(
        ui,
        o + vec2(32.0, 60.0),
        &mut e.audio,
        // i18n: Options → General → Download panels: the button shows over a page's sound players (with controls)
        "Over sound players",
        on,
    );
    // A label at its pen, giving way to the box after it (shown whole on hover when cut).
    let label = |ui: &mut Ui, y: f32, s: &str, id: &str| {
        let (rect, cut) =
            winui::label_into(&p, o.x + 32.0, o.y + y, s, ink, o.x + 270.0, o.x + 14.0);
        if cut {
            winui::full_on_hover(ui, rect, ("opt-panels-label", id), s);
        }
    };
    label(
        ui,
        98.0,
        // i18n: Options → General → Download panels: before a number of seconds: the button doesn't show over a video or sound shorter than that (0: any length)
        crate::i18n::t("Not on media shorter than (seconds):"),
        "min",
    );
    let min = winui::spin_box(
        ui,
        r(o, 270.0, 84.0, 60.0, 21.0),
        &mut e.min_s,
        0,
        u64::from(PANEL_MIN_S_MAX),
        on,
        "opt-panels-min",
    );
    if on {
        // i18n: tooltip of the number of seconds in Options → General → Download panels: 0 shows the button whatever the length; the browser knows a video's length once its player has loaded it (until then the button shows)
        min.on_hover_text(crate::i18n::t(
            "0: any length. A length is known once the player has loaded the video; until then the button shows.",
        ));
    }
    label(
        ui,
        124.0,
        // i18n: Options → General → Download panels: before the list of corners the button shows in first (dragged elsewhere on a site, it stays there on that site)
        crate::i18n::t("Where it shows first:"),
        "corner",
    );
    if let Some(i) = winui::combo_light(
        ui,
        r(o, 270.0, 110.0, 116.0, 21.0),
        &CORNER_LABELS,
        e.corner,
        on,
        "opt-panels-corner",
    ) {
        e.corner = i;
    }
    winui::text_in_window(
        ui,
        &p,
        o.x + 14.0,
        o.y + 156.0,
        crate::i18n::t("Never show it on these sites:"),
        ink,
    );
    multiline(
        ui,
        r(o, 14.0, 163.0, 372.0, 55.0),
        &mut e.sites,
        "panel-sites",
        on,
    );
    winui::text_in_window(
        ui,
        &p,
        o.x + 14.0,
        o.y + 234.0,
        crate::i18n::t("(names separated by spaces)"),
        LIGHT_TEXT(),
    );
    // The list as a file, to keep or carry to a new installation (#424).
    let [export_at, import_at] = winui::button_row(
        &p,
        &[("Export...", 232.0, 73.0), ("Import...", 313.0, 73.0)],
        8.0,
        180.0,
        140.0,
    )[..] else {
        unreachable!("two buttons")
    };
    if winui::button_light(
        ui,
        r(o, export_at.0, 229.0, export_at.1, 21.0),
        "Export...",
        ButtonKind::Normal,
        false,
    )
    .clicked()
    {
        let title = crate::i18n::t("Export").to_owned();
        winui::start_pick(ui.ctx(), "opt-panels-export", move || {
            rfd::FileDialog::new()
                .set_title(title)
                .add_filter("TXT", &["txt"])
                .set_file_name("sites.txt")
                .save_file()
        });
    }
    if winui::button_light(
        ui,
        r(o, import_at.0, 229.0, import_at.1, 21.0),
        "Import...",
        ButtonKind::Normal,
        false,
    )
    .clicked()
    {
        let title = crate::i18n::t("Import").to_owned();
        winui::start_pick(ui.ctx(), "opt-panels-import", move || {
            rfd::FileDialog::new()
                .set_title(title)
                .add_filter("TXT", &["txt"])
                .pick_file()
        });
    }
    if let Some(path) = winui::picked(ui.ctx(), "opt-panels-export") {
        e.said = match std::fs::write(&path, mdm_ipc::capture::sites_to_text(&e.sites)) {
            Ok(()) => crate::i18n::tn(
                e.sites.split_whitespace().count() as u64,
                "{n} site saved to the file.",
                "{n} sites saved to the file.",
            ),
            Err(err) => crate::i18n::fmt(
                crate::i18n::t("The list could not be saved: {error}"),
                &[("error", &err.to_string())],
            ),
        };
    }
    if let Some(path) = winui::picked(ui.ctx(), "opt-panels-import") {
        e.said = match read_sites_file(&path) {
            Ok(text) => {
                let (merged, added) = mdm_ipc::capture::merge_sites(&e.sites, &text);
                e.sites = merged;
                crate::i18n::tn(
                    added as u64,
                    "{n} site added from the file.",
                    "{n} sites added from the file.",
                )
            }
            Err(err) => crate::i18n::fmt(
                crate::i18n::t("The file could not be read: {error}"),
                &[("error", &err.to_string())],
            ),
        };
    }
    if !e.said.is_empty() {
        let (rect, cut) = winui::text_fit(
            &p,
            o.x + 14.0,
            o.y + 264.0,
            &e.said,
            LIGHT_TEXT(),
            o.x + 225.0,
        );
        if cut {
            winui::full_on_hover(ui, rect, "opt-panels-said", &e.said);
        }
    }
    let in_range = !on
        || e.min_s
            .trim()
            .parse::<u32>()
            .is_ok_and(|n| n <= PANEL_MIN_S_MAX);
    ok_cancel(ui, o, 232.0, 258.0, in_range)
}

/// General → Keys...: the keys that, held while clicking a link in a browser, leave it to the
/// browser, and those that have DDM take it whatever its type (the extension's page.js goes by
/// them); the two differ. And "Also take files the browser would open".
pub(super) fn keys_dialog(ui: &mut Ui, o: Pos2, e: &mut KeysEdit) -> Outcome<()> {
    let p = ui.painter().clone();
    let labels = key_labels();
    let shown: Vec<&str> = labels.iter().map(String::as_str).collect();
    // A sentence (one or two lines), then its list under it.
    let mut y = 22.0;
    let lines = winui::paragraph(
        &p,
        o.x + 14.0,
        o.y + y,
        // i18n: Options → General → Keys...: over the list of keys (Alt by default) that leave a clicked link to the browser
        crate::i18n::t(
            "Hold these keys while clicking a link to let the browser download it instead of Dannex Download Manager:",
        ),
        LIGHT_TEXT(),
        o.x + 426.0,
        14.0,
    );
    y += 14.0 * lines.saturating_sub(1) as f32 + 8.0;
    if let Some(i) = winui::combo_light(
        ui,
        r(o, 14.0, y, 160.0, 21.0),
        &shown,
        e.prevent,
        true,
        "opt-keys-prevent",
    ) {
        e.prevent = i;
    }
    y += 21.0 + 22.0;
    let lines = winui::paragraph(
        &p,
        o.x + 14.0,
        o.y + y,
        // i18n: Options → General → Keys...: over the list of keys (Ctrl+Alt by default) that have DDM download a clicked link whatever its type, as the browser's right-click command "Download with DDM" does
        crate::i18n::t(
            "Hold these keys while clicking a link to have Dannex Download Manager download it, whatever its type:",
        ),
        LIGHT_TEXT(),
        o.x + 426.0,
        14.0,
    );
    y += 14.0 * lines.saturating_sub(1) as f32 + 8.0;
    if let Some(i) = winui::combo_light(
        ui,
        r(o, 14.0, y, 160.0, 21.0),
        &shown,
        e.force,
        true,
        "opt-keys-force",
    ) {
        e.force = i;
    }
    if !e.valid() {
        winui::text_in_window(
            ui,
            &p,
            o.x + 182.0,
            o.y + y + 15.0,
            // i18n: Options → General → Keys...: the same keys were chosen for both lists
            crate::i18n::t("Choose different keys for the two."),
            winui::ERROR(),
        );
    }
    winui::checkbox_light(
        ui,
        o + vec2(14.0, y + 40.0),
        &mut e.opened,
        "Also take files the browser would open (PDF, music, video)",
        true,
    );
    ok_cancel(ui, o, 272.0, 213.0, e.valid())
}

/// General → Customize DDM menu items → Edit...: which commands the browsers' right-click menu
/// shows; the extension builds its menu again when it next asks DDM (its rules).
pub(super) fn menu_dialog(ui: &mut Ui, o: Pos2, shown: &mut [bool; 5]) -> Outcome<()> {
    let p = ui.painter().clone();
    winui::text_in_window(
        ui,
        &p,
        o.x + 14.0,
        o.y + 22.0,
        // i18n: Options → General → Customize DDM menu items: over the commands to tick
        crate::i18n::t("Show these commands in the browsers' right-click menu:"),
        LIGHT_TEXT(),
    );
    for (i, (on, label)) in shown.iter_mut().zip(MENU_LABELS).enumerate() {
        winui::checkbox_light(ui, o + vec2(24.0, 34.0 + 20.0 * i as f32), on, label, true);
    }
    winui::paragraph(
        &p,
        o.x + 14.0,
        o.y + 150.0,
        // i18n: Options → General → Customize DDM menu items: when the browsers' menus change
        crate::i18n::t("A browser changes its menu when you next switch to it."),
        LIGHT_TEXT(),
        o.x + 386.0,
        14.0,
    );
    ok_cancel(ui, o, 232.0, 182.0, true)
}
