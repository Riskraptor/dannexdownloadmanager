//! Download complete: the finished download's box (Open, Open with..., Open folder, Close).

use super::*;

// ---- Download complete (client 455 × 215) --------------------------------------------------

pub struct Complete {
    pub id: u64,
    pub url: String,
    pub path: PathBuf,
    pub size: u64,
    pub dont_show: bool,
    /// What went wrong besides, in the interface language (a video's subtitles, #68): one line
    /// under the size.
    pub notices: Vec<String>,
    /// Clicked (or activated) once: from then on a window like any other, no longer above every
    /// program's (it shows above them until then, as IDM's does: #77).
    pub seen: bool,
    /// Open was pressed: the file is looked for and handed to its program off the window's
    /// thread (#743); the box closes when it opened, and stays (for Open folder) when it didn't.
    pub opening: Option<crate::launch::Asked<PathBuf, bool>>,
    /// How many other downloads' boxes wait behind this one (set by the app each frame): they
    /// show one at a time, at one place, so many downloads finishing together never cover the
    /// screen. Said under the buttons, with "Close all".
    pub more: usize,
    /// "Close all" was pressed: this box and the ones waiting behind it close together.
    pub close_all: bool,
}

impl Complete {
    /// The box for download `id`, saved at `path` from `url`, `size` bytes, with `notices` (what
    /// went wrong besides).
    pub fn new(id: u64, url: String, path: PathBuf, size: u64, notices: Vec<String>) -> Self {
        Complete {
            id,
            url,
            path,
            size,
            dont_show: false,
            notices,
            seen: false,
            opening: None,
            more: 0,
            close_all: false,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Outcome<()> {
        let id = format!("complete-{}", self.id);
        self.seen |= notice_seen(ctx, &id);
        notice(
            ctx,
            &id,
            "Download complete",
            [455.0, 222.0],
            true,
            winui::BG(),
            !self.seen,
            |ui, o| {
                let p = ui.painter().clone();
                // The system's open folder (IDM shows one here): the shared file icons'.
                crate::fileicons::paint_at(
                    ui,
                    &p,
                    crate::fileicons::Kind::FolderOpen,
                    o + vec2(10.0, 11.0),
                    32.0,
                );
                winui::text(
                    &p,
                    o.x + 51.0,
                    o.y + 21.0,
                    crate::i18n::t("Download complete"),
                    winui::TEXT(),
                );
                let line = crate::i18n::fmt(
                    crate::i18n::t("Downloaded {size} ({bytes} Bytes)"),
                    &[
                        ("size", &dialog_size(self.size)),
                        ("bytes", &crate::i18n::integer(self.size)),
                    ],
                );
                winui::text(&p, o.x + 51.0, o.y + 34.0, &line, winui::TEXT());
                // What didn't come with it (a video's subtitles the site refused), shortened to
                // the window and whole on hover.
                if !self.notices.is_empty() {
                    winui::fit(
                        ui,
                        &p,
                        o.x + 51.0,
                        o.y + 52.0,
                        &self.notices.join(" "),
                        winui::ERROR(),
                        o.x + 444.0,
                        ("complete-notice", self.id),
                    );
                }
                // i18n: noun: the web address (URL)
                winui::text(
                    &p,
                    o.x + 11.0,
                    o.y + 72.0,
                    crate::i18n::t("Address"),
                    winui::TEXT(),
                );
                // Read-only and copyable (an editable copy let typing seem to work, then reverted).
                winui::readonly_field(
                    ui,
                    r(o, 11.0, 78.0, 434.0, 23.0),
                    &self.url,
                    winui::BG(),
                    true,
                    "complete-url",
                );
                winui::text(
                    &p,
                    o.x + 11.0,
                    o.y + 117.0,
                    // i18n: Download complete: label before the saved file's path
                    crate::i18n::t("Saved to"),
                    winui::TEXT(),
                );
                winui::readonly_field(
                    ui,
                    r(o, 11.0, 124.0, 434.0, 23.0),
                    &display_path(&self.path),
                    winui::BG(),
                    true,
                    "complete-saved",
                );

                let btn = |ui: &mut Ui, (x, w): (f32, f32), label: &str, kind| {
                    winui::button(ui, r(o, x, 157.0, w, 21.0), label, kind, false, None)
                };
                // As wide as their labels in this language (#192): a longer one pushes those
                // after it right, Close last at the window's edge.
                let [open_at, with_at, folder_at, close_at] = winui::button_row_from_left(
                    &p,
                    &[
                        ("Open", 12.0, 90.0),
                        ("Open with...", 115.0, 90.0),
                        ("Open folder", 219.0, 90.0),
                        ("Close", 354.0, 90.0),
                    ],
                    13.0,
                    444.0,
                    200.0,
                )[..] else {
                    unreachable!("four buttons")
                };
                // i18n: verb: opens the downloaded file
                let open = btn(ui, open_at, "Open", ButtonKind::Default);
                let with = btn(ui, with_at, "Open with...", ButtonKind::Normal);
                let folder = btn(ui, folder_at, "Open folder", ButtonKind::Normal);
                // i18n: verb: closes this window
                let close = btn(ui, close_at, "Close", ButtonKind::Normal);
                // Others waiting behind this one (they show one at a time, at this place): how
                // many, and Close all, at the row's end before the drag handle.
                let mut close_all = false;
                let until = (self.more > 0).then(|| {
                    // i18n: link in Download complete while other downloads' boxes wait behind it: closes them all
                    let all = crate::i18n::mark("Close all");
                    let note = crate::i18n::tn(
                        self.more as u64,
                        // i18n: Download complete: how many other downloads finished, whose boxes wait behind this one
                        "{n} more download complete",
                        "{n} more downloads complete",
                    );
                    let link_x = 405.0 - winui::text_width(&p, crate::i18n::t(all)).ceil();
                    let note_x = link_x - 8.0 - winui::text_width(&p, &note).ceil();
                    winui::text(&p, o.x + note_x, o.y + 204.0, &note, winui::TEXT());
                    close_all = winui::link(ui, &p, o.x + link_x, o.y + 204.0, all, "complete-all");
                    o.x + note_x - 8.0
                });
                // Under Open, at the buttons' left edge.
                winui::checkbox_until(
                    ui,
                    o + vec2(12.0, 194.0),
                    &mut self.dont_show,
                    "Don't show this again",
                    true,
                    until,
                );

                // Drag handle: white box with a 1 px #828790 border; drag it to drop the file into
                // Explorer, a mail or a chat (IDM 5.05). Windows only: dragging a file out to
                // another app goes through OLE there; Linux and macOS have no such drag built
                // yet, so the handle isn't shown (Open folder is the way there).
                #[cfg(windows)]
                {
                    let handle = r(o, 417.0, 190.0, 27.0, 20.0);
                    let grip = ui
                        .interact(
                            handle,
                            egui::Id::unique(("drag-out", self.id)),
                            winui::pointer_drag(),
                        )
                        // i18n: tooltip of the handle in Download complete: drag it to drop the downloaded file somewhere
                        .on_hover_text(crate::i18n::t("Drag the file into a folder or an app"));
                    if grip.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                    // The file is looked for beside the window, with the drag (#743).
                    if grip.drag_started()
                        && let Err(e) = crate::win32::drag_file_out(
                            // The window's title as the system has it (dialog_window).
                            &crate::i18n::window_title(crate::i18n::t("Download complete")),
                            &self.path,
                        )
                    {
                        tracing::warn!("drag out: {e}");
                    }
                    p.rect_filled(handle, 0.0, winui::FIELD());
                    p.rect_stroke(
                        handle,
                        0.0,
                        egui::Stroke::new(1.0, winui::FIELD_EDGE()),
                        egui::StrokeKind::Inside,
                    );
                    crate::icons::Icon::DragOut.image(true).paint_at(
                        ui,
                        egui::Rect::from_center_size(handle.center(), vec2(16.0, 16.0)),
                    );
                }

                // Enter: Open, the default button (as in the other dialogs).
                // The keys of a drag out of the grip are the drag's (Escape cancels it), not
                // this window's (#561: the drag runs beside the window, which sees them too).
                #[cfg(windows)]
                let dragging = crate::win32::dragging_out();
                #[cfg(not(windows))]
                let dragging = false; // the grip is Windows-only
                // A file that can't open says why (the box stays for Open folder). Looked for and
                // handed over off the window's thread (#743): the box closes once it opened.
                let opened = self.opening.as_ref().and_then(|a| a.answer(&self.path));
                if opened.is_some() {
                    self.opening = None;
                }
                if (open.clicked() || (!dragging && winui::enter_pressed(ui)))
                    && self.opening.is_none()
                {
                    let opening = crate::launch::Asked::default();
                    let _ = opening.get(
                        &self.path,
                        std::time::Duration::MAX,
                        std::time::Duration::ZERO,
                        Some(ui.ctx()),
                        |p| open_file_now(p),
                    );
                    self.opening = Some(opening);
                }
                if close_all {
                    self.close_all = true;
                    Outcome::Done(())
                } else if opened == Some(true) {
                    Outcome::Done(())
                } else if with.clicked() {
                    open_with(&self.path);
                    Outcome::Done(())
                } else if folder.clicked() {
                    show_in_folder(&self.path);
                    Outcome::Done(())
                } else if close.clicked() || (!dragging && winui::escape_pressed(ui)) {
                    Outcome::Done(())
                } else {
                    Outcome::Open
                }
            },
        )
    }
}
