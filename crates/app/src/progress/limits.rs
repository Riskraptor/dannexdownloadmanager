//! The Speed Limiter and Options on completion tabs: the download's own limits, and what happens
//! when it is done.

use super::*;

impl Progress {
    /// The Speed Limiter tab, live for this download. Returns true when "Hide tab" is pressed.
    pub(super) fn limiter_page(&mut self, ui: &mut Ui, o: Pos2, item: &Item) -> bool {
        let p = ui.painter().clone();
        if !self.limit_init {
            if let Some(rate) = item.torrent.as_ref().and_then(|t| t.upload_limit) {
                self.up_on = true;
                self.up_kb = (rate / 1024).max(1).to_string();
            }
            if let Some(rate) = item.live.as_ref().and_then(|l| l.dl.speed_limit()) {
                self.limiter = true;
                self.limit_kb = (rate / 1024).max(1).to_string();
            }
            // A remembered limit: the box is ticked (and the limit shows even while stopped).
            if let Some(rate) = item.own_limit {
                self.limiter = true;
                self.limit_kb = (rate / 1024).max(1).to_string();
                self.remember = true;
            }
            self.limit_init = true;
        }
        text(
            &p,
            o.x + 33.0,
            o.y + 51.0,
            crate::i18n::t("Transfer rate"),
            winui::TEXT(),
        );
        let rate = if item.status == Status::Downloading {
            model::rate(item.rate())
        } else {
            String::new()
        };
        text(&p, o.x + 136.0, o.y + 51.0, &rate, winui::TEXT());
        let was_on = self.limiter;
        // A torrent's upload limit takes the right half (x 290): the download side's captions
        // stop before it.
        let torrent = item.torrent.is_some();
        winui::checkbox_until(
            ui,
            o + vec2(33.0, 64.0),
            &mut self.limiter,
            // i18n: progress window's Speed Limiter tab: checkbox; this download's own speed limit
            "Limit this download's speed",
            true,
            torrent.then_some(286.0),
        );
        let label = if self.limiter {
            winui::TEXT()
        } else {
            disabled()
        };
        winui::fit(
            ui,
            &p,
            o.x + 33.0,
            o.y + 92.0,
            crate::i18n::t("Maximum download speed:"),
            label,
            o.x + if torrent { 284.0 } else { 525.0 },
            ("limit-label", self.id),
        );
        let rate_box = winui::edit_right(
            ui,
            r(o, 33.0, 100.0, 77.0, 23.0),
            &mut self.limit_kb,
            self.limiter,
        );
        self.limit_kb.retain(|c| c.is_ascii_digit());
        // A typed rate applies once typed (focus leaves, Enter), not at each digit (typing 100
        // would throttle to 1, then 10 KB/s on the way). A rate a screen reader or UI Automation
        // sets (Set value) applies at once: there is no typing to wait for, and often no focus
        // to leave, so it never applied.
        let typed = rate_box.lost_focus()
            || (rate_box.has_focus() && winui::enter_pressed(ui))
            || winui::value_set_by_assistive(&rate_box);
        // i18n: unit after an input box: kilobytes per second
        text(
            &p,
            o.x + 118.0,
            o.y + 113.0,
            crate::i18n::t("KBytes/sec"),
            label,
        );
        let remembered = self.remember;
        let remember_box = winui::checkbox(ui, o + vec2(33.0, 134.0), &mut self.remember, "", true);
        // Its caption is drawn apart (two lines, below): screen readers get it as its name.
        let name = format!(
            "{} {}",
            crate::i18n::t(REMEMBER_1),
            crate::i18n::t(REMEMBER_2)
        );
        winui::expose(
            &remember_box,
            egui::Role::CheckBox,
            &name,
            true,
            Some(self.remember),
        );
        // Its caption is two lines drawn below; a click on them ticks it too.
        winui::caption_click(
            ui,
            r(o, 51.0, 127.0, 450.0, 27.0),
            "remember-limit",
            &mut self.remember,
            true,
        );
        // An empty or zero rate isn't "unlimited" while the box is ticked: the limit stays
        // as it was until a rate is typed.
        let kb: u64 = self.limit_kb.parse().unwrap_or(0);
        if (was_on != self.limiter || (self.limiter && typed) || remembered != self.remember)
            && !(self.limiter && kb == 0)
        {
            let limit = self.limiter.then_some(kb * 1024);
            self.pending_limit = Some((limit, self.remember));
        }
        text(
            &p,
            o.x + 51.0,
            o.y + 137.0,
            crate::i18n::t(REMEMBER_1),
            winui::TEXT(),
        );
        text(
            &p,
            o.x + 51.0,
            o.y + 150.0,
            crate::i18n::t(REMEMBER_2),
            winui::TEXT(),
        );
        if item.torrent.is_some() {
            self.upload_limit(ui, &p, o);
        }
        hide_tab(ui, o)
    }

