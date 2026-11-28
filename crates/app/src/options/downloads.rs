//! Options → Downloads: Download File Info and the progress window (its tabs), duplicates, the
//! User-Agent of manually added downloads, and virus checking.

use super::*;

/// Downloads → "When an address is already in the list:" (`settings.duplicate_action`).
pub const DUPLICATE: [&str; 4] = [
    // i18n: Options → Downloads: what DDM does when an address already in the list is added again: ask each time
    crate::i18n::mark("Ask what to do"),
    // i18n: Options → Downloads: an address already in the list added again: added as a new download whose file gets a number ("file (1).zip")
    crate::i18n::mark("Add it again, with a numbered file name"),
    // i18n: Options → Downloads: an address already in the list added again: added as a new download that replaces the file
    crate::i18n::mark("Add it again, replacing the file"),
    // i18n: Options → Downloads: an address already in the list added again: its finished file is shown (Download complete), an unfinished one resumed
    crate::i18n::mark("Show the finished file, or resume the unfinished one"),
];

/// Downloads → Virus checking settings → Edit...: a copy of the settings, kept on OK.
pub(super) struct VirusEdit {
    pub(super) on: bool,
    pub(super) program: String,
    pub(super) args: String,
    pub(super) types: String,
    /// The antivirus programs found here, offered with one click (looked for as it opens).
    pub(super) presets: Vec<crate::viruscheck::Preset>,
}

impl VirusEdit {
    pub(super) fn from(s: &Settings) -> Self {
        VirusEdit {
            on: s.virus_check,
            program: s.virus_program.clone(),
            args: s.virus_args.clone(),
            types: s.virus_types.clone(),
            presets: crate::viruscheck::presets(),
        }
    }

    pub(super) fn save(self, s: &mut Settings) {
        s.virus_check = self.on && !self.program.trim().is_empty();
        s.virus_program = self.program.trim().to_owned();
        s.virus_args = self.args.trim().to_owned();
        s.virus_types = self.types.trim().to_owned();
    }
}

/// Downloads → Customize "Download progress" dialog → Edit...: a copy of its settings, kept on
/// OK.
#[derive(Clone, Copy)]
pub(super) struct ProgressEdit {
    pub(super) view: crate::progress::ProgressView,
    pub(super) limiter: bool,
    pub(super) completion: bool,
}

impl ProgressEdit {
    pub(super) fn from(s: &Settings) -> Self {
        ProgressEdit {
            view: s.progress_view,
            limiter: s.progress_tab_limiter,
            completion: s.progress_tab_completion,
        }
    }

    pub(super) fn save(self, s: &mut Settings) {
        s.progress_view = self.view;
        s.progress_tab_limiter = self.limiter;
        s.progress_tab_completion = self.completion;
    }
}

