//! The Download status tab: the address, status, sizes and times, the speed graph, and the name
//! being renamed while the download runs.

use super::*;

/// Right end for text on the tab page (its frame is x 18..529).
pub(super) const PAGE_TEXT_RIGHT: f32 = 520.0;

impl Progress {
    // The rows sit 7 px inside the page's frame at the top and at the bottom (with Rename right
    // below Server), with a slightly wider break after Status, as before.
    pub(super) fn status_page(
        &mut self,
        ui: &mut Ui,
        o: Pos2,
        item: &Item,
        snap: Option<&Snapshot>,
        frac: Option<f64>,
    ) {
        let p = &ui.painter().clone();
        // Long values are cut with "..." at the page edge and shown whole on hover.
        let fit = |ui: &Ui, key: &str, x: f32, base: f32, s: &str, color| {
            let (rect, cut) =
                winui::text_fit(p, o.x + x, o.y + base, s, color, o.x + PAGE_TEXT_RIGHT);
            winui::copyable(ui, rect, key, s, cut);
        };
        let status = match item.status {
            // A torrent reading and checking its files says so, not "Receiving data" (#39).
            _ if let Some(checking) = item.torrent_checking() => checking,
            // A torrent its debrid service is downloading (#44).
            Status::Downloading if item.at_service().is_some() => {
                item.at_service().unwrap_or_default()
            }
            Status::Downloading => {
                // A video (yt-dlp) has no engine connections, nor a gallery while gallery-dl
                // downloads its files itself: data arriving is the sign.
                let receiving = snap.is_some_and(|s| {
                    s.connections
                        .iter()
                        .any(|c| matches!(c.status, ConnStatus::Receiving))
                }) || ((item.video.is_some()
                    || item.torrent.is_some()
                    || item.gallery.is_some())
                    && item.rate() > 0.0);
                if receiving {
                    crate::i18n::t("Receiving data...")
                } else {
                    crate::i18n::t("Connecting...")
                }
                .to_owned()
            }
            Status::Complete if item.missing => crate::i18n::t("File missing").to_owned(),
            // i18n: a download's status (state it is in, not a command): finished
            Status::Complete => crate::i18n::t("Complete").to_owned(),
            // i18n: a download's status (state it is in, not a command): stopped for now
            Status::Stopped => crate::i18n::t("Paused").to_owned(),
            // i18n: a download's status (state it is in, not a command): waiting in a queue
            Status::Queued => crate::i18n::t("Waiting").to_owned(),
            Status::Error => crate::i18n::fmt(
                // i18n: a download's status: it failed, with the reason
                crate::i18n::t("Error: {error}"),
                &[(
                    "error",
                    // i18n: the reason a download failed is not known
                    item.error.as_deref().unwrap_or(crate::i18n::t("unknown")),
                )],
            ),
        };
        fit(
            ui,
            "url",
            33.0,
            44.0,
            &crate::i18n::ltr(item.url.as_str()),
            winui::TEXT(),
        );
        let rows: [(&str, f32, f32, String); 6] = [
            // i18n: noun: label before the download's state
            (crate::i18n::mark("Status"), 61.0, 102.0, status),
            (
                crate::i18n::mark("File size"),
                80.0,
                135.0,
                item.size.map_or(String::new(), |s| {
                    crate::model::approx_size(size3(s), item.size_approx)
                }),
            ),
            (
                // i18n: label before the amount received so far
                crate::i18n::mark("Downloaded"),
                96.0,
                135.0,
                match frac {
                    // No size: only how much has come, no 0 % that never moves.
                    None => size3(item.downloaded),
                    // The share in brackets, as the language writes a percentage.
                    Some(frac) => format!(
                        "{} ({})",
                        size3(item.downloaded),
                        crate::i18n::percent(frac, 2)
                    ),
                },
            ),
            (
                crate::i18n::mark("Transfer rate"),
                112.0,
                135.0,
                if item.status == Status::Downloading {
                    model::rate(item.rate())
                } else {
                    String::new()
                },
            ),
            (
                crate::i18n::mark("Time left"),
                128.0,
                135.0,
                item.time_left(),
            ),
            (
                // i18n: label: whether the server lets a stopped download continue (Yes / No)
                crate::i18n::mark("Can resume"),
                145.0,
                135.0,
                match item.resumable {
                    Some(true) => crate::i18n::t("Yes"),
                    Some(false) => crate::i18n::t("No"),
                    None => "",
                }
                .to_owned(),
            ),
        ];
        // A value starts 6 px after its label at the least (a long translation); the rows that share
        // the column at 135 (all but Status) keep sharing one.
        let after = |label: &str| 33.0 + winui::text_width(p, crate::i18n::t(label)).ceil() + 6.0;
        let column = rows
            .iter()
            .filter(|r| r.2 == 135.0)
            .map(|r| after(r.0))
            .fold(135.0, f32::max);
        for (label, base, vx, value) in rows {
            let vx = if vx == 135.0 {
                column
            } else {
                vx.max(after(label))
            };
            text(
                p,
                o.x + 33.0,
                o.y + base,
                crate::i18n::t(label),
                winui::TEXT(),
            );
            // Light mode shows the status value in pure blue (light-dialogs.md).
            let vc = if label == "Status" {
                crate::theme::colors().status_value
            } else {
                winui::TEXT()
            };
            fit(ui, label, vx, base, &value, vc);
        }
        // Where its server is (#140): a file download's, and a video's whose streams DDM's engine
        // downloads (#769; a torrent or a gallery has no single server of DDM's own). As the
        // running download sees it, else as its last run did.
        let seen = snap
            .and_then(|s| s.server.as_ref())
            .map(crate::whereis::Seen::from)
            .or_else(|| item.server.clone());
        // A video yt-dlp downloads itself (#769): why, in that row.
        if let Some(why) = item.video_helper() {
            fit(ui, "helper", 33.0, 161.0, &helper_line(&why), winui::TEXT());
        } else if let Some(files) = item.gallery_files() {
            // A gallery's files (#769): how many are done.
            fit(
                ui,
                "files",
                33.0,
                161.0,
                &gallery_line(&files),
                winui::TEXT(),
            );
        } else if item.torrent.is_none()
            && item.gallery.is_none()
            && (item.video.is_none() || seen.is_some())
        {
            // i18n: label before where a download's server is (its country and round trip)
            let label = crate::i18n::t("Server");
            text(p, o.x + 33.0, o.y + 161.0, label, winui::TEXT());
            crate::whereis::line(
                ui,
                o.x + column.max(33.0 + winui::text_width(p, label).ceil() + 6.0),
                o.y + 161.0,
                o.x + PAGE_TEXT_RIGHT,
                seen.as_ref(),
                &format!("progress-{}", item.id),
            );
        }
        self.rename_row(ui, p, o, column, item);
    }