    /// A torrent's own upload limit, right of the download one (#39): kept with the torrent,
    /// so it applies again whenever it runs or shares.
    pub(super) fn upload_limit(&mut self, ui: &mut Ui, p: &egui::Painter, o: Pos2) {
        let was_on = self.up_on;
        winui::checkbox_until(
            ui,
            o + vec2(290.0, 64.0),
            &mut self.up_on,
            // i18n: a torrent's progress window, Speed Limiter tab: a limit for what this torrent sends to others
            "Limit uploading",
            true,
            Some(525.0),
        );
        let label = if self.up_on {
            winui::TEXT()
        } else {
            disabled()
        };
        winui::fit(
            ui,
            p,
            o.x + 290.0,
            o.y + 92.0,
            // i18n: a torrent's progress window, Speed Limiter tab: label above the upload speed box
            crate::i18n::t("Maximum upload speed:"),
            label,
            o.x + 525.0,
            ("upload-label", self.id),
        );
        let rate_box = winui::edit_right(
            ui,
            r(o, 290.0, 100.0, 77.0, 23.0),
            &mut self.up_kb,
            self.up_on,
        );
        self.up_kb.retain(|c| c.is_ascii_digit());
        let typed = rate_box.lost_focus()
            || (rate_box.has_focus() && winui::enter_pressed(ui))
            || winui::value_set_by_assistive(&rate_box);
        text(
            p,
            o.x + 375.0,
            o.y + 113.0,
            crate::i18n::t("KBytes/sec"),
            label,
        );
        let kb: u64 = self.up_kb.parse().unwrap_or(0);
        if (was_on != self.up_on || (self.up_on && typed)) && !(self.up_on && kb == 0) {
            self.pending_upload = Some(self.up_on.then_some(kb * 1024));
        }
    }

    /// When done → "Show download complete dialog" for this file, once ticked or
    /// unticked here, or with something else to do when done chosen here (None: Options' setting,
    /// as it is when the file finishes).
    pub fn shows_complete(&self) -> Option<bool> {
        let acts = self.hang_up || self.exit || self.turn_off_on;
        (self.complete_chosen || acts).then_some(self.show_complete)
    }

    /// When done's other settings, when they apply (the complete dialog is off).
    pub fn when_done(&self) -> Option<crate::queue::WhenDone> {
        (!self.show_complete).then(|| crate::queue::WhenDone {
            hang_up: self.hang_up,
            exit: self.exit,
            turn_off: self
                .turn_off_on
                .then(|| crate::queue::TurnOff::ALL[self.turn_off]),
            force: self.turn_off_on && self.force,
        })
    }