impl OptionsDialog {
    pub(super) fn downloads(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        // i18n: Options → Downloads: the page's title
        header(ui, o, Icon::PageDownloads, "How downloads are added");
        // Edit... as wide as its label in this language (#192), its caption before it.
        let (edit_x, edit_w) =
            winui::buttons_to_right(&p, 392.0, 0.0, &[("Edit...", 73.0)], 160.0)[0];
        winui::text_right(
            &p,
            o.x + edit_x - 13.0,
            o.y + 104.0,
            // i18n: Options → Downloads: label before Edit...: how a download's progress window opens, and its tabs
            crate::i18n::t("How the progress window opens"),
            LIGHT_TEXT(),
        );
        // How a download's progress window opens: shown, minimized or hidden (Sub::Progress).
        if winui::button_light(
            ui,
            r(o, edit_x, 89.0, edit_w, 21.0),
            // i18n: verb: edit the item
            "Edit...",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.sub = Some(Sub::Progress(ProgressEdit::from(&self.s)));
        }
        let s = &mut self.s;
        // The page's content ends with its rules, at 402: the check boxes' captions end there
        // ("...", whole on hover), not at the page's edge, and the note under the first three
        // wraps there (a second line takes the room above the rule and pushes the first check
        // box under it down), each line cut with "..." past two.
        let page_right = 402.0;
        let until = Some(o.x + page_right + 5.0);
        winui::checkbox_light_until(
            ui,
            o + vec2(24.0, 120.0),
            &mut s.show_start_dialog,
            // i18n: Options → Downloads: checkbox; the Download File Info window asks first, for each download added
            "Show Download File Info first",
            true,
            until,
        );
        let on = s.show_start_dialog;
        winui::checkbox_light_until(
            ui,
            o + vec2(50.0, 141.0),
            &mut s.only_add_to_queue,
            // i18n: Options → Downloads: checkbox under "Show Download File Info first", while that is off: new downloads go to the queue without starting
            "Otherwise, add them to the queue without starting",
            !on,
            until,
        );
        winui::checkbox_light_until(
            ui,
            o + vec2(24.0, 160.0),
            &mut s.show_complete,
            crate::progress::SHOW_COMPLETE,
            true,
            until,
        );
        // i18n: Options → Downloads: a note under the checkboxes for Download File Info and Download complete: a queue's downloads never show those windows
        let note = crate::i18n::t("A queue's downloads show neither window.");
        let mut lines = winui::fit_lines(&p, winui::tahoma(), note, page_right - 31.0);
        lines.truncate(2);
        let two = (lines.len() - 1) as f32;
        for (k, line) in lines.iter().enumerate() {
            let (rect, cut) = winui::text_fit(
                &p,
                o.x + 31.0,
                o.y + 191.0 - 6.0 * two + 13.0 * k as f32,
                line,
                LIGHT_TEXT(),
                o.x + page_right,
            );
            if cut {
                winui::full_on_hover(ui, rect, ("opt-queue-note", k), note);
            }
        }
        winui::etched_h(&p, o.x + 21.0, o.y + 198.0 + 7.0 * two, 381.0);
        winui::checkbox_light_until(
            ui,
            o + vec2(24.0, 212.0 + 4.0 * two),
            &mut s.start_immediately,
            // i18n: Options → Downloads: checkbox; the file starts coming in while Download File Info waits for an answer
            "Start fetching while Download File Info is open",
            true,
            until,
        );
        winui::checkbox_light_until(
            ui,
            o + vec2(24.0, 241.0),
            &mut s.queue_panel_later,
            // i18n: Options → Downloads: checkbox; Download Later asks which queue the download goes to
            "Ask for the queue on Download Later",
            true,
            until,
        );
        winui::checkbox_light_until(
            ui,
            o + vec2(24.0, 266.0),
            &mut s.queue_panel_batch,
            // i18n: Options → Downloads: checkbox; after Batch download (and lists of links), DDM asks which queue they go to
            "Ask for the queue after a batch download",
            true,
            until,
        );
        winui::checkbox_light_until(
            ui,
            o + vec2(24.0, 290.0),
            &mut s.ignore_mtime,
            // i18n: Options → Downloads: checkbox; a download resumes even though the server says its file's date changed
            "Resume even if the file's date on the server changed",
            true,
            until,
        );
        winui::etched_h(&p, o.x + 21.0, o.y + 314.0, 381.0);
        text(
            &p,
            o.x + 21.0,
            o.y + 334.0,
            // i18n: Options → Downloads: over the drop-down choosing what happens when an address already in the list is added again
            crate::i18n::t("When an address is already in the list:"),
            LIGHT_TEXT(),
        );
        if let Some(i) = winui::combo_light(
            ui,
            r(o, 21.0, 340.0, 380.0, 21.0),
            &DUPLICATE,
            s.duplicate_action as usize,
            true,
            "opt-duplicate",
        ) {
            s.duplicate_action = i as u8;
        }
        winui::etched_h(&p, o.x + 21.0, o.y + 367.0, 381.0);
        self.user_agent(ui, o);
        winui::etched_h(&p, o.x + 21.0, o.y + 426.0, 381.0);
        // The antivirus program finished downloads go through (viruscheck.rs).
        winui::text_right(
            &p,
            o.x + edit_x - 17.0,
            o.y + 451.0,
            // i18n: Options → Downloads: label before Edit...: checking finished downloads with an antivirus program
            crate::i18n::t("Virus checking"),
            LIGHT_TEXT(),
        );
        if winui::button_light(
            ui,
            r(o, edit_x, 436.0, edit_w, 21.0),
            // i18n: verb: edit the item
            "Edit...",
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            self.sub = Some(Sub::Virus(VirusEdit::from(&self.s)));
        }
    }