    pub(super) fn rename_row(
        &mut self,
        ui: &mut Ui,
        p: &egui::Painter,
        o: Pos2,
        column: f32,
        item: &Item,
    ) {
        let top = 171.0;
        let base = top + 13.0;
        // i18n: noun: label before the deferred new file or folder name in the progress window
        let rename_label = crate::i18n::t("Rename");
        // i18n: verb: button that saves the deferred rename in the progress window
        let confirm_key = crate::i18n::mark("Confirm");
        // i18n: status shown in green beside the Rename box once the new name is saved
        let saved_text = crate::i18n::t("Saved");

        text(p, o.x + 33.0, o.y + base, rename_label, winui::TEXT());

        let saved_w = winui::text_width(p, saved_text).ceil().max(38.0);
        let btn_w = (winui::text_width(p, crate::i18n::t(confirm_key)) + 20.0)
            .ceil()
            .max(52.0);
        let saved_x = PAGE_TEXT_RIGHT - saved_w;
        let btn_x = saved_x - 8.0 - btn_w;
        let edit_x = column.max(33.0 + winui::text_width(p, rename_label).ceil() + 6.0);
        let edit_w = (btn_x - 6.0 - edit_x).max(100.0);

        let disabled_reason = (item.status != Status::Complete)
            .then_some(self.rename_disabled_reason)
            .flatten();
        let enabled = item.status != Status::Complete && disabled_reason.is_none();
        let edit_rc = r(o, edit_x, top, edit_w, 21.0);
        let edit_resp = winui::edit(ui, edit_rc, &mut self.rename_text, enabled, false);
        if edit_resp.changed() {
            self.on_rename_text_changed(std::time::Instant::now());
            if let Some(wait) = self.poll_rename_autosave(std::time::Instant::now()) {
                ui.ctx()
                    .request_repaint_after(wait + std::time::Duration::from_millis(15));
            }
        }

        let validated = crate::deferred_rename::validate_name(&self.rename_text);
        let error_reason = if enabled {
            validated.as_ref().err().copied().map(rename_error_message)
        } else {
            None
        };
        if error_reason.is_some() {
            winui::error_frame(p, edit_rc);
        }

        let dirty = match &validated {
            Ok(norm) => *norm != self.rename_saved,
            Err(_) => false,
        };
        let can_confirm = enabled
            && validated.is_ok()
            && (dirty || (!self.rename_text.trim().is_empty() && !self.rename_show_saved));
        let btn_kind = if can_confirm {
            ButtonKind::Normal
        } else {
            ButtonKind::Disabled
        };
        let btn_resp = winui::button(
            ui,
            r(o, btn_x, top, btn_w, 21.0),
            confirm_key,
            btn_kind,
            false,
            None,
        );
        let clicked = btn_resp.clicked();
        let enter = edit_resp.has_focus() && winui::take_key(ui, egui::Key::Enter);
        let assistive = winui::value_set_by_assistive(&edit_resp);

        if let Some(reason_key) = error_reason.or(disabled_reason) {
            let why = crate::i18n::t(reason_key);
            ui.interact(
                edit_rc,
                egui::Id::unique(("progress-rename-reason", self.id)),
                egui::Sense::hover(),
            )
            .on_hover_text(why);
            edit_resp
                .clone()
                .on_hover_text(why)
                .on_disabled_hover_text(why);
            btn_resp.on_hover_text(why);
            if error_reason.is_some() && edit_resp.has_focus() {
                egui::Tooltip::for_widget(&edit_resp).show(|ui| {
                    ui.label(why);
                });
            }
        }

        if enabled
            && (clicked || enter || assistive)
            && let Ok(norm) = validated
            && (dirty || norm.is_some())
        {
            self.commit_rename(norm);
        }

        if enabled && self.rename_show_saved {
            text(p, o.x + saved_x, o.y + base, saved_text, winui::SUCCESS());
        }
    }
}

pub(super) fn rename_error_message(err: crate::deferred_rename::RenameError) -> &'static str {
    use crate::deferred_rename::RenameError;
    match err {
        // i18n: reason shown on the Rename box when the entered name ends with a period
        RenameError::TrailingDot => crate::i18n::mark("A file name cannot end with a dot"),
        // i18n: reason shown on the Rename box when the entered name exceeds the length limit
        RenameError::TooLong => crate::i18n::mark("The name is too long"),
        // i18n: reason shown on the Rename box when the entered name has a forbidden character
        RenameError::InvalidChar => {
            crate::i18n::mark("This character is not allowed in a file name")
        }
        // i18n: reason shown on the Rename box when the entered name is a reserved device name on Windows (CON, NUL, COM1…)
        RenameError::ReservedDevice => crate::i18n::mark("This name is reserved by Windows"),
    }
}

/// How much lower a torrent's details sit than a download's (and how much taller its window
/// is): its tab strip, where the connections caption is, would touch the buttons above.
pub(super) fn torrent_drop(item: &Item) -> f32 {
    if item.torrent.is_some() { 6.0 } else { 0.0 }
}