    /// Returns true when "Hide tab" is pressed.
    pub(super) fn completion_page(&mut self, ui: &mut Ui, o: Pos2, item: &Item) -> bool {
        let p = ui.painter().clone();
        // The label as wide as its language makes it (half the row at most), then the path to
        // the row's end, shortened in the middle so the file's name stays in sight (#62).
        let (left, right) = (o.x + 33.0, o.x + 33.0 + 474.0);
        let label = winui::fit(
            ui,
            &p,
            left,
            o.y + 45.0,
            crate::i18n::t("Save To:"),
            winui::TEXT(),
            left + 237.0,
            "save-to-label",
        );
        // From the label's width: its rect is mirrored in a right-to-left interface.
        let at = (left + label.width() + 6.0).max(o.x + 90.0);
        crate::fileicons::paint_at(
            ui,
            &p,
            crate::fileicons::Kind::File(&item.file_name),
            egui::pos2(at, o.y + 33.0),
            16.0,
        );
        winui::fit_middle(
            ui,
            &p,
            at + crate::fileicons::INDENT,
            o.y + 45.0,
            &display_path(&item.dest),
            winui::TEXT(),
            right,
            "save-to-path",
        );
        let was = self.show_complete;
        winui::checkbox(
            ui,
            o + vec2(33.0, 55.0),
            &mut self.show_complete,
            SHOW_COMPLETE,
            true,
        );
        self.complete_chosen |= self.show_complete != was;
        winui::etched_h(&p, o.x + 33.0, o.y + 74.0, 474.0);
        // Within the page (a longer translation ends in "...", whole on hover).
        winui::fit(
            ui,
            &p,
            o.x + 33.0,
            o.y + 89.0,
            // i18n: progress window's When done tab: a note above the checkboxes it greys while "Show the Download complete window" is ticked
            crate::i18n::t("These apply only while \"Show the Download complete window\" is off"),
            winui::TEXT(),
            o.x + 507.0,
            ("when-done-note", self.id),
        );
        // Used when the complete dialog is off (it asks what to do itself).
        let on = !self.show_complete;
        // Dial-up is hung up through Windows' rasdial; macOS and Linux have no such command.
        let hang_up = winui::checkbox(
            ui,
            o + vec2(33.0, 96.0),
            &mut self.hang_up,
            HANG_UP,
            on && cfg!(windows),
        );
        winui::why_disabled(hang_up, on && !cfg!(windows), NO_DIAL_UP);
        winui::checkbox(
            ui,
            o + vec2(33.0, 114.0),
            &mut self.exit,
            EXIT_WHEN_DONE,
            on,
        );
        // The drop-down after the caption, however long it is in this language, as wide as its
        // choices need, its right end at the row's end at the most (the caption is cut there).
        let combo_w = winui::combo_fit_width(&p, &crate::queue::TurnOff::LABELS, 119.0, 160.0);
        let combo_x = (winui::checkbox_caption_end(&p, 33.0, TURN_OFF) + 8.0)
            .max(267.0)
            .min(507.0 - combo_w);
        winui::checkbox_until(
            ui,
            o + vec2(33.0, 132.0),
            &mut self.turn_off_on,
            TURN_OFF,
            on,
            Some(o.x + combo_x),
        );
        winui::checkbox(
            ui,
            o + vec2(52.0, 150.0),
            &mut self.force,
            FORCE,
            on && self.turn_off_on,
        );
        if let Some(i) = winui::combo_light(
            ui,
            r(o, combo_x, 130.0, combo_w, 21.0),
            &crate::queue::TurnOff::LABELS,
            self.turn_off,
            on && self.turn_off_on,
            "progress-turn-off",
        ) {
            self.turn_off = i;
        }
        hide_tab(ui, o)
    }
}

/// "Hide tab" anchored at the bottom-right corner of the tab pane (`135 + RENAME_DROP`).
pub(super) fn hide_tab(ui: &mut Ui, o: Pos2) -> bool {
    // A longer translation ("Registerkarte ausblenden") widens it to the left, into the free
    // room right of the Remember caption (which ends before x 300).
    let rc = winui::button_rect_for(
        ui.painter(),
        r(o, 401.0, 135.0 + RENAME_DROP, 96.0, 21.0),
        "Hide tab",
        190.0,
    );
    winui::button(ui, rc, "Hide tab", ButtonKind::Normal, false, None).clicked()
}