    /// "User-Agent for manually added downloads" (#668): Automatic, the default browser's (the
    /// box shows it, read-only), or the one typed in the box.
    pub(super) fn user_agent(&mut self, ui: &mut Ui, o: Pos2) {
        let p = ui.painter().clone();
        let s = &mut self.s;
        // The checkbox ends where the box does; the label before it gives way ("...") if need be.
        let auto_x = 402.0 - (winui::checkbox_caption_end(&p, 0.0, "Automatic") + 2.0);
        // i18n: Options → Downloads: over the User-Agent (the browser's name and version servers are told) sent for downloads added by hand
        let label = crate::i18n::t("User-Agent for downloads you add yourself:");
        let (rect, cut) = winui::label_into(
            &p,
            o.x + 21.0,
            o.y + 388.0,
            label,
            LIGHT_TEXT(),
            o.x + auto_x - 6.0,
            o.x + 21.0,
        );
        if cut {
            winui::full_on_hover(ui, rect, "opt-user-agent", label);
        }
        let agent = mdm_engine::agent::automatic();
        let tip = match mdm_engine::agent::automatic_browser() {
            Some(browser) => crate::i18n::fmt(
                // i18n: tooltip; {browser} is a browser and its version, e.g. "Brave 157"
                crate::i18n::t("Your default browser's User-Agent: {browser}"),
                &[("browser", &browser)],
            ),
            None if mdm_engine::agent::looked_for() => crate::i18n::t(
                "Your default browser isn't one DDM knows: a current Chrome's User-Agent",
            )
            .to_owned(),
            None => crate::i18n::t("Looking for your default browser...").to_owned(),
        };
        let tip = format!("{tip}\n{agent}");
        let was = self.ua_auto;
        winui::checkbox_light(
            ui,
            o + vec2(auto_x, 378.0),
            &mut self.ua_auto,
            // i18n: checkbox: the User-Agent is the default browser's, which DDM finds itself
            "Automatic",
            true,
        )
        .on_hover_text(&tip);
        if self.ua_auto != was {
            // Ticked: the automatic one. Unticked: the box starts from it, to be changed.
            s.user_agent = if self.ua_auto {
                String::new()
            } else {
                agent.clone()
            };
        }
        let rc = r(o, 21.0, 398.0, 381.0, 23.0);
        if self.ua_auto {
            let mut shown = agent;
            winui::edit_light(ui, rc, &mut shown, false, false).on_hover_text(&tip);
        } else {
            winui::edit_light(ui, rc, &mut s.user_agent, true, false);
        }
    }
}

/// Downloads → Virus checking settings → Edit...: on or off, the antivirus program and its
/// arguments (`[FILE]` the file), the file types it checks, and the presets found here.
pub(super) fn virus_dialog(ui: &mut Ui, o: Pos2, e: &mut VirusEdit) -> Outcome<()> {
    let p = ui.painter().clone();
    winui::checkbox_light(
        ui,
        o + vec2(14.0, 12.0),
        &mut e.on,
        // i18n: Options → Virus checking: check box: run the antivirus program on each finished download of the types listed
        "Check downloaded files for viruses",
        true,
    );
    text(
        &p,
        o.x + 14.0,
        o.y + 52.0,
        // i18n: Options → Virus checking: label before the antivirus program's path
        crate::i18n::t("Antivirus program:"),
        LIGHT_TEXT(),
    );
    let (browse_x, browse_w) =
        winui::buttons_to_right(&p, 456.0, 0.0, &[("Browse...", 84.0)], 160.0)[0];
    winui::edit_light(
        ui,
        r(o, 14.0, 58.0, browse_x - 8.0 - 14.0, 23.0),
        &mut e.program,
        true,
        false,
    );
    if winui::button_light(
        ui,
        r(o, browse_x, 59.0, browse_w, 21.0),
        "Browse...",
        ButtonKind::Normal,
        false,
    )
    .clicked()
    {
        let title = crate::i18n::t("Antivirus program:")
            .trim_end_matches(':')
            .to_owned();
        winui::start_pick(ui.ctx(), "options-virus-program", move || {
            rfd::FileDialog::new().set_title(title).pick_file()
        });
    }
    if let Some(f) = winui::picked(ui.ctx(), "options-virus-program") {
        e.program = f.display().to_string();
    }
    text(
        &p,
        o.x + 14.0,
        o.y + 100.0,
        // i18n: Options → Virus checking: label before the antivirus program's arguments; [FILE] is written as is (it stands for the downloaded file)
        crate::i18n::t("Arguments ([FILE] is the downloaded file):"),
        LIGHT_TEXT(),
    );
    winui::edit_light(ui, r(o, 14.0, 106.0, 442.0, 23.0), &mut e.args, true, false);
    text(
        &p,
        o.x + 14.0,
        o.y + 148.0,
        crate::i18n::t("File types:"),
        LIGHT_TEXT(),
    );
    winui::edit_light(
        ui,
        r(o, 14.0, 154.0, 442.0, 23.0),
        &mut e.types,
        true,
        false,
    );
    // The presets found here (Microsoft Defender on Windows, ClamAV on Linux and macOS): one
    // click fills the program and its arguments.
    let mut x = 14.0;
    for preset in &e.presets {
        let label = crate::i18n::fmt(
            // i18n: Options → Virus checking: button that fills in an antivirus program found on this computer; {name} is its name ("Microsoft Defender")
            crate::i18n::t("Use {name}"),
            &[("name", preset.name)],
        );
        let w = winui::button_fit_width(&p, &label, 120.0, 220.0);
        if winui::button_light(
            ui,
            r(o, x, 192.0, w, 21.0),
            &label,
            ButtonKind::Normal,
            false,
        )
        .clicked()
        {
            e.program = preset.program.display().to_string();
            e.args = preset.args.to_owned();
            e.on = true;
        }
        x += w + 8.0;
    }
    let types_ok = !e.on || !e.types.trim().is_empty();
    ok_cancel(
        ui,
        o,
        304.0,
        230.0,
        (!e.on || !e.program.trim().is_empty()) && types_ok,
    )
}

/// Downloads → Customize "Download progress" dialog → Edit... (IDM's place for it): how a
/// download's progress window opens when it is started or resumed, and which of its optional
/// tabs show. A queue's start theirs hidden whatever is chosen, so a queue never fills the
/// screen.
pub(super) fn progress_dialog(ui: &mut Ui, o: Pos2, e: &mut ProgressEdit) -> Outcome<()> {
    use crate::progress::ProgressView;
    let p = ui.painter().clone();
    winui::text_in_window(
        ui,
        &p,
        o.x + 14.0,
        o.y + 22.0,
        // i18n: Options → Downloads → Customize "Download progress" dialog: over three choices of how a download's progress window opens
        crate::i18n::t("When a download starts or resumes, open its progress window:"),
        LIGHT_TEXT(),
    );
    let choices = [
        // i18n: how a download's progress window opens: on the screen, as usual (the default)
        (ProgressView::Normal, crate::i18n::mark("Normally")),
        // i18n: how a download's progress window opens: minimized to the taskbar
        (ProgressView::Minimized, crate::i18n::mark("Minimized")),
        // i18n: how a download's progress window opens: hidden in the notification area (system tray), where the tray icon's menu shows it
        (
            ProgressView::Hidden,
            crate::i18n::mark("Hidden in the notification area"),
        ),
    ];
    let mut group = Vec::with_capacity(choices.len());
    for (i, (v, label)) in choices.iter().enumerate() {
        let resp = winui::radio_light_until(
            ui,
            o + vec2(24.0, 36.0 + 20.0 * i as f32),
            e.view == *v,
            label,
            true,
            Some(o.x + 366.0),
        );
        if resp.clicked() {
            e.view = *v;
        }
        group.push(resp);
    }
    if let Some(i) = winui::radio_keys(ui, &group) {
        e.view = choices[i].0;
    }
    winui::paragraph(
        &p,
        o.x + 14.0,
        o.y + 112.0,
        // i18n: Options → Downloads → Customize "Download progress" dialog: a note under the choices
        crate::i18n::t(
            "A queue's downloads start with theirs hidden. Show progress in a download's right-click menu, or the tray icon's Downloads in progress, opens one.",
        ),
        LIGHT_TEXT(),
        o.x + 366.0,
        14.0,
    );
    winui::etched_h(&p, o.x + 14.0, o.y + 168.0, 352.0);
    winui::text_in_window(
        ui,
        &p,
        o.x + 14.0,
        o.y + 188.0,
        // i18n: Options → Downloads → Customize "Download progress" dialog: over the check boxes of the progress window's optional tabs
        crate::i18n::t("Tabs the window shows:"),
        LIGHT_TEXT(),
    );
    // The tabs by their own titles (Download status always shows).
    winui::checkbox_light_until(
        ui,
        o + vec2(24.0, 198.0),
        &mut e.limiter,
        "Speed Limiter",
        true,
        Some(o.x + 366.0),
    );
    winui::checkbox_light_until(
        ui,
        o + vec2(24.0, 218.0),
        &mut e.completion,
        "When done",
        true,
        Some(o.x + 366.0),
    );
    ok_cancel(ui, o, 212.0, 248.0, true)
}
